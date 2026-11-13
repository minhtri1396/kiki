// Package server is the local window for KIKI.
package server

import (
	"context"
	"embed"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"kiki/internal/agentdef"
	"kiki/internal/agentfile"
	"kiki/internal/console"
	"kiki/internal/jobs"
	"kiki/internal/orca"
	"kiki/internal/store"
	"kiki/internal/toolchain"
)

//go:embed web/index.html web/app.css web/app.js
var webFS embed.FS

// Server serves the console.
type Server struct {
	Root  string
	Store *store.Store
	Orca  *orca.Client
	Jobs  *jobs.Manager

	modelsMu    sync.Mutex
	models      map[string][]console.Model
	modelsAt    time.Time
	cursorReady bool
	claudeReady bool

	teamMu sync.Mutex
}

// Handler routes the console.
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/state", s.handleState)
	mux.HandleFunc("PUT /api/setup", s.handleSave)
	mux.HandleFunc("GET /api/tools/{tool}/models", s.handleToolModels)
	mux.HandleFunc("POST /api/generate", s.handleGenerate)
	mux.HandleFunc("POST /api/generate/preview", s.handleGeneratePreview)
	mux.HandleFunc("POST /api/repos/services", s.handleServices)
	mux.HandleFunc("POST /api/repos/{kind}/clone", s.handleClone)
	mux.HandleFunc("GET /api/agents/{id}", s.handleAgent)
	mux.HandleFunc("PUT /api/agents/{id}", s.handleSaveAgent)
	mux.HandleFunc("POST /api/agents/{id}/preview", s.handlePreview)
	mux.HandleFunc("GET /api/agents/{id}/team-diff", s.handleTeamAgentDiff)
	mux.HandleFunc("POST /api/agents/{id}/team-diff", s.handleTeamAgentDiff)
	mux.HandleFunc("POST /api/agents/{id}/launch", s.handleLaunch)
	mux.HandleFunc("DELETE /api/launches/{id}", s.handleDeleteLaunch)
	mux.HandleFunc("DELETE /api/launches", s.handleClearLaunches)
	mux.HandleFunc("POST /api/indexes/check", s.handleCheck)
	mux.HandleFunc("POST /api/indexes/check/{kind}", s.handleCheckOne)
	mux.HandleFunc("POST /api/indexes/refresh/{index}", s.handleRefreshIndex)
	mux.HandleFunc("POST /api/open", s.handleOpen)
	mux.HandleFunc("DELETE /api/indexes/{kind}", s.handleClearIndex)
	mux.HandleFunc("POST /api/indexes/obsidian", s.handleObsidian)
	mux.HandleFunc("POST /api/indexes/codegraph", s.handleCodegraph)
	mux.HandleFunc("POST /api/indexes/codegraph/{kind}", s.handleCodegraph)
	mux.HandleFunc("GET /api/jobs", s.handleJobs)
	mux.HandleFunc("GET /api/jobs/{id}", s.handleJob)
	mux.HandleFunc("POST /api/jobs/{id}/cancel", s.handleCancel)
	mux.HandleFunc("POST /api/team/browse", s.handleTeamBrowse)
	mux.HandleFunc("PUT /api/team", s.handleTeamSave)
	mux.HandleFunc("GET /api/team/members", s.handleTeamMembers)
	mux.HandleFunc("POST /api/team/member", s.handleTeamMember)
	mux.HandleFunc("GET /", s.handlePage)
	mux.HandleFunc("GET /app.css", s.handleAsset)
	mux.HandleFunc("GET /app.js", s.handleAsset)
	return mux
}

func (s *Server) handleState(w http.ResponseWriter, r *http.Request) {
	state, err := s.snapshot()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, state)
}

