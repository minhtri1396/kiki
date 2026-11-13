package server

import (
	"bytes"
	"errors"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"

	"kiki/internal/agentfile"
	"kiki/internal/console"
	"kiki/internal/gitignore"
	"kiki/internal/store"
	"kiki/internal/teamfolder"
	"kiki/internal/toolchain"
)

// chooseFolder opens the system folder dialog. Tests replace it.
var chooseFolder = systemChooseFolder

type repoDecision struct {
	Action string `json:"action"`
	ID     string `json:"id"`
}

type teamConflict struct {
	payload map[string]any
}

func (s *Server) beginTeam(w http.ResponseWriter) bool {
	if !s.teamMu.TryLock() {
		writeErr(w, http.StatusConflict, errors.New("Team folder setup is already running."))
		return false
	}
	return true
}

func (s *Server) teamStatus() (map[string]any, error) {
	cfg, err := s.Store.TeamFolder()
	if err != nil {
		return nil, err
	}
	return publicTeam(cfg), nil
}

func publicTeam(cfg store.TeamFolder) map[string]any {
	return map[string]any{
		"path":   cfg.Path,
		"user":   cfg.User,
		"member": cfg.Member,
	}
}

func (s *Server) handleTeamBrowse(w http.ResponseWriter, r *http.Request) {
	if !s.beginTeam(w) {
		return
	}
	defer s.teamMu.Unlock()
	picked, err := chooseFolder()
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	dir, err := teamfolder.Dir(picked)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"path": dir})
}

func (s *Server) handleTeamSave(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Path      string                  `json:"path"`
		User      string                  `json:"user"`
		Direction string                  `json:"direction"`
		Repos     map[string]repoDecision `json:"repos"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	if !s.beginTeam(w) {
		return
	}
	defer s.teamMu.Unlock()
	path := strings.TrimSpace(body.Path)
	if path == "" {
		if err := s.Store.SaveTeamFolder(store.TeamFolder{}); err != nil {
			writeErr(w, http.StatusInternalServerError, err)
			return
		}
		s.handleState(w, r)
		return
	}
	userDir, err := teamfolder.UserDir(path, body.User)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	local, err := s.agentDir()
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	direction := strings.TrimSpace(body.Direction)
	exists := teamfolder.HasAgents(userDir)
	switch {
	case exists && direction == "":
		writeJSON(w, http.StatusConflict, map[string]any{
			"error":    "This user already has an agents folder.",
			"conflict": "agents",
			"path":     filepath.Dir(userDir),
			"user":     filepath.Base(userDir),
		})
		return
	case direction == "pull":
		if !exists {
			writeErr(w, http.StatusBadRequest, errors.New("That user folder has no agents folder."))
			return
		}
		conflict, err := s.importAgents(teamfolder.AgentsDir(userDir), local, body.Repos)
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		if conflict != nil {
			writeJSON(w, http.StatusConflict, conflict.payload)
			return
		}
	case direction == "" || direction == "push":
		if err := os.MkdirAll(userDir, 0o755); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		if err := teamfolder.Copy(local, teamfolder.AgentsDir(userDir)); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
	default:
		writeErr(w, http.StatusBadRequest, errors.New("Choose how to copy the agents."))
		return
	}
	user := filepath.Base(userDir)
	if err := s.Store.SaveTeamFolder(store.TeamFolder{
		Path:   filepath.Dir(userDir),
		User:   user,
		Member: user,
	}); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handleTeamMembers(w http.ResponseWriter, r *http.Request) {
	if !s.beginTeam(w) {
		return
	}
	defer s.teamMu.Unlock()
	path := strings.TrimSpace(r.URL.Query().Get("path"))
	if path == "" {
		cfg, err := s.Store.TeamFolder()
		if err != nil {
			writeErr(w, http.StatusInternalServerError, err)
			return
		}
		path = cfg.Path
	}
	dir, err := teamfolder.Dir(path)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	names, err := teamfolder.Members(dir)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"members": names, "path": dir})
}

func (s *Server) handleTeamMember(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Name  string                  `json:"name"`
		Repos map[string]repoDecision `json:"repos"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	if !s.beginTeam(w) {
		return
	}
	defer s.teamMu.Unlock()
	cfg, err := s.Store.TeamFolder()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	member, err := teamfolder.Member(cfg.Path, body.Name)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if !teamfolder.HasAgents(member) {
		writeErr(w, http.StatusBadRequest, errors.New("That member folder has no agents folder."))
		return
	}
	local, err := s.agentDir()
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	conflict, err := s.importAgents(teamfolder.AgentsDir(member), local, body.Repos)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if conflict != nil {
		writeJSON(w, http.StatusConflict, conflict.payload)
		return
	}
	cfg.Member = filepath.Base(member)
	if err := s.Store.SaveTeamFolder(cfg); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	s.handleState(w, r)
}

