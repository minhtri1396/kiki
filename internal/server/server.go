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

	modelsOnce sync.Once
	models     map[string][]console.Model
}

// Handler routes the console.
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/state", s.handleState)
	mux.HandleFunc("PUT /api/setup", s.handleSave)
	mux.HandleFunc("PUT /api/models", s.handleModels)
	mux.HandleFunc("POST /api/repos/services", s.handleServices)
	mux.HandleFunc("POST /api/repos/{kind}/clone", s.handleClone)
	mux.HandleFunc("POST /api/agents/{id}/preview", s.handlePreview)
	mux.HandleFunc("POST /api/agents/{id}/launch", s.handleLaunch)
	mux.HandleFunc("DELETE /api/launches/{id}", s.handleDeleteLaunch)
	mux.HandleFunc("DELETE /api/launches", s.handleClearLaunches)
	mux.HandleFunc("POST /api/indexes/check", s.handleCheck)
	mux.HandleFunc("POST /api/indexes/gitnexus", s.handleGitNexus)
	mux.HandleFunc("POST /api/indexes/codegraph/{kind}", s.handleCodegraph)
	mux.HandleFunc("GET /api/jobs", s.handleJobs)
	mux.HandleFunc("GET /api/jobs/{id}", s.handleJob)
	mux.HandleFunc("POST /api/jobs/{id}/cancel", s.handleCancel)
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
		WorkspaceID string `json:"workspaceId"`
		OrcaAgent   string `json:"orcaAgent"`
		Repos       []struct {
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
		Root:        s.Root,
		WorkspaceID: strings.TrimSpace(body.WorkspaceID),
		OrcaAgent:   strings.TrimSpace(body.OrcaAgent),
	}
	for _, kind := range toolchain.Kinds {
		repo, ok := byKind[kind]
		if !ok {
			repo = console.Repo{Kind: kind}
		}
		snap.Repos = append(snap.Repos, repo)
	}
	snap, err := console.ApplyRoot(s.Root, snap)
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
	s.handleState(w, r)
}