func (s *Server) handleSave(w http.ResponseWriter, r *http.Request) {
	var body struct {
		WorkspaceID   string `json:"workspaceId"`
		OrcaAgent     string `json:"orcaAgent"`
		GenerateTool  string `json:"generateTool"`
		GenerateModel string `json:"generateModel"`
		Repos         []struct {
			Kind     string   `json:"kind"`
			Folder   string   `json:"folder"`
			Services []string `json:"services"`
		} `json:"repos"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	byKind := map[string]console.Repo{}
	for _, repo := range body.Repos {
		services := make([]string, 0, len(repo.Services))
		for _, name := range repo.Services {
			name = strings.TrimSpace(name)
			if name == "" {
				continue
			}
			services = append(services, name)
		}
		byKind[repo.Kind] = console.Repo{Kind: repo.Kind, Folder: strings.TrimSpace(repo.Folder), Services: services}
	}
	snap := console.Snapshot{
		Root:          s.Root,
		WorkspaceID:   strings.TrimSpace(body.WorkspaceID),
		OrcaAgent:     strings.TrimSpace(body.OrcaAgent),
		GenerateTool:  strings.TrimSpace(body.GenerateTool),
		GenerateModel: strings.TrimSpace(body.GenerateModel),
	}
	for _, kind := range toolchain.Kinds {
		repo, ok := byKind[kind]
		if !ok {
			repo = console.Repo{Kind: kind}
		}
		snap.Repos = append(snap.Repos, repo)
	}
	for _, kind := range extraRepoKinds(byKind) {
		snap.Repos = append(snap.Repos, byKind[kind])
	}
	current, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	for i := range snap.Repos {
		kept := current.Repo(snap.Repos[i].Kind)
		snap.Repos[i].Index = kept.Index
		snap.Repos[i].Sparse = kept.Sparse
	}
	snap, err = console.ApplyRoot(s.Root, snap)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if err := console.Validate(snap); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if err := s.Store.Save(snap); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if err := s.syncAgentFiles(); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handleServices(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Folder string `json:"folder"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	names, err := console.ScanServices(body.Folder)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"services": names})
}

func (s *Server) handleAgent(w http.ResponseWriter, r *http.Request) {
	file, ok := s.lookupAgent(w, r.PathValue("id"))
	if !ok {
		return
	}
	writeJSON(w, http.StatusOK, agentJSON(file))
}

