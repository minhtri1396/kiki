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
	"kiki/internal/guard"
	"kiki/internal/jobs"
	"kiki/internal/mcp"
	"kiki/internal/orca"
	"kiki/internal/store"
	"kiki/internal/toolchain"
	"kiki/internal/tools"
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

	// MCPHome overrides the home directory used when installing Atlassian MCP and the RTK hook.
	// Empty uses the current user.
	MCPHome string

	// RTKHooks installs or removes the RTK command hook. Nil uses tools.SyncHooks.
	RTKHooks func(home string, on bool) error

	// GuardHooks installs or removes guardrail hooks. Nil uses the guard package.
	GuardHooks func(home string, on bool) error

	guardMu       sync.Mutex
	guardHome     string
	guardURL      string
	guardBindings []guard.Binding
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
	mux.HandleFunc("POST /api/repos/browse", s.handleRepoBrowse)
	mux.HandleFunc("PUT /api/repos/{kind}/folder", s.handleRepoFolder)
	mux.HandleFunc("POST /api/repos/{kind}/clone", s.handleClone)
	mux.HandleFunc("GET /api/agents/{id}", s.handleAgent)
	mux.HandleFunc("PUT /api/agents/{id}", s.handleSaveAgent)
	mux.HandleFunc("POST /api/agents/{id}/preview", s.handlePreview)
	mux.HandleFunc("POST /api/agents/{id}/launch", s.handleLaunch)
	mux.HandleFunc("POST /api/guardrail", s.handleGuardrail)
	mux.HandleFunc("DELETE /api/launches/{id}", s.handleDeleteLaunch)
	mux.HandleFunc("DELETE /api/launches", s.handleClearLaunches)
	mux.HandleFunc("POST /api/indexes/check", s.handleCheck)
	mux.HandleFunc("POST /api/indexes/check/{kind}", s.handleCheckOne)
	mux.HandleFunc("POST /api/indexes/refresh/{index}", s.handleRefreshIndex)
	mux.HandleFunc("POST /api/open", s.handleOpen)
	mux.HandleFunc("DELETE /api/indexes/{kind}", s.handleClearIndex)
	mux.HandleFunc("PUT /api/indexes/{kind}", s.handleUseIndex)
	mux.HandleFunc("POST /api/indexes/obsidian", s.handleObsidian)
	mux.HandleFunc("POST /api/indexes/codegraph", s.handleCodegraph)
	mux.HandleFunc("POST /api/indexes/codegraph/{kind}", s.handleCodegraph)
	mux.HandleFunc("GET /api/jobs", s.handleJobs)
	mux.HandleFunc("GET /api/jobs/{id}", s.handleJob)
	mux.HandleFunc("POST /api/jobs/{id}/cancel", s.handleCancel)
	mux.HandleFunc("POST /api/team/browse", s.handleTeamBrowse)
	mux.HandleFunc("PUT /api/team", s.handleTeamSave)
	mux.HandleFunc("GET /api/team/members", s.handleTeamMembers)
	mux.HandleFunc("GET /api/team/compare", s.handleTeamCompare)
	mux.HandleFunc("GET /api/team/diff", s.handleTeamDiff)
	mux.HandleFunc("GET /api/atlassian/jira/boards", s.handleJiraBoards)
	mux.HandleFunc("GET /api/atlassian/confluence/spaces", s.handleConfluenceSpaces)
	mux.HandleFunc("GET /api/atlassian/confluence/pages", s.handleConfluencePages)
	mux.HandleFunc("POST /api/team/member", s.handleTeamMember)
	mux.HandleFunc("POST /api/team/agents", s.handleSaveUserAgents)
	mux.HandleFunc("POST /api/team/repos", s.handleSaveUserRepos)
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
		Orchestrator  string `json:"orchestrator"`
		GenerateTool  string `json:"generateTool"`
		GenerateModel string `json:"generateModel"`
		Repos         []struct {
			Kind     string   `json:"kind"`
			Folder   string   `json:"folder"`
			Services []string `json:"services"`
		} `json:"repos"`
		Atlassian *atlassianBody  `json:"atlassian"`
		Tools     *tools.Settings `json:"tools"`
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
		Orchestrator:  strings.TrimSpace(body.Orchestrator),
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
	if snap.Orchestrator == "" {
		snap.Orchestrator = current.Orchestrator
	}
	snap.Orchestrator = console.NormalizeOrchestrator(snap.Orchestrator)
	snap, err = console.ApplyRoot(s.Root, snap)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if err := console.Validate(snap); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	var atlassian *mcp.Atlassian
	if body.Atlassian != nil {
		saved, err := s.Store.Atlassian()
		if err != nil {
			writeErr(w, http.StatusInternalServerError, err)
			return
		}
		next := mcp.Apply(saved, body.Atlassian.form())
		if err := next.Validate(); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		atlassian = &next
	}
	if err := s.Store.Save(snap); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if atlassian != nil {
		if err := s.Store.SaveAtlassian(*atlassian); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
	}
	if body.Tools != nil {
		next := *body.Tools
		if next.Guardrails == nil {
			currentTools, err := s.loadTools()
			if err != nil {
				writeErr(w, http.StatusInternalServerError, err)
				return
			}
			next.Guardrails = currentTools.Guardrails
		}
		if err := s.Store.SaveTools(next); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		if err := s.syncRTK(next.RTK); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		if err := s.syncGuardrails(next.GuardrailsOn()); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
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

func (s *Server) handleRepoBrowse(w http.ResponseWriter, r *http.Request) {
	picked, err := chooseFolder()
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	dir := filepath.Clean(strings.TrimSpace(picked))
	info, err := os.Stat(dir)
	if err != nil || !info.IsDir() {
		writeErr(w, http.StatusBadRequest, fmt.Errorf("folder %s does not exist", dir))
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"path": dir})
}