func (s *Server) handleModels(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Agents []store.AgentModel `json:"agents"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	order := map[string]int{}
	for i, agent := range agentdef.Catalog {
		order[agent.ID] = i
	}
	saved := make([]store.AgentModel, 0, len(body.Agents))
	seen := map[string]bool{}
	for _, item := range body.Agents {
		if seen[item.ID] {
			continue
		}
		if _, found := order[item.ID]; !found {
			writeErr(w, http.StatusBadRequest, errors.New("unknown agent"))
			return
		}
		seen[item.ID] = true
		model, err := console.ResolveModel(s.modelCatalog(), item.Tool, item.Model)
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		saved = append(saved, store.AgentModel{ID: item.ID, Tool: strings.TrimSpace(item.Tool), Model: model})
	}
	sort.Slice(saved, func(i, j int) bool { return order[saved[i].ID] < order[saved[j].ID] })
	if err := s.Store.SaveModels(saved); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"modelsFile":  s.Store.ModelsFile(),
		"agentModels": saved,
	})
}

func (s *Server) agentModels() ([]store.AgentModel, error) {
	rows, err := s.Store.LoadModels()
	if err != nil {
		return nil, err
	}
	out := make([]store.AgentModel, 0, len(rows))
	for _, row := range rows {
		if _, found := agentdef.ByID(row.ID); !found {
			continue
		}
		model, err := console.ResolveModel(s.modelCatalog(), row.Tool, row.Model)
		if err != nil {
			continue
		}
		out = append(out, store.AgentModel{ID: row.ID, Tool: strings.TrimSpace(row.Tool), Model: model})
	}
	return out, nil
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

func (s *Server) handlePreview(w http.ResponseWriter, r *http.Request) {
	prep, ok := s.prepare(w, r)
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
	prep, ok := s.prepare(w, r)
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

func (s *Server) prepare(w http.ResponseWriter, r *http.Request) (launchPrep, bool) {
	agent, found := agentdef.ByID(r.PathValue("id"))
	if !found {
		writeErr(w, http.StatusNotFound, errors.New("unknown agent"))
		return launchPrep{}, false
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
	kind, worktree, err := worktreeOf(agent, snap, repo)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return launchPrep{}, false
	}
	snap.Worktree = worktree
	body, err := agent.Body()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return launchPrep{}, false
	}
	return launchPrep{
		agent:    agent,
		prompt:   console.Prompt(agent, body, snap, note),
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
	if _, err := s.Jobs.Check(ctx, s.Root, foldersOf(snap)); err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	s.handleState(w, r)
}

func (s *Server) handleGitNexus(w http.ResponseWriter, r *http.Request) {
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	job, err := s.Jobs.StartGitNexus(s.Root, snap.WorkspaceID)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusAccepted, job)
}

func (s *Server) handleCodegraph(w http.ResponseWriter, r *http.Request) {
	snap, err := s.load()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	kind := r.PathValue("kind")
	job, err := s.Jobs.StartCodegraph(s.Root, kind, foldersOf(snap)[kind])
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusAccepted, job)
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
		})
	}
	agents := make([]map[string]any, 0, len(agentdef.Catalog))
	for _, agent := range agentdef.Catalog {
		agents = append(agents, map[string]any{
			"id":      agent.ID,
			"title":   agent.Title,
			"group":   agent.Group,
			"file":    "app/agents/" + agent.File,
			"repos":   agent.Repos,
			"peer":    agent.Peer,
			"summary": agent.Summary,
		})
	}
	agentModels, err := s.agentModels()
	if err != nil {
		return nil, err
	}
	return map[string]any{
		"root":        s.Root,
		"setupFile":   s.Store.File(),
		"modelsFile":  s.Store.ModelsFile(),
		"agentModels": agentModels,
		"workspaceId": snap.WorkspaceID,
		"orcaAgent":   snap.OrcaAgent,
		"orcaAgents":  console.OrcaAgents,
		"models":      modelJSON(s.modelCatalog()),
		"repos":       repos,
		"agents":      agents,
		"indexes":     s.indexes(foldersOf(snap), snap.WorkspaceID),
		"jobs":        s.Jobs.List(),
		"launches":    launches,
	}, nil
}

func foldersOf(snap console.Snapshot) map[string]string {
	folders := map[string]string{}
	for _, repo := range snap.Repos {
		folders[repo.Kind] = repo.Folder
	}
	return folders
}

func (s *Server) indexes(folders map[string]string, workspaceID string) map[string]any {
	report := s.Jobs.Last()
	gitFolder := filepath.Join(s.Root, ".gitnexus")
	git := map[string]any{
		"kind":      "gitnexus",
		"dirExists": isDir(gitFolder),
		"ready":     false,
		"checked":   report.GitNexus.CheckedAt != "",
		"summary":   report.GitNexus.Summary,
		"command":   jobs.FormatArgs(toolchain.GitNexusAnalyze(workspaceID)),
		"folder":    s.Root,
	}
	if report.GitNexus.CheckedAt != "" {
		git["ready"] = report.GitNexus.Ready && isDir(gitFolder)
	}
	codegraph := make([]map[string]any, 0, len(toolchain.CodeKinds))
	for _, kind := range toolchain.CodeKinds {
		folder := folders[kind]
		check := report.Codegraph[kind]
		command := "codegraph init <folder>"
		indexDir := ""
		if folder != "" {
			command = jobs.FormatArgs(toolchain.CodegraphInit(folder))
			indexDir = filepath.Join(folder, ".codegraph")
		}
		item := map[string]any{
			"kind":      kind,
			"dirExists": indexDir != "" && isDir(indexDir),
			"ready":     false,
			"checked":   check.CheckedAt != "",
			"summary":   check.Summary,
			"command":   command,
			"folder":    folder,
		}
		if check.CheckedAt != "" {
			item["ready"] = check.Ready && indexDir != "" && isDir(indexDir)
		}
		codegraph = append(codegraph, item)
	}
	return map[string]any{"gitnexus": git, "codegraph": codegraph}
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

func (s *Server) modelCatalog() map[string][]console.Model {
	s.modelsOnce.Do(func() {
		catalog := console.ModelCatalog()
		if live := console.CursorModels(context.Background()); len(live) > 0 {
			catalog["cursor"] = live
		}
		s.models = catalog
	})
	return s.models
}

func modelJSON(catalog map[string][]console.Model) map[string][]map[string]any {
	out := map[string][]map[string]any{}
	for _, tool := range console.OrcaAgents {
		rows := make([]map[string]any, 0, len(catalog[tool]))
		for _, model := range catalog[tool] {
			rows = append(rows, map[string]any{
				"id":      model.ID,
				"label":   model.Label,
				"default": model.Default,
			})
		}
		out[tool] = rows
	}
	return out
}

func worktreeOf(agent agentdef.Agent, snap console.Snapshot, kind string) (string, string, error) {
	if kind == "" {
		if len(agent.Repos) == 1 {
			kind = agent.Repos[0]
		} else {
			return "", "", errors.New("choose the repo folder for this agent")
		}
	}
	allowed := false
	for _, item := range agent.Repos {
		if item == kind {
			allowed = true
		}
	}
	if !allowed {
		return "", "", errors.New("that repo is not used by this agent")
	}
	folder := strings.TrimSpace(snap.Repo(kind).Folder)
	if folder == "" {
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