func (s *Server) handleSaveAgent(w http.ResponseWriter, r *http.Request) {
	var body struct {
		ID          string `json:"id"`
		Name        string `json:"name"`
		Description string `json:"description"`
		Category    string `json:"category"`
		Enabled     bool   `json:"enabled"`
		Tool        string `json:"tool"`
		Model       string `json:"model"`
		Repos       []struct {
			Kind        string `json:"kind"`
			Description string `json:"description"`
		} `json:"repos"`
		WorksWith []struct {
			ID            string `json:"id"`
			Collaboration string `json:"collaboration"`
		} `json:"worksWith"`
		Prompt string `json:"prompt"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	dir, err := s.agentDir()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	model, err := console.ResolveModel(s.modelCatalog(), strings.TrimSpace(body.Tool), strings.TrimSpace(body.Model))
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	currentID := r.PathValue("id")
	with := make([]agentfile.Collaborator, 0, len(body.WorksWith))
	for _, peer := range body.WorksWith {
		with = append(with, agentfile.Collaborator{ID: peer.ID, Collaboration: peer.Collaboration})
	}
	repos := make([]agentfile.Repository, 0, len(body.Repos))
	for _, repo := range body.Repos {
		repos = append(repos, agentfile.Repository{Kind: repo.Kind, Description: repo.Description})
	}
	next := agentfile.File{
		ID:          currentID,
		Name:        body.Name,
		Description: body.Description,
		Category:    body.Category,
		Enabled:     body.Enabled,
		AITool:      body.Tool,
		Model:       model,
		Repos:       repos,
		WorksWith:   with,
		Prompt:      body.Prompt,
	}
	if err := agentfile.Save(dir, currentID, next); err != nil {
		if errors.Is(err, agentfile.ErrNotFound) {
			writeErr(w, http.StatusNotFound, err)
			return
		}
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handlePreview(w http.ResponseWriter, r *http.Request) {
	prep, ok := s.prepare(w, r, true)
	if !ok {
		return
	}
	project, err := orca.ProjectName(prep.snap.WorkspaceID, prep.kind)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"agentId":  prep.agent.ID,
		"runtime":  prep.tool,
		"model":    prep.model,
		"command":  displayCommand(prep.tool, prep.worktree, prep.prompt, project, prep.agent.ID, prep.model),
		"prompt":   prep.prompt,
		"note":     prep.note,
		"worktree": prep.worktree,
		"project":  project,
		"tab":      prep.agent.ID,
	})
}

func (s *Server) handleLaunch(w http.ResponseWriter, r *http.Request) {
	prep, ok := s.prepare(w, r, false)
	if !ok {
		return
	}
	project, err := orca.ProjectName(prep.snap.WorkspaceID, prep.kind)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 2*time.Minute)
	defer cancel()
	client := s.Orca
	if client == nil {
		client = &orca.Client{Dir: prep.worktree}
	}
	var raw string
	if prep.stamp != "" {
		project, err = orca.FeatureProjectName(prep.snap.WorkspaceID, prep.kind, prep.stamp)
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		branch, branchErr := orca.FeatureBranch(prep.stamp, prep.slug, prep.kind)
		if branchErr != nil {
			writeErr(w, http.StatusBadRequest, branchErr)
			return
		}
		raw, err = client.LaunchFeature(ctx, prep.tool, prep.worktree, prep.prompt, project, prep.agent.ID, prep.model, prep.slug, branch)
	} else {
		raw, err = client.Launch(ctx, prep.tool, prep.worktree, prep.prompt, project, prep.agent.ID, prep.model)
	}
	status := "ok"
	detail := raw
	if err != nil {
		status = "error"
		detail = err.Error()
	}
	launch, storeErr := s.Store.AddLaunch(prep.agent.ID, prep.tool, prep.note, status, store.DetailLimit(detail))
	if storeErr != nil {
		writeErr(w, http.StatusInternalServerError, storeErr)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"launch": launch})
}

func (s *Server) handleDeleteLaunch(w http.ResponseWriter, r *http.Request) {
	id, err := strconv.ParseInt(r.PathValue("id"), 10, 64)
	if err != nil || id <= 0 {
		writeErr(w, http.StatusNotFound, store.ErrNotFound)
		return
	}
	if err := s.Store.DeleteLaunch(id); err != nil {
		if errors.Is(err, store.ErrNotFound) {
			writeErr(w, http.StatusNotFound, err)
			return
		}
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true})
}

func (s *Server) handleClearLaunches(w http.ResponseWriter, r *http.Request) {
	if err := s.Store.ClearLaunches(); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true})
}

type launchPrep struct {
	agent    agentdef.Agent
	prompt   string
	snap     console.Snapshot
	note     string
	kind     string
	worktree string
	tool     string
	model    string
	slug     string
	stamp    string
}

func (s *Server) prepare(w http.ResponseWriter, r *http.Request, allowOff bool) (launchPrep, bool) {
	file, ok := s.lookupAgent(w, r.PathValue("id"))
	if !ok {
		return launchPrep{}, false
	}
	if !file.Enabled && !allowOff {
		writeErr(w, http.StatusBadRequest, errors.New("that agent is turned off"))
		return launchPrep{}, false
	}
	agent := agentdef.Agent{
		ID:      file.ID,
		Title:   file.Name,
		Group:   file.Category,
		File:    filepath.ToSlash(filepath.Join(file.Category, file.ID+".md")),
		Repos:   catalogRepos(file.Repos),
		Peers:   catalogPeers(file.WorksWith),
		Summary: file.Description,
	}
	note, repo, tool, model, slug, stamp, err := readLaunch(r)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return launchPrep{}, false
	}
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return launchPrep{}, false
	}
	if tool == "" {
		tool = snap.OrcaAgent
	}
	model, err = console.ResolveModel(s.modelCatalog(), tool, model)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return launchPrep{}, false
	}
	kind, worktree, err := worktreeOf(agent, snap, repo, allowOff)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return launchPrep{}, false
	}
	snap.Worktree = worktree
	return launchPrep{
		agent:    agent,
		prompt:   console.Prompt(agent, file.Prompt, snap, note, launchPeers(file.WorksWith)),
		snap:     snap,
		note:     note,
		kind:     kind,
		worktree: worktree,
		tool:     tool,
		model:    model,
		slug:     slug,
		stamp:    stamp,
	}, true
}

func (s *Server) handleCheck(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), 45*time.Second)
	defer cancel()
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	snap, err = s.rememberPresentIndexes(snap)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if _, err := s.Jobs.Check(ctx, s.Root, targetsOf(snap)); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handleOpen(w http.ResponseWriter, r *http.Request) {
	var body struct {
		URL string `json:"url"`
	}
	if r.Body == nil {
		writeErr(w, http.StatusBadRequest, errors.New("invalid data"))
		return
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, errors.New("invalid data"))
		return
	}
	if err := openIndexPage(body.URL); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}

func (s *Server) handleRefreshIndex(w http.ResponseWriter, r *http.Request) {
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if _, err := s.Jobs.RefreshFolders(targetsOf(snap), strings.TrimSpace(r.PathValue("index"))); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handleCheckOne(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), 45*time.Second)
	defer cancel()
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if _, err := s.Jobs.CheckOne(ctx, s.Root, targetsOf(snap), r.PathValue("kind")); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handleClearIndex(w http.ResponseWriter, r *http.Request) {
	kind := strings.TrimSpace(r.PathValue("kind"))
	if !toolchain.RepoID(kind) {
		writeErr(w, http.StatusBadRequest, errors.New("unknown repository"))
		return
	}
	var body struct {
		DeleteFolder bool `json:"deleteFolder"`
	}
	if r.Body != nil {
		err := json.NewDecoder(r.Body).Decode(&body)
		if err != nil && !errors.Is(err, io.EOF) {
			writeErr(w, http.StatusBadRequest, errors.New("invalid data"))
			return
		}
	}
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	repo := snap.Repo(kind)
	if body.DeleteFolder {
		if err := store.RemoveIndexFolder(repo.Folder, repo.Index); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
	}
	if err := s.Store.SaveRepoIndex(snap, kind, ""); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handleObsidian(w http.ResponseWriter, r *http.Request) {
	s.beginIndex(w, r, "obsidian")
}

func (s *Server) handleCodegraph(w http.ResponseWriter, r *http.Request) {
	s.beginIndex(w, r, "codegraph")
}

func (s *Server) beginIndex(w http.ResponseWriter, r *http.Request, index string) {
	kind, err := indexRepo(r)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if err := s.Store.SaveRepoIndex(snap, kind, index); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	folder := snap.Repo(kind).Folder
	var job jobs.Job
	if index == "obsidian" {
		job, err = s.Jobs.StartObsidian(kind, folder)
	} else {
		job, err = s.Jobs.StartCodegraph(s.Root, kind, folder)
	}
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusAccepted, job)
}

func indexRepo(r *http.Request) (string, error) {
	kind := strings.TrimSpace(r.PathValue("kind"))
	if kind == "" && r.Body != nil {
		var body struct {
			Repo string `json:"repo"`
		}
		err := json.NewDecoder(r.Body).Decode(&body)
		if err != nil && !errors.Is(err, io.EOF) {
			return "", errors.New("invalid data")
		}
		kind = strings.TrimSpace(body.Repo)
	}
	if !toolchain.RepoID(kind) {
		return "", errors.New("choose a repository")
	}
	return kind, nil
}

func (s *Server) handleJobs(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{"jobs": s.Jobs.List()})
}

func (s *Server) handleJob(w http.ResponseWriter, r *http.Request) {
	job, ok := s.Jobs.Get(r.PathValue("id"))
	if !ok {
		writeErr(w, http.StatusNotFound, errors.New("unknown command"))
		return
	}
	writeJSON(w, http.StatusOK, job)
}

func (s *Server) handleCancel(w http.ResponseWriter, r *http.Request) {
	if !s.Jobs.Cancel(r.PathValue("id")) {
		writeErr(w, http.StatusNotFound, errors.New("command is no longer running"))
		return
	}
	s.handleJob(w, r)
}

func (s *Server) handlePage(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path != "/" {
		http.NotFound(w, r)
		return
	}
	serveAsset(w, "web/index.html", "text/html; charset=utf-8")
}

func (s *Server) handleAsset(w http.ResponseWriter, r *http.Request) {
	switch r.URL.Path {
	case "/app.css":
		serveAsset(w, "web/app.css", "text/css; charset=utf-8")
	case "/app.js":
		serveAsset(w, "web/app.js", "text/javascript; charset=utf-8")
	default:
		http.NotFound(w, r)
	}
}

func extraRepoKinds(byKind map[string]console.Repo) []string {
	extras := make([]string, 0)
	for kind := range byKind {
		if toolchain.Known(kind) || !toolchain.RepoID(kind) {
			continue
		}
		extras = append(extras, kind)
	}
	sort.Strings(extras)
	return extras
}

func (s *Server) load() (console.Snapshot, error) {
	snap, err := s.Store.Load()
	if err != nil {
		return snap, err
	}
	snap, err = console.ApplyRoot(s.Root, snap)
	if err != nil {
		return snap, err
	}
	snap.SetupFile = s.Store.File()
	return snap, nil
}

// EnsureAgentFiles creates the agent folder and any missing agent markdown files.
func EnsureAgentFiles(root string, db *store.Store) (string, error) {
	snap, err := db.Load()
	if err != nil {
		return "", err
	}
	dir, err := agentfile.Resolve(root, "")
	if err != nil {
		return "", err
	}
	if err := agentfile.Ensure(dir, snap.OrcaAgent); err != nil {
		return "", err
	}
	return dir, nil
}

func (s *Server) syncAgentFiles() error {
	_, err := EnsureAgentFiles(s.Root, s.Store)
	return err
}

func (s *Server) agentDir() (string, error) {
	return agentfile.Resolve(s.Root, "")
}

func (s *Server) lookupAgent(w http.ResponseWriter, id string) (agentfile.File, bool) {
	dir, err := s.agentDir()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return agentfile.File{}, false
	}
	file, found, err := agentfile.Find(dir, id)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return agentfile.File{}, false
	}
	if !found {
		writeErr(w, http.StatusNotFound, agentfile.ErrNotFound)
		return agentfile.File{}, false
	}
	return file, true
}

func agentJSON(file agentfile.File) map[string]any {
	repos := make([]map[string]string, 0, len(file.Repos))
	for _, repo := range file.Repos {
		repos = append(repos, map[string]string{
			"kind":        repo.Kind,
			"description": repo.Description,
		})
	}
	works := make([]map[string]string, 0, len(file.WorksWith))
	for _, peer := range file.WorksWith {
		works = append(works, map[string]string{
			"id":            peer.ID,
			"collaboration": peer.Collaboration,
		})
	}
	return map[string]any{
		"id":          file.ID,
		"name":        file.Name,
		"description": file.Description,
		"category":    file.Category,
		"enabled":     file.Enabled,
		"tool":        file.AITool,
		"model":       file.Model,
		"repos":       repos,
		"worksWith":   works,
		"prompt":      file.Prompt,
	}
}

func repoKinds(list []agentfile.Repository) []string {
	kinds := make([]string, 0, len(list))
	for _, repo := range list {
		if repo.Kind != "" {
			kinds = append(kinds, repo.Kind)
		}
	}
	return kinds
}

func catalogRepos(list []agentfile.Repository) []agentdef.Repo {
	repos := make([]agentdef.Repo, 0, len(list))
	for _, repo := range list {
		repos = append(repos, agentdef.Repo{Kind: repo.Kind, Description: repo.Description})
	}
	return repos
}

func peerIDs(list []agentfile.Collaborator) []string {
	ids := make([]string, 0, len(list))
	for _, peer := range list {
		if peer.ID != "" {
			ids = append(ids, peer.ID)
		}
	}
	return ids
}

func catalogPeers(list []agentfile.Collaborator) []agentdef.Peer {
	peers := make([]agentdef.Peer, 0, len(list))
	for _, peer := range list {
		peers = append(peers, agentdef.Peer{ID: peer.ID, Collaboration: peer.Collaboration})
	}
	return peers
}

func launchPeers(list []agentfile.Collaborator) []console.Peer {
	peers := make([]console.Peer, 0, len(list))
	for _, peer := range list {
		peers = append(peers, console.Peer{ID: peer.ID, Note: peer.Collaboration})
	}
	return peers
}

func (s *Server) snapshot() (map[string]any, error) {
	snap, err := s.load()
	if err != nil {
		return nil, err
	}
	launches, err := s.Store.Launches(50)
	if err != nil {
		return nil, err
	}
	repos := make([]map[string]any, 0, len(snap.Repos))
	for _, repo := range snap.Repos {
		services := repo.Services
		if services == nil {
			services = []string{}
		}
		available, err := console.ScanServices(repo.Folder)
		if err != nil {
			available = []string{}
		}
		exists := repo.Folder != "" && isDir(repo.Folder)
		empty := false
		if repo.Folder != "" {
			var err error
			empty, err = folderEmpty(repo.Folder)
			if err != nil {
				empty = false
			}
		}
		repos = append(repos, map[string]any{
			"kind":      repo.Kind,
			"folder":    repo.Folder,
			"services":  services,
			"available": available,
			"exists":    exists,
			"empty":     empty,
			"index":     repo.Index,
			"sparse":    repo.Sparse,
		})
	}
	agentPath, err := agentfile.Resolve(s.Root, "")
	if err != nil {
		return nil, err
	}
	files, err := agentfile.List(agentPath)
	if err != nil {
		return nil, err
	}
	teamDir, teamReady, err := s.userAgentsDir()
	if err != nil {
		return nil, err
	}
	agents := make([]map[string]any, 0, len(files))
	for _, file := range files {
		kinds := repoKinds(file.Repos)
		drift := false
		if teamReady {
			same, err := agentFilesMatch(agentPath, file.Path, teamDir)
			if err != nil {
				return nil, err
			}
			drift = !same
		}
		agents = append(agents, map[string]any{
			"id":        file.ID,
			"title":     file.Name,
			"group":     file.Category,
			"category":  file.Category,
			"repos":     kinds,
			"worksWith": peerIDs(file.WorksWith),
			"summary":   file.Description,
			"enabled":   file.Enabled,
			"tool":      file.AITool,
			"model":     file.Model,
			"teamDrift": drift,
		})
	}
	team, err := s.teamStatus()
	if err != nil {
		return nil, err
	}
	return map[string]any{
		"root":            s.Root,
		"setupFile":       s.Store.File(),
		"agentPath":       agentPath,
		"workspaceId":     snap.WorkspaceID,
		"orcaAgent":       snap.OrcaAgent,
		"generateTool":    snap.GenerateTool,
		"generateModel":   snap.GenerateModel,
		"orcaAgents":      console.OrcaAgents,
		"installedAgents": console.InstalledAgents(),
		"models":          modelJSON(s.modelCatalog()),
		"repos":           repos,
		"agents":          agents,
		"indexes":         s.indexes(snap),
		"jobs":            s.Jobs.List(),
		"launches":        launches,
		"teamFolder":      team,
	}, nil
}

// rememberPresentIndexes saves an index when the repository folder already
// contains .obsidian or .codegraph and the app has no index for that repository.
// A repository that already has an index is left as it is. Both folders means
// the choice stays unset.
func (s *Server) rememberPresentIndexes(snap console.Snapshot) (console.Snapshot, error) {
	kind := ""
	index := ""
	for i, repo := range snap.Repos {
		if strings.TrimSpace(repo.Index) != "" {
			continue
		}
		found, ok := presentIndex(repo.Folder)
		if !ok {
			continue
		}
		snap.Repos[i].Index = found
		kind = repo.Kind
		index = found
	}
	if kind == "" {
		return snap, nil
	}
	if err := s.Store.SaveRepoIndex(snap, kind, index); err != nil {
		return snap, err
	}
	return s.load()
}

func presentIndex(folder string) (string, bool) {
	folder = strings.TrimSpace(folder)
	if folder == "" || !isDir(folder) {
		return "", false
	}
	obsidian := isDir(filepath.Join(folder, ".obsidian"))
	codegraph := isDir(filepath.Join(folder, ".codegraph"))
	switch {
	case obsidian && !codegraph:
		return "obsidian", true
	case codegraph && !obsidian:
		return "codegraph", true
	default:
		return "", false
	}
}

func targetsOf(snap console.Snapshot) []jobs.Target {
	targets := make([]jobs.Target, 0, len(snap.Repos))
	for _, repo := range snap.Repos {
		targets = append(targets, jobs.Target{Kind: repo.Kind, Folder: repo.Folder, Index: repo.Index})
	}
	return targets
}

func (s *Server) indexes(snap console.Snapshot) []map[string]any {
	report := s.Jobs.Last()
	out := make([]map[string]any, 0, len(snap.Repos))
	for _, repo := range snap.Repos {
		check := report.Repos[repo.Kind]
		folder := strings.TrimSpace(repo.Folder)
		command := ""
		dirExists := false
		switch repo.Index {
		case "obsidian":
			command = "obsidian vault info=path"
			if folder != "" {
				command = jobs.FormatArgs(toolchain.ObsidianStatus(folder))
				dirExists = isDir(filepath.Join(folder, ".obsidian"))
			}
		case "codegraph":
			command = "codegraph init <folder>"
			if folder != "" {
				command = jobs.FormatArgs(toolchain.CodegraphInit(folder))
				dirExists = isDir(filepath.Join(folder, ".codegraph"))
			}
		}
		item := map[string]any{
			"kind":      repo.Kind,
			"index":     repo.Index,
			"dirExists": dirExists,
			"ready":     false,
			"checked":   check.CheckedAt != "",
			"summary":   check.Summary,
			"command":   command,
			"folder":    folder,
		}
		if check.CheckedAt != "" && repo.Index != "" {
			item["ready"] = check.Ready && dirExists
		}
		out = append(out, item)
	}
	return out
}

func displayCommand(runtime, root, prompt, project, tab, model string) string {
	args := orca.LaunchArgs(runtime, root, prompt, model)
	for i, arg := range args {
		if arg == "--prompt" && i+1 < len(args) {
			args[i+1] = "<prompt>"
		}
	}
	return "project " + project + "\ntab " + tab + "\norca " + jobs.FormatArgs(args)
}

func readLaunch(r *http.Request) (string, string, string, string, string, string, error) {
	var body struct {
		Note  string `json:"note"`
		Repo  string `json:"repo"`
		Agent string `json:"agent"`
		Model string `json:"model"`
		Slug  string `json:"slug"`
		Stamp string `json:"stamp"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		if errors.Is(err, io.EOF) {
			return "", "", "", "", "", "", nil
		}
		return "", "", "", "", "", "", errors.New("invalid data")
	}
	if len(body.Note) > 4000 {
		return "", "", "", "", "", "", errors.New("message is longer than 4000 characters")
	}
	slug := strings.TrimSpace(body.Slug)
	stamp := strings.TrimSpace(body.Stamp)
	if (slug == "") != (stamp == "") {
		return "", "", "", "", "", "", errors.New("feature name is not valid")
	}
	return body.Note, strings.TrimSpace(body.Repo), strings.TrimSpace(body.Agent), strings.TrimSpace(body.Model), slug, stamp, nil
}