// importAgents replaces dest with the agent files in source.
// A repository id the app does not have is returned as a conflict until the
// request chooses to create it or to use an existing repository.
func (s *Server) importAgents(source, dest string, decisions map[string]repoDecision) (*teamConflict, error) {
	snap, err := s.load()
	if err != nil {
		return nil, err
	}
	known := map[string]bool{}
	repoIDs := make([]string, 0, len(snap.Repos))
	for _, repo := range snap.Repos {
		known[repo.Kind] = true
		repoIDs = append(repoIDs, repo.Kind)
	}
	files, err := agentfile.List(source)
	if err != nil {
		return nil, err
	}
	seen := map[string]bool{}
	unknown := []string{}
	for _, file := range files {
		for _, repo := range file.Repos {
			kind := strings.TrimSpace(repo.Kind)
			if kind == "" || known[kind] || seen[kind] {
				continue
			}
			seen[kind] = true
			unknown = append(unknown, kind)
		}
	}
	sort.Strings(unknown)
	replacements := map[string]string{}
	create := []string{}
	if len(unknown) > 0 {
		missing := []string{}
		for _, id := range unknown {
			choice, ok := decisions[id]
			action := strings.TrimSpace(choice.Action)
			if !ok || (action != "create" && action != "use") {
				missing = append(missing, id)
				continue
			}
			if action == "create" {
				if !toolchain.RepoID(id) {
					return nil, fmt.Errorf("repository %s cannot be created", id)
				}
				create = append(create, id)
				continue
			}
			target := strings.TrimSpace(choice.ID)
			if !known[target] {
				return nil, fmt.Errorf("choose an existing repository for %s", id)
			}
			replacements[id] = target
		}
		if len(missing) > 0 {
			return &teamConflict{payload: map[string]any{
				"error":   "Choose a repository for each one this app does not have.",
				"unknown": missing,
				"repos":   repoIDs,
			}}, nil
		}
	}
	if err := teamfolder.Copy(source, dest); err != nil {
		return nil, err
	}
	if err := agentfile.ReplaceRepos(dest, replacements); err != nil {
		return nil, err
	}
	if err := s.ensureRepos(create); err != nil {
		return nil, err
	}
	return nil, nil
}

func (s *Server) ensureRepos(ids []string) error {
	if len(ids) == 0 {
		return nil
	}
	snap, err := s.load()
	if err != nil {
		return err
	}
	existing := map[string]bool{}
	for _, repo := range snap.Repos {
		existing[repo.Kind] = true
	}
	added := []string{}
	for _, id := range ids {
		if existing[id] {
			continue
		}
		if !toolchain.RepoID(id) {
			return fmt.Errorf("repository %s cannot be created", id)
		}
		snap.Repos = append(snap.Repos, console.Repo{Kind: id})
		existing[id] = true
		added = append(added, id)
	}
	if len(added) == 0 {
		return nil
	}
	snap, err = console.ApplyRoot(s.Root, snap)
	if err != nil {
		return err
	}
	if err := console.Validate(snap); err != nil {
		return err
	}
	if err := s.Store.Save(snap); err != nil {
		return err
	}
	entries := make([]gitignore.Entry, 0, len(added))
	for _, id := range added {
		entries = append(entries, gitignore.Entry{Path: filepath.Join(s.Root, id), Dir: true})
	}
	_, err = gitignore.Ensure(s.Root, entries)
	return err
}