func (s *Server) handleRepoFolder(w http.ResponseWriter, r *http.Request) {
	kind := strings.TrimSpace(r.PathValue("kind"))
	if !toolchain.RepoID(kind) {
		writeErr(w, http.StatusBadRequest, errors.New("unknown repository"))
		return
	}
	var body struct {
		Folder string `json:"folder"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if err := s.Store.SaveRepoFolder(snap, kind, body.Folder); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	s.handleState(w, r)
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
			Readonly    bool   `json:"readonly"`
		} `json:"repos"`
		WorksWith []struct {
			ID            string `json:"id"`
			Collaboration string `json:"collaboration"`
		} `json:"worksWith"`
		MCPs []struct {
			ID          string `json:"id"`
			Description string `json:"description"`
			BoardID     string `json:"boardId"`
			BoardName   string `json:"boardName"`
			SpaceID     string `json:"spaceId"`
			SpaceKey    string `json:"spaceKey"`
			SpaceName   string `json:"spaceName"`
			ParentID    string `json:"parentId"`
			ParentTitle string `json:"parentTitle"`
		} `json:"mcps"`
		Tools []struct {
			ID          string `json:"id"`
			Description string `json:"description"`
		} `json:"tools"`
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
		repos = append(repos, agentfile.Repository{Kind: repo.Kind, Description: repo.Description, Readonly: repo.Readonly})
	}
	mcps := make([]agentfile.Use, 0, len(body.MCPs))
	for _, item := range body.MCPs {
		mcps = append(mcps, agentfile.Use{
			ID:          item.ID,
			Description: item.Description,
			BoardID:     item.BoardID,
			BoardName:   item.BoardName,
			SpaceID:     item.SpaceID,
			SpaceKey:    item.SpaceKey,
			SpaceName:   item.SpaceName,
			ParentID:    item.ParentID,
			ParentTitle: item.ParentTitle,
		})
	}
	selectedTools := make([]agentfile.Use, 0, len(body.Tools))
	for _, item := range body.Tools {
		id := strings.TrimSpace(item.ID)
		if id == "" || tools.Global(id) {
			continue
		}
		selectedTools = append(selectedTools, agentfile.Use{ID: id})
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
		MCPs:        mcps,
		Tools:       selectedTools,
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
	command, project, err := s.previewOrchestrator(prep)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"agentId":  prep.agent.ID,
		"runtime":  prep.tool,
		"model":    prep.model,
		"command":  command,
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
	ctx, cancel := context.WithTimeout(r.Context(), 2*time.Minute)
	defer cancel()
	raw, err := s.launchOrchestrator(ctx, prep)
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
	cfg, err := s.atlassianSetup()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return launchPrep{}, false
	}
	mcps := mcpsFromAtlassian(cfg)
	enabled, err := s.enabledTools()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return launchPrep{}, false
	}
	agent := agentdef.Agent{
		ID:      file.ID,
		Title:   file.Name,
		Group:   file.Category,
		File:    filepath.ToSlash(filepath.Join(file.Category, file.ID+".md")),
		Repos:   catalogRepos(file.Repos),
		Peers:   catalogPeers(file.WorksWith),
		MCPs:    applyAtlassianPlaces(catalogUses(file.MCPs, mcps), cfg),
		Tools:   toolPromptUses(file.Tools, enabled),
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
		tool = strings.TrimSpace(file.AITool)
	}
	if tool == "" {
		writeErr(w, http.StatusBadRequest, errors.New("choose an AI tool"))
		return launchPrep{}, false
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
	if !allowOff {
		if err := toolsReady(useIDs(file.Tools), enabled); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return launchPrep{}, false
		}
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

func (s *Server) handleUseIndex(w http.ResponseWriter, r *http.Request) {
	kind := strings.TrimSpace(r.PathValue("kind"))
	if !toolchain.RepoID(kind) {
		writeErr(w, http.StatusBadRequest, errors.New("unknown repository"))
		return
	}
	var body struct {
		Index string `json:"index"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	index := strings.TrimSpace(body.Index)
	if index != "obsidian" && index != "codegraph" {
		writeErr(w, http.StatusBadRequest, errors.New("index must be obsidian or codegraph"))
		return
	}
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	folder := strings.TrimSpace(snap.Repo(kind).Folder)
	if !indexToolPresent(folder, index) {
		writeErr(w, http.StatusBadRequest, fmt.Errorf("%s is not in this repository", indexDir(index)))
		return
	}
	if err := s.Store.SaveRepoIndex(snap, kind, index); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	s.handleState(w, r)
}

func presentIndexTools(folder string) []string {
	present := make([]string, 0, 2)
	folder = strings.TrimSpace(folder)
	if folder == "" {
		return present
	}
	if isDir(filepath.Join(folder, ".obsidian")) {
		present = append(present, "obsidian")
	}
	if isDir(filepath.Join(folder, ".codegraph")) {
		present = append(present, "codegraph")
	}
	return present
}

func indexToolPresent(folder, index string) bool {
	folder = strings.TrimSpace(folder)
	return folder != "" && isDir(filepath.Join(folder, indexDir(index)))
}

func indexDir(index string) string {
	if index == "obsidian" {
		return ".obsidian"
	}
	return ".codegraph"
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
	folder := strings.TrimSpace(snap.Repo(kind).Folder)
	if indexToolPresent(folder, index) {
		s.handleState(w, r)
		return
	}
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
// The first time that folder is empty, each new file starts on cursor. After that, each agent keeps its own AI Tool.
func EnsureAgentFiles(root string) (string, error) {
	dir, err := agentfile.Resolve(root, "")
	if err != nil {
		return "", err
	}
	if err := agentfile.Ensure(dir, "cursor"); err != nil {
		return "", err
	}
	return dir, nil
}

func (s *Server) syncAgentFiles() error {
	_, err := EnsureAgentFiles(s.Root)
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
	return map[string]any{
		"id":          file.ID,
		"name":        file.Name,
		"description": file.Description,
		"category":    file.Category,
		"enabled":     file.Enabled,
		"tool":        file.AITool,
		"model":       file.Model,
		"repos":       repositoryJSON(file.Repos),
		"worksWith":   teamworkJSON(file.WorksWith),
		"mcps":        useJSON(file.MCPs),
		"tools":       useJSON(file.Tools),
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
		repos = append(repos, agentdef.Repo{Kind: repo.Kind, Description: repo.Description, Readonly: repo.Readonly})
	}
	return repos
}

func teamworkJSON(list []agentfile.Collaborator) []map[string]string {
	out := make([]map[string]string, 0, len(list))
	for _, peer := range list {
		if peer.ID == "" {
			continue
		}
		out = append(out, map[string]string{
			"id":            peer.ID,
			"collaboration": peer.Collaboration,
		})
	}
	return out
}

func putUse(row map[string]string, key, value string) {
	value = strings.TrimSpace(value)
	if value == "" {
		return
	}
	row[key] = value
}

func useJSON(list []agentfile.Use) []map[string]string {
	out := make([]map[string]string, 0, len(list))
	for _, item := range list {
		if item.ID == "" {
			continue
		}
		row := map[string]string{
			"id":          item.ID,
			"description": item.Description,
		}
		putUse(row, "boardId", item.BoardID)
		putUse(row, "boardName", item.BoardName)
		putUse(row, "spaceId", item.SpaceID)
		putUse(row, "spaceKey", item.SpaceKey)
		putUse(row, "spaceName", item.SpaceName)
		putUse(row, "parentId", item.ParentID)
		putUse(row, "parentTitle", item.ParentTitle)
		out = append(out, row)
	}
	return out
}

func repositoryJSON(list []agentfile.Repository) []map[string]any {
	out := make([]map[string]any, 0, len(list))
	for _, repo := range list {
		if repo.Kind == "" {
			continue
		}
		out = append(out, map[string]any{
			"kind":        repo.Kind,
			"description": repo.Description,
			"readonly":    repo.Readonly,
		})
	}
	return out
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
			"git":       gitOrigin(repo.Folder),
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
	agents := make([]map[string]any, 0, len(files))
	for _, file := range files {
		kinds := repoKinds(file.Repos)
		agents = append(agents, map[string]any{
			"id":             file.ID,
			"title":          file.Name,
			"group":          file.Category,
			"category":       file.Category,
			"repos":          kinds,
			"repositoryUses": repositoryJSON(file.Repos),
			"worksWith":      peerIDs(file.WorksWith),
			"teamwork":       teamworkJSON(file.WorksWith),
			"mcps":           useJSON(file.MCPs),
			"tools":          useJSON(file.Tools),
			"summary":        file.Description,
			"enabled":        file.Enabled,
			"tool":           file.AITool,
			"model":          file.Model,
		})
	}
	team, err := s.teamStatus()
	if err != nil {
		return nil, err
	}
	atlassian, err := s.Store.Atlassian()
	if err != nil {
		return nil, err
	}
	savedTools, err := s.loadTools()
	if err != nil {
		return nil, err
	}
	return map[string]any{
		"root":            s.Root,
		"setupFile":       s.Store.File(),
		"agentPath":       agentPath,
		"workspaceId":     snap.WorkspaceID,
		"orchestrator":    snap.Orchestrator,
		"orchestrators":   console.Orchestrators,
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
		"atlassian":       atlassianState(atlassian),
		"mcps":            namedJSON(mcpsFromAtlassian(atlassian)),
		"tools":           namedJSON(toolsFrom(savedTools)),
		"toolChoices":     toolChoiceJSON(savedTools),
		"guardrails":      savedTools.GuardrailsOn(),
	}, nil
}

// namedID is one MCP server or tool the edit-agent dialog can offer.
type namedID struct {
	ID   string
	Name string
}

// atlassianSetup is the Atlassian section saved in Settings.
func (s *Server) atlassianSetup() (mcp.Atlassian, error) {
	if s == nil || s.Store == nil {
		return mcp.Atlassian{}, nil
	}
	return s.Store.Atlassian()
}

// applyAtlassianPlaces fills a Jira board or Confluence space from Settings
// when this agent has not chosen its own.
func applyAtlassianPlaces(uses []agentdef.Use, cfg mcp.Atlassian) []agentdef.Use {
	cfg = cfg.Normalized()
	for i, item := range uses {
		switch strings.TrimSpace(item.ID) {
		case "atlassian_jira":
			if strings.TrimSpace(item.BoardID) == "" && strings.TrimSpace(item.BoardName) == "" {
				uses[i].BoardID = cfg.Jira.BoardID
				uses[i].BoardName = cfg.Jira.BoardName
			}
		case "atlassian_confluence":
			if strings.TrimSpace(item.SpaceID) == "" && strings.TrimSpace(item.SpaceName) == "" {
				uses[i].SpaceID = cfg.Confluence.SpaceID
				uses[i].SpaceKey = cfg.Confluence.SpaceKey
				uses[i].SpaceName = cfg.Confluence.SpaceName
				uses[i].ParentID = cfg.Confluence.ParentID
				uses[i].ParentTitle = cfg.Confluence.ParentTitle
			}
		}
	}
	return uses
}

func mcpsFromAtlassian(cfg mcp.Atlassian) []namedID {
	cfg = cfg.Normalized()
	if !cfg.Enabled {
		return nil
	}
	var out []namedID
	if cfg.Jira.Enabled {
		out = append(out, namedID{ID: "atlassian_jira", Name: "Atlassian - Jira"})
	}
	if cfg.Confluence.Enabled {
		out = append(out, namedID{ID: "atlassian_confluence", Name: "Atlassian - Confluence"})
	}
	return out
}

// enabledTools is the tools turned on in Settings.
func (s *Server) enabledTools() ([]namedID, error) {
	cfg, err := s.loadTools()
	if err != nil {
		return nil, err
	}
	return toolsFrom(cfg), nil
}

// SyncRTK registers the RTK hook when Settings has RTK on, and removes it when RTK is off.
func (s *Server) SyncRTK() error {
	cfg, err := s.loadTools()
	if err != nil {
		return err
	}
	return s.syncRTK(cfg.RTK)
}

func (s *Server) syncRTK(on bool) error {
	home := strings.TrimSpace(s.MCPHome)
	if home == "" {
		var err error
		home, err = os.UserHomeDir()
		if err != nil {
			return err
		}
	}
	if s.RTKHooks != nil {
		return s.RTKHooks(home, on)
	}
	return tools.SyncHooks(home, on)
}

func (s *Server) loadTools() (tools.Settings, error) {
	if s == nil || s.Store == nil {
		return tools.Settings{}, nil
	}
	return s.Store.Tools()
}

func toolsFrom(cfg tools.Settings) []namedID {
	var out []namedID
	for _, item := range tools.Catalog() {
		if item.Global || !cfg.On(item.ID) {
			continue
		}
		out = append(out, namedID{ID: item.ID, Name: item.Name})
	}
	return out
}

func toolChoiceJSON(cfg tools.Settings) []map[string]any {
	list := tools.Catalog()
	out := make([]map[string]any, 0, len(list))
	for _, item := range list {
		out = append(out, map[string]any{
			"id":        item.ID,
			"name":      item.Name,
			"command":   item.Command,
			"detail":    item.Detail,
			"page":      item.Page,
			"enabled":   cfg.On(item.ID),
			"installed": tools.Installed(item.Command),
			"global":    item.Global,
		})
	}
	return out
}

// toolsReady reports that every tool selected for the agent is turned on in Settings.
func toolsReady(wanted []string, enabled []namedID) error {
	ready := map[string]bool{}
	for _, item := range enabled {
		ready[item.ID] = true
	}
	var missing []string
	for _, id := range wanted {
		id = strings.TrimSpace(id)
		if id == "" || tools.Global(id) || ready[id] {
			continue
		}
		missing = append(missing, id)
	}
	if len(missing) == 0 {
		return nil
	}
	return fmt.Errorf("set up %s before starting this agent", strings.Join(missing, ", "))
}

func namedJSON(list []namedID) []map[string]string {
	out := make([]map[string]string, 0, len(list))
	for _, item := range list {
		id := strings.TrimSpace(item.ID)
		if id == "" {
			continue
		}
		name := strings.TrimSpace(item.Name)
		if name == "" {
			name = id
		}
		out = append(out, map[string]string{"id": id, "name": name})
	}
	return out
}

func catalogUses(list []agentfile.Use, allowed []namedID) []agentdef.Use {
	names := map[string]string{}
	for _, item := range allowed {
		id := strings.TrimSpace(item.ID)
		if id == "" {
			continue
		}
		name := strings.TrimSpace(item.Name)
		if name == "" {
			name = id
		}
		names[id] = name
	}
	list = spreadAtlassian(list, names)
	out := make([]agentdef.Use, 0, len(list))
	seen := map[string]bool{}
	for _, item := range list {
		id := strings.TrimSpace(item.ID)
		name, ok := names[id]
		if id == "" || !ok || seen[id] {
			continue
		}
		seen[id] = true
		out = append(out, agentdef.Use{
			ID:          id,
			Name:        name,
			Description: item.Description,
			BoardID:     item.BoardID,
			BoardName:   item.BoardName,
			SpaceID:     item.SpaceID,
			SpaceKey:    item.SpaceKey,
			SpaceName:   item.SpaceName,
			ParentID:    item.ParentID,
			ParentTitle: item.ParentTitle,
		})
	}
	return out
}

// toolPromptUses keeps the tools this agent is set to use and fills each one
// with the description the app defines for that tool.
func toolPromptUses(list []agentfile.Use, enabled []namedID) []agentdef.Use {
	uses := catalogUses(list, enabled)
	text := map[string]string{}
	for _, spec := range tools.Catalog() {
		text[spec.ID] = spec.Detail
	}
	for i := range uses {
		if detail := strings.TrimSpace(text[uses[i].ID]); detail != "" {
			uses[i].Description = detail
		}
	}
	return uses
}

// spreadAtlassian turns a saved "atlassian" choice into the Jira and Confluence
// tags that Settings currently offers. A tag the agent already set keeps its note.
func spreadAtlassian(list []agentfile.Use, names map[string]string) []agentfile.Use {
	legacy := ""
	found := false
	out := make([]agentfile.Use, 0, len(list)+1)
	present := map[string]bool{}
	for _, item := range list {
		id := strings.TrimSpace(item.ID)
		if id == "atlassian" {
			found = true
			if legacy == "" {
				legacy = item.Description
			}
			continue
		}
		present[id] = true
		out = append(out, item)
	}
	if !found {
		return list
	}
	for _, id := range []string{"atlassian_jira", "atlassian_confluence"} {
		if _, ok := names[id]; !ok || present[id] {
			continue
		}
		out = append(out, agentfile.Use{ID: id, Description: legacy})
	}
	return out
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
			"present":   presentIndexTools(folder),
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

func (s *Server) previewOrchestrator(prep launchPrep) (string, string, error) {
	switch console.NormalizeOrchestrator(prep.snap.Orchestrator) {
	case "orca":
		return s.previewOrca(prep)
	default:
		return "", "", fmt.Errorf("Agent Orchestration tool is not supported")
	}
}

func (s *Server) previewOrca(prep launchPrep) (string, string, error) {
	project, err := orca.ProjectName(prep.snap.WorkspaceID, prep.kind)
	if err != nil {
		return "", "", err
	}
	return orcaCommand(prep.tool, prep.worktree, prep.prompt, project, prep.agent.ID, prep.model), project, nil
}

func (s *Server) launchOrchestrator(ctx context.Context, prep launchPrep) (string, error) {
	s.bindGuard(prep.agent.ID, prep.worktree, repoReadonly(prep.agent.Repos, prep.kind))
	if err := s.ensureAgentHooks(prep.tool); err != nil {
		return "", err
	}
	if err := s.ensureAgentTools(prep); err != nil {
		return "", err
	}
	if err := s.ensureAgentMCPs(prep); err != nil {
		return "", err
	}
	switch console.NormalizeOrchestrator(prep.snap.Orchestrator) {
	case "orca":
		return s.launchOrca(ctx, prep)
	default:
		return "", fmt.Errorf("Agent Orchestration tool is not supported")
	}
}

func (s *Server) launchOrca(ctx context.Context, prep launchPrep) (string, error) {
	project, err := orca.ProjectName(prep.snap.WorkspaceID, prep.kind)
	if err != nil {
		return "", err
	}
	client := s.Orca
	if client == nil {
		client = &orca.Client{Dir: prep.worktree}
	}
	next := *client
	env := append([]string{}, client.Env...)
	next.Env = append(env, "KIKI_AGENT="+prep.agent.ID)
	next.OnWorktree = func(path string) {
		s.bindGuard(prep.agent.ID, path, repoReadonly(prep.agent.Repos, prep.kind))
	}
	client = &next
	if prep.stamp != "" {
		project, err = orca.FeatureProjectName(prep.snap.WorkspaceID, prep.kind, prep.stamp)
		if err != nil {
			return "", err
		}
		branch, branchErr := orca.FeatureBranch(prep.stamp, prep.slug, prep.kind)
		if branchErr != nil {
			return "", branchErr
		}
		return client.LaunchFeature(ctx, prep.tool, prep.worktree, prep.prompt, project, prep.agent.ID, prep.model, prep.slug, branch)
	}
	return client.Launch(ctx, prep.tool, prep.worktree, prep.prompt, project, prep.agent.ID, prep.model)
}

func orcaCommand(runtime, root, prompt, project, tab, model string) string {
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
	tool := strings.TrimSpace(snap.GenerateTool)
	if tool == "" {
		return "", "", console.InputError{Message: "choose an AI Tool for Generate text"}
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
		writeChoiceErr(w, err)
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
	if _, err := console.RewritePrompt(strings.TrimSpace(body.Kind), body.Subject, body.Text); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	tool, model, err := s.generateChoice()
	if err != nil {
		writeChoiceErr(w, err)
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

func repoReadonly(repos []agentdef.Repo, kind string) bool {
	kind = strings.TrimSpace(kind)
	for _, repo := range repos {
		if repo.Kind == kind {
			return repo.Readonly
		}
	}
	return false
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

func writeChoiceErr(w http.ResponseWriter, err error) {
	var input console.InputError
	if errors.As(err, &input) {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeErr(w, http.StatusInternalServerError, err)
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