func (s *Server) generateChoice() (string, string, error) {
	snap, err := s.load()
	if err != nil {
		return "", "", err
	}
	tool := snap.GenerateTool
	if tool == "" {
		tool = snap.OrcaAgent
	}
	model, err := console.ResolveModel(s.modelCatalog(), tool, snap.GenerateModel)
	if err != nil {
		model = strings.TrimSpace(snap.GenerateModel)
	}
	return tool, model, nil
}

func (s *Server) handleGeneratePreview(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Kind    string `json:"kind"`
		Subject string `json:"subject"`
		Text    string `json:"text"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	tool, model, err := s.generateChoice()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	command, err := console.PreviewCommand(tool, model, strings.TrimSpace(body.Kind), body.Subject, body.Text)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"command": command, "tool": tool, "model": model})
}

func (s *Server) handleGenerate(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Kind    string `json:"kind"`
		Subject string `json:"subject"`
		Text    string `json:"text"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	tool, model, err := s.generateChoice()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if _, err := console.RewritePrompt(strings.TrimSpace(body.Kind), body.Subject, body.Text); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	flusher, ok := w.(http.Flusher)
	if !ok {
		writeErr(w, http.StatusInternalServerError, errors.New("streaming is not available"))
		return
	}
	w.Header().Set("Content-Type", "application/x-ndjson")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(http.StatusOK)
	ctx, cancel := context.WithTimeout(r.Context(), 2*time.Minute)
	defer cancel()
	enc := json.NewEncoder(w)
	writeEvent := func(ev console.GenerateEvent) error {
		if err := enc.Encode(ev); err != nil {
			return err
		}
		flusher.Flush()
		return nil
	}
	if err := writeEvent(console.GenerateEvent{Type: "meta", Tool: tool, Model: model}); err != nil {
		return
	}
	err = console.GenerateStream(ctx, tool, model, strings.TrimSpace(body.Kind), body.Subject, body.Text, writeEvent)
	if err != nil {
		_ = writeEvent(console.GenerateEvent{Type: "error", Error: err.Error()})
	}
}

