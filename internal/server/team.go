package server

import (
	"crypto/sha256"
	"encoding/hex"
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
	stale, err := s.memberStale(cfg)
	if err != nil {
		return nil, err
	}
	userStale, err := s.userStale(cfg)
	if err != nil {
		return nil, err
	}
	return publicTeam(cfg, stale, userStale), nil
}

func publicTeam(cfg store.TeamFolder, stale, userStale bool) map[string]any {
	return map[string]any{
		"path":      cfg.Path,
		"user":      cfg.User,
		"member":    cfg.Member,
		"stale":     stale,
		"userStale": userStale,
	}
}

// memberStale reports that the selected member's agents differ from .kiki/agents.
func (s *Server) memberStale(cfg store.TeamFolder) (bool, error) {
	path := strings.TrimSpace(cfg.Path)
	user := strings.TrimSpace(cfg.User)
	name := strings.TrimSpace(cfg.Member)
	if path == "" || user == "" || name == "" {
		return false, nil
	}
	member, err := teamfolder.Member(path, name)
	if err != nil || !teamfolder.HasAgents(member) {
		return true, nil
	}
	local, err := s.agentDir()
	if err != nil {
		return false, err
	}
	same, err := teamfolder.Same(teamfolder.AgentsDir(member), local)
	if err != nil {
		return false, err
	}
	return !same, nil
}

// userStale reports that .kiki/agents differs from this user's agents folder.
func (s *Server) userStale(cfg store.TeamFolder) (bool, error) {
	path := strings.TrimSpace(cfg.Path)
	user := strings.TrimSpace(cfg.User)
	if path == "" || user == "" {
		return false, nil
	}
	userDir, err := teamfolder.UserDir(path, user)
	if err != nil {
		return false, err
	}
	local, err := s.agentDir()
	if err != nil {
		return false, err
	}
	if !teamfolder.HasAgents(userDir) {
		return true, nil
	}
	same, err := teamfolder.Same(local, teamfolder.AgentsDir(userDir))
	if err != nil {
		return false, err
	}
	return !same, nil
}