func (s *Server) userAgentsDir() (string, bool, error) {
	cfg, err := s.Store.TeamFolder()
	if err != nil {
		return "", false, err
	}
	if strings.TrimSpace(cfg.Path) == "" || strings.TrimSpace(cfg.User) == "" {
		return "", false, nil
	}
	userDir, err := teamfolder.UserDir(cfg.Path, cfg.User)
	if err != nil || !teamfolder.HasAgents(userDir) {
		return "", false, nil
	}
	return teamfolder.AgentsDir(userDir), true, nil
}

func agentTeamPath(localDir, localPath, teamDir string) (string, error) {
	rel, err := filepath.Rel(localDir, localPath)
	if err != nil {
		return "", errors.New("The agent file is not available.")
	}
	rel = filepath.Clean(rel)
	if rel == "." || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", errors.New("The agent file is not available.")
	}
	dest := filepath.Join(teamDir, rel)
	back, err := filepath.Rel(teamDir, dest)
	if err != nil || back == ".." || strings.HasPrefix(back, ".."+string(filepath.Separator)) {
		return "", errors.New("The agent file is not available.")
	}
	return dest, nil
}

func readAgentBytes(path string) ([]byte, bool, error) {
	info, err := os.Lstat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, false, nil
		}
		return nil, false, err
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.Mode().IsRegular() {
		return nil, false, errors.New("The agent file is not available.")
	}
	if info.Size() > 8<<20 {
		return nil, false, errors.New("The agent file is larger than 8MB.")
	}
	body, err := os.ReadFile(path)
	if err != nil {
		return nil, false, err
	}
	return body, true, nil
}

func agentFilesMatch(localDir, localPath, teamDir string) (bool, error) {
	teamPath, err := agentTeamPath(localDir, localPath, teamDir)
	if err != nil {
		return false, err
	}
	localBody, localOK, err := readAgentBytes(localPath)
	if err != nil || !localOK {
		if err != nil {
			return false, err
		}
		return false, errors.New("The agent file is not available.")
	}
	teamBody, teamOK, err := readAgentBytes(teamPath)
	if err != nil {
		return false, err
	}
	return teamOK && bytes.Equal(localBody, teamBody), nil
}

func (s *Server) handleTeamAgentDiff(w http.ResponseWriter, r *http.Request) {
	file, ok := s.lookupAgent(w, r.PathValue("id"))
	if !ok {
		return
	}
	localDir, err := s.agentDir()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	teamDir, ready, err := s.userAgentsDir()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if !ready {
		writeErr(w, http.StatusBadRequest, errors.New("Choose your folder in the team folder first."))
		return
	}
	teamPath, err := agentTeamPath(localDir, file.Path, teamDir)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	localBody, _, err := readAgentBytes(file.Path)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	teamBody, teamOK, err := readAgentBytes(teamPath)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if r.Method == http.MethodGet {
		writeJSON(w, http.StatusOK, map[string]any{
			"id":          file.ID,
			"title":       file.Name,
			"teamMissing": !teamOK,
			"diff":        console.TextDiff(string(localBody), string(teamBody)),
		})
		return
	}
	var body struct {
		Keep string `json:"keep"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	switch strings.TrimSpace(body.Keep) {
	case "local":
		if err := os.MkdirAll(filepath.Dir(teamPath), 0o755); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		if err := os.WriteFile(teamPath, localBody, 0o644); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
	case "team":
		if !teamOK {
			if err := os.Remove(file.Path); err != nil && !os.IsNotExist(err) {
				writeErr(w, http.StatusBadRequest, err)
				return
			}
		} else if err := os.WriteFile(file.Path, teamBody, 0o644); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
	default:
		writeErr(w, http.StatusBadRequest, errors.New("Choose which copy to keep."))
		return
	}
	s.handleState(w, r)
}

func systemChooseFolder() (string, error) {
	if runtime.GOOS != "darwin" {
		return "", errors.New("Enter the full path of the team folder.")
	}
	out, err := exec.Command("osascript", "-e", `POSIX path of (choose folder with prompt "Choose the team folder")`).Output()
	if err != nil {
		return "", errors.New("Folder selection was cancelled.")
	}
	path := strings.TrimRight(strings.TrimSpace(string(out)), "/")
	if path == "" {
		return "", errors.New("Folder selection was cancelled.")
	}
	return path, nil
}