func (s *Server) handleToolModels(w http.ResponseWriter, r *http.Request) {
	tool := strings.TrimSpace(r.PathValue("tool"))
	known := false
	for _, name := range console.OrcaAgents {
		if name == tool {
			known = true
			break
		}
	}
	if !known {
		writeErr(w, http.StatusBadRequest, errors.New("AI Tool is not supported"))
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 20*time.Second)
	defer cancel()
	models := console.LiveModels(ctx, tool)
	if len(models) == 0 {
		models = console.ModelCatalog()[tool]
	}
	s.storeToolModels(tool, models)
	writeJSON(w, http.StatusOK, map[string]any{"models": modelRows(models)})
}

func (s *Server) storeToolModels(tool string, models []console.Model) {
	s.modelsMu.Lock()
	defer s.modelsMu.Unlock()
	next := map[string][]console.Model{}
	if s.models != nil {
		for name, list := range s.models {
			next[name] = list
		}
	} else {
		next = console.ModelCatalog()
	}
	next[tool] = models
	s.models = next
	s.modelsAt = time.Now()
	if tool == "cursor" {
		s.cursorReady = true
	}
	if tool == "claude" {
		s.claudeReady = true
	}
}

func (s *Server) modelCatalog() map[string][]console.Model {
	s.modelsMu.Lock()
	if s.cursorReady && s.models != nil {
		catalog := s.models
		s.modelsMu.Unlock()
		return catalog
	}
	if s.models != nil && time.Since(s.modelsAt) < 15*time.Second {
		catalog := s.models
		s.modelsMu.Unlock()
		return catalog
	}
	claudeReady := s.claudeReady
	previousClaude := append([]console.Model(nil), s.models["claude"]...)
	s.modelsMu.Unlock()

	catalog := console.ModelCatalog()
	if claudeReady && len(previousClaude) > 0 {
		catalog["claude"] = previousClaude
	} else if live := console.ClaudeModels(context.Background()); len(live) > 0 {
		catalog["claude"] = live
		claudeReady = true
	}
	cursorReady := false
	if live := console.CursorModels(context.Background()); len(live) > 0 {
		catalog["cursor"] = live
		cursorReady = true
	}

	s.modelsMu.Lock()
	defer s.modelsMu.Unlock()
	if s.cursorReady && s.models != nil {
		return s.models
	}
	s.models = catalog
	s.modelsAt = time.Now()
	s.cursorReady = cursorReady
	if claudeReady {
		s.claudeReady = true
	}
	return s.models
}