// memberDiffers lists members whose agents folder is not the same as .kiki/agents.
func (s *Server) memberDiffers(dir string, names []string) ([]string, error) {
	local, err := s.agentDir()
	if err != nil {
		return nil, err
	}
	differs := []string{}
	for _, name := range names {
		member := filepath.Join(dir, name)
		if !teamfolder.HasAgents(member) {
			differs = append(differs, name)
			continue
		}
		same, err := teamfolder.Same(teamfolder.AgentsDir(member), local)
		if err != nil {
			return nil, err
		}
		if !same {
			differs = append(differs, name)
		}
	}
	return differs, nil
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
		Path        string                  `json:"path"`
		User        string                  `json:"user"`
		Direction   string                  `json:"direction"`
		UseExisting bool                    `json:"useExisting"`
		Repos       map[string]repoDecision `json:"repos"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	path := strings.TrimSpace(body.Path)
	if path != "" && !teamfolder.ValidName(body.User) {
		writeErr(w, http.StatusBadRequest, errors.New("Enter your name."))
		return
	}
	if !s.beginTeam(w) {
		return
	}
	defer s.teamMu.Unlock()
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
	taken, err := s.nameTaken(userDir)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if taken && !body.UseExisting {
		writeJSON(w, http.StatusConflict, map[string]any{
			"error":    "That name is already in the team folder.",
			"conflict": "name",
			"user":     filepath.Base(userDir),
		})
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

// nameTaken reports that userDir is already a member folder for someone else.
func (s *Server) nameTaken(userDir string) (bool, error) {
	saved, err := s.Store.TeamFolder()
	if err != nil {
		return false, err
	}
	dir := filepath.Dir(userDir)
	name := filepath.Base(userDir)
	if filepath.Clean(saved.Path) == dir && strings.TrimSpace(saved.User) == name {
		return false, nil
	}
	names, err := teamfolder.Members(dir)
	if err != nil {
		return false, err
	}
	for _, member := range names {
		if member == name {
			return true, nil
		}
	}
	return false, nil
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
	differs := []string{}
	if strings.TrimSpace(r.URL.Query().Get("path")) == "" {
		differs, err = s.memberDiffers(dir, names)
		if err != nil {
			writeErr(w, http.StatusInternalServerError, err)
			return
		}
	}
	writeJSON(w, http.StatusOK, map[string]any{"members": names, "differs": differs, "path": dir})
}

type teamAgentView struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Category string `json:"category"`
	Hash     string `json:"hash"`
}

func agentViews(dir string) ([]teamAgentView, error) {
	files, err := agentfile.List(dir)
	if err != nil {
		return nil, err
	}
	views := make([]teamAgentView, 0, len(files))
	for _, file := range files {
		name := strings.TrimSpace(file.Name)
		if name == "" {
			name = file.ID
		}
		raw, err := os.ReadFile(file.Path)
		if err != nil {
			return nil, err
		}
		sum := sha256.Sum256(raw)
		views = append(views, teamAgentView{
			ID:       file.ID,
			Name:     name,
			Category: file.Category,
			Hash:     hex.EncodeToString(sum[:]),
		})
	}
	return views, nil
}

func compareFolder(local, memberDir, name string) (map[string]any, error) {
	views := []teamAgentView{}
	differs := true
	if teamfolder.HasAgents(memberDir) {
		var err error
		views, err = agentViews(teamfolder.AgentsDir(memberDir))
		if err != nil {
			return nil, err
		}
		same, err := teamfolder.Same(local, teamfolder.AgentsDir(memberDir))
		if err != nil {
			return nil, err
		}
		differs = !same
	}
	return map[string]any{"name": name, "differs": differs, "agents": views}, nil
}

func (s *Server) handleTeamCompare(w http.ResponseWriter, r *http.Request) {
	if !s.beginTeam(w) {
		return
	}
	defer s.teamMu.Unlock()
	cfg, err := s.Store.TeamFolder()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if strings.TrimSpace(cfg.Path) == "" || strings.TrimSpace(cfg.User) == "" {
		writeErr(w, http.StatusBadRequest, errors.New("Set up the team folder in Settings first."))
		return
	}
	local, err := s.agentDir()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	app, err := agentViews(local)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	userDir, err := teamfolder.UserDir(cfg.Path, cfg.User)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	user, err := compareFolder(local, userDir, cfg.User)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	names, err := teamfolder.Members(cfg.Path)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	memberName := strings.TrimSpace(r.URL.Query().Get("member"))
	member := map[string]any{"name": "", "differs": false, "agents": []teamAgentView{}}
	if memberName != "" {
		memberDir, err := teamfolder.Member(cfg.Path, memberName)
		if err != nil {
			member = map[string]any{"name": memberName, "differs": false, "missing": true, "agents": []teamAgentView{}}
		} else {
			member, err = compareFolder(local, memberDir, memberName)
			if err != nil {
				writeErr(w, http.StatusInternalServerError, err)
				return
			}
		}
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"members": names,
		"app":     app,
		"user":    user,
		"member":  member,
	})
}

func (s *Server) handleTeamDiff(w http.ResponseWriter, r *http.Request) {
	if !s.beginTeam(w) {
		return
	}
	defer s.teamMu.Unlock()
	folder := strings.TrimSpace(r.URL.Query().Get("folder"))
	id := strings.TrimSpace(r.URL.Query().Get("id"))
	if folder == "" || id == "" {
		writeErr(w, http.StatusBadRequest, errors.New("Choose an agent file to compare."))
		return
	}
	cfg, err := s.Store.TeamFolder()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if strings.TrimSpace(cfg.Path) == "" || strings.TrimSpace(cfg.User) == "" {
		writeErr(w, http.StatusBadRequest, errors.New("Set up the team folder in Settings first."))
		return
	}
	memberDir, err := teamfolder.Member(cfg.Path, folder)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	local, err := s.agentDir()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	appText, appName, appOK, err := agentFileText(local, id)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	folderText, folderName, folderOK, err := agentFileText(teamfolder.AgentsDir(memberDir), id)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if !appOK && !folderOK {
		writeErr(w, http.StatusNotFound, errors.New("That agent is not in this app or that folder."))
		return
	}
	name := appName
	if name == "" {
		name = folderName
	}
	if name == "" {
		name = id
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"id":     id,
		"name":   name,
		"folder": folder,
		"diff":   console.TextDiff(appText, folderText),
	})
}

func agentFileText(dir, id string) (string, string, bool, error) {
	file, ok, err := agentfile.Find(dir, id)
	if err != nil || !ok {
		return "", "", false, err
	}
	raw, err := os.ReadFile(file.Path)
	if err != nil {
		return "", "", false, err
	}
	name := strings.TrimSpace(file.Name)
	if name == "" {
		name = file.ID
	}
	// Category is the folder, not a line in the file. Prefix it so a move still diffs.
	text := "category: " + file.Category + "\n" + string(raw)
	return text, name, true, nil
}

func (s *Server) handleTeamMember(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Name      string                  `json:"name"`
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
	cfg, err := s.Store.TeamFolder()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if strings.TrimSpace(cfg.Path) == "" || strings.TrimSpace(cfg.User) == "" {
		writeErr(w, http.StatusBadRequest, errors.New("Set up the team folder in Settings first."))
		return
	}
	member, err := teamfolder.Member(cfg.Path, body.Name)
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
	if direction == "" {
		direction = "pull"
	}
	switch direction {
	case "pull":
		if !teamfolder.HasAgents(member) {
			writeErr(w, http.StatusBadRequest, errors.New("That member folder has no agents folder."))
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
	case "push":
		if err := teamfolder.Copy(local, teamfolder.AgentsDir(member)); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
	default:
		writeErr(w, http.StatusBadRequest, errors.New("Choose how to copy the agents."))
		return
	}
	cfg.Member = filepath.Base(member)
	if err := s.Store.SaveTeamFolder(cfg); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handleSaveUserAgents(w http.ResponseWriter, r *http.Request) {
	if !s.beginTeam(w) {
		return
	}
	defer s.teamMu.Unlock()
	cfg, err := s.Store.TeamFolder()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if strings.TrimSpace(cfg.Path) == "" || strings.TrimSpace(cfg.User) == "" {
		writeErr(w, http.StatusBadRequest, errors.New("Set up the team folder in Settings first."))
		return
	}
	local, err := s.agentDir()
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if err := s.copyLocalAgentsToUser(cfg, local); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) copyLocalAgentsToUser(cfg store.TeamFolder, local string) error {
	userDir, err := teamfolder.UserDir(cfg.Path, cfg.User)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(userDir, 0o755); err != nil {
		return err
	}
	return teamfolder.Copy(local, teamfolder.AgentsDir(userDir))
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