func modelRows(models []console.Model) []map[string]any {
	rows := make([]map[string]any, 0, len(models))
	for _, model := range models {
		row := map[string]any{
			"id":      model.ID,
			"label":   model.Label,
			"default": model.Default,
		}
		if model.Group != "" {
			row["group"] = model.Group
		}
		rows = append(rows, row)
	}
	return rows
}

func modelJSON(catalog map[string][]console.Model) map[string][]map[string]any {
	out := map[string][]map[string]any{}
	for _, tool := range console.OrcaAgents {
		out[tool] = modelRows(catalog[tool])
	}
	return out
}

func worktreeOf(agent agentdef.Agent, snap console.Snapshot, kind string, allowUnset bool) (string, string, error) {
	if kind == "" {
		if len(agent.Repos) == 1 {
			kind = agent.Repos[0].Kind
		} else {
			return "", "", errors.New("choose the repo folder for this agent")
		}
	}
	allowed := false
	for _, item := range agent.Repos {
		if item.Kind == kind {
			allowed = true
		}
	}
	if !allowed {
		return "", "", errors.New("that repo is not used by this agent")
	}
	folder := strings.TrimSpace(snap.Repo(kind).Folder)
	if folder == "" {
		if allowUnset {
			return kind, "", nil
		}
		return "", "", fmt.Errorf("%s folder is not set", kind)
	}
	return kind, folder, nil
}

func readJSON(w http.ResponseWriter, r *http.Request, dst any) bool {
	r.Body = http.MaxBytesReader(w, r.Body, 1<<20)
	if err := json.NewDecoder(r.Body).Decode(dst); err != nil {
		writeErr(w, http.StatusBadRequest, errors.New("invalid data"))
		return false
	}
	return true
}

func writeJSON(w http.ResponseWriter, code int, value any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(value)
}

func writeErr(w http.ResponseWriter, code int, err error) {
	writeJSON(w, code, map[string]string{"error": err.Error()})
}

func serveAsset(w http.ResponseWriter, name, contentType string) {
	raw, err := fs.ReadFile(webFS, name)
	if err != nil {
		http.Error(w, "missing asset", http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", contentType)
	w.Header().Set("Cache-Control", "no-store")
	_, _ = w.Write(raw)
}

func isDir(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.IsDir()
}
