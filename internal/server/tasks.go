package server

import (
	"context"
	"errors"
	"net/http"
	"regexp"
	"strings"
	"time"

	"kiki/internal/agentfile"
	"kiki/internal/console"
	"kiki/internal/orca"
	"kiki/internal/store"
)

func (s *Server) handleCreateTask(w http.ResponseWriter, r *http.Request) {
	in, ok := s.readTask(w, r, "")
	if !ok {
		return
	}
	task, err := s.Store.CreateTask(in)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	writeJSON(w, http.StatusCreated, map[string]any{"task": task})
}

func (s *Server) handleUpdateTask(w http.ResponseWriter, r *http.Request) {
	id, ok := taskID(w, r)
	if !ok {
		return
	}
	in, ok := s.readTask(w, r, id)
	if !ok {
		return
	}
	task, err := s.Store.UpdateTask(id, in)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"task": task})
}

func (s *Server) handleDeleteTask(w http.ResponseWriter, r *http.Request) {
	id, ok := taskID(w, r)
	if !ok {
		return
	}
	task, err := s.Store.Task(id)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	if task.Started || strings.TrimSpace(task.BranchSlug) != "" {
		ctx, cancel := context.WithTimeout(r.Context(), 2*time.Minute)
		defer cancel()
		if err := s.removeTaskFromOrca(ctx, task); err != nil {
			if task.Started || !strings.Contains(err.Error(), "Orca is not running") {
				writeErr(w, http.StatusBadGateway, err)
				return
			}
		}
	}
	if err := s.Store.DeleteTask(id); err != nil {
		writeTaskErr(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true})
}

func (s *Server) removeTaskFromOrca(ctx context.Context, task store.Task) error {
	snap, err := s.load()
	if err != nil {
		return err
	}
	kinds, err := s.taskRepoKinds(task)
	if err != nil {
		return err
	}
	seen := map[string]bool{}
	var terminals []string
	for _, run := range task.Runs {
		handle := strings.TrimSpace(run.Terminal)
		if handle == "" || seen[handle] {
			continue
		}
		seen[handle] = true
		terminals = append(terminals, handle)
	}
	ticket := jiraTicket(task.Name)
	if ticket == "" {
		ticket = jiraTicket(task.Description)
	}
	client := s.Orca
	if client == nil {
		client = &orca.Client{}
	}
	keepGroup, keepRepos, err := s.orcaGroupStillUsed(task)
	if err != nil {
		return err
	}
	folders := map[string]string{}
	for _, kind := range kinds {
		folders[kind] = strings.TrimSpace(snap.Repo(kind).Folder)
	}
	return client.RemoveTask(ctx, orca.TaskScope{
		ID:        task.ID,
		GroupID:   task.GroupID,
		Workspace: snap.WorkspaceID,
		Slug:      task.BranchSlug,
		Ticket:    ticket,
		Repos:     kinds,
		Folders:   folders,
		Terminals: terminals,
		KeepGroup: keepGroup,
		KeepRepos: keepRepos,
	})
}

func (s *Server) orcaGroupStillUsed(task store.Task) (bool, []string, error) {
	groupID := strings.TrimSpace(task.GroupID)
	if groupID == "" || s.Store == nil {
		return false, nil, nil
	}
	tasks, err := s.Store.Tasks()
	if err != nil {
		return false, nil, err
	}
	keepGroup := false
	seen := map[string]bool{}
	var keepRepos []string
	for _, other := range tasks {
		if other.ID == task.ID || strings.TrimSpace(other.GroupID) != groupID {
			continue
		}
		if !other.Started && strings.TrimSpace(other.BranchSlug) == "" {
			continue
		}
		keepGroup = true
		kinds, err := s.taskRepoKinds(other)
		if err != nil {
			return false, nil, err
		}
		for _, kind := range kinds {
			if seen[kind] {
				continue
			}
			seen[kind] = true
			keepRepos = append(keepRepos, kind)
		}
	}
	return keepGroup, keepRepos, nil
}

func (s *Server) taskRepoKinds(task store.Task) ([]string, error) {
	dir, err := s.agentDir()
	if err != nil {
		return nil, err
	}
	files, err := agentfile.List(dir)
	if err != nil {
		return nil, err
	}
	byID := make(map[string]agentfile.File, len(files))
	for _, file := range files {
		byID[file.ID] = file
	}
	var kinds []string
	seen := map[string]bool{}
	for _, agent := range task.Agents {
		file, ok := byID[agent.AgentID]
		if !ok || len(file.Repos) == 0 {
			continue
		}
		kind := strings.TrimSpace(file.Repos[0].Kind)
		if kind == "" || seen[kind] {
			continue
		}
		seen[kind] = true
		kinds = append(kinds, kind)
	}
	return kinds, nil
}

func taskID(w http.ResponseWriter, r *http.Request) (string, bool) {
	id := strings.TrimSpace(r.PathValue("id"))
	if !orca.ValidTaskID(id) {
		writeErr(w, http.StatusNotFound, store.ErrTaskNotFound)
		return "", false
	}
	return id, true
}

func (s *Server) readTask(w http.ResponseWriter, r *http.Request, id string) (store.TaskInput, bool) {
	var body struct {
		Name        string `json:"name"`
		Description string `json:"description"`
		Column      string `json:"column"`
		GroupID     string `json:"groupId"`
		Agents      []struct {
			AgentID string `json:"agentId"`
			Status  string `json:"status"`
		} `json:"agents"`
	}
	if !readJSON(w, r, &body) {
		return store.TaskInput{}, false
	}
	in := store.TaskInput{
		Name:        body.Name,
		Description: body.Description,
		Column:      body.Column,
		GroupID:     body.GroupID,
		Agents:      make([]store.TaskAgent, 0, len(body.Agents)),
	}
	for _, agent := range body.Agents {
		in.Agents = append(in.Agents, store.TaskAgent{AgentID: agent.AgentID, Status: agent.Status})
	}
	if err := s.checkTaskAgents(id, in.Agents); err != nil {
		writeTaskErr(w, err)
		return store.TaskInput{}, false
	}
	return in, true
}

func (s *Server) checkTaskAgents(id string, agents []store.TaskAgent) error {
	known, err := s.knownAgentIDs()
	if err != nil {
		return err
	}
	kept := map[string]bool{}
	if id != "" {
		current, err := s.Store.Task(id)
		if err != nil {
			return err
		}
		for _, agent := range current.Agents {
			kept[agent.AgentID] = true
		}
	}
	for _, agent := range agents {
		agentID := strings.TrimSpace(agent.AgentID)
		if agentID == "" || known[agentID] || kept[agentID] {
			continue
		}
		return store.TaskError{Message: agentID + " is not an agent in this app."}
	}
	return nil
}

func (s *Server) knownAgentIDs() (map[string]bool, error) {
	dir, err := s.agentDir()
	if err != nil {
		return nil, err
	}
	files, err := agentfile.List(dir)
	if err != nil {
		return nil, err
	}
	known := make(map[string]bool, len(files))
	for _, file := range files {
		known[file.ID] = true
	}
	return known, nil
}

var jiraTicketPattern = regexp.MustCompile(`(?i)\b[A-Z][A-Z0-9]{1,9}-[0-9]+\b`)

func jiraTicket(text string) string {
	return strings.ToUpper(jiraTicketPattern.FindString(text))
}

func branchSlug(value string) string {
	value = strings.TrimSpace(value)
	if cut := strings.IndexAny(value, "\r\n"); cut >= 0 {
		value = value[:cut]
	}
	value = strings.Trim(value, " \t`\"'")
	var b strings.Builder
	underscore := false
	for _, r := range strings.ToLower(value) {
		if r == 'đ' {
			r = 'd'
		}
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
			underscore = false
			continue
		}
		if b.Len() > 0 && !underscore {
			b.WriteByte('_')
			underscore = true
		}
	}
	slug := strings.Trim(b.String(), "_")
	if len(slug) > 80 {
		slug = strings.TrimRight(slug[:80], "_")
	}
	return slug
}

func (s *Server) handleTaskBranch(w http.ResponseWriter, r *http.Request) {
	id, ok := taskID(w, r)
	if !ok {
		return
	}
	task, err := s.Store.Task(id)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	ticket := jiraTicket(task.Name)
	if ticket == "" {
		ticket = jiraTicket(task.Description)
	}
	slug := task.BranchSlug
	if slug == "" || task.BranchFor != task.Name || !orca.ValidBranchSlug(slug) {
		slug, err = s.englishBranchSlug(r.Context(), task.Name)
		if err != nil {
			writeChoiceErr(w, err)
			return
		}
		if !orca.ValidBranchSlug(slug) {
			writeErr(w, http.StatusBadRequest, store.TaskError{Message: "The branch name could not be made from this task name."})
			return
		}
		task, err = s.Store.SetTaskBranch(id, slug, task.Name)
		if err != nil {
			writeTaskErr(w, err)
			return
		}
		slug = task.BranchSlug
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"taskId": task.ID,
		"slug":   slug,
		"ticket": ticket,
	})
}

func (s *Server) englishBranchSlug(ctx context.Context, name string) (string, error) {
	tool, model := "", ""
	if s.BranchNamer == nil {
		var err error
		tool, model, err = s.generateChoice()
		if err != nil {
			return "", err
		}
	}
	ctx, cancel := context.WithTimeout(ctx, 2*time.Minute)
	defer cancel()
	var english string
	var err error
	if s.BranchNamer != nil {
		english, err = s.BranchNamer(ctx, tool, model, name)
	} else {
		english, err = console.GenerateText(ctx, tool, model, "branch", "", name)
	}
	if err != nil {
		return "", err
	}
	return branchSlug(english), nil
}

func writeTaskErr(w http.ResponseWriter, err error) {
	if errors.Is(err, store.ErrTaskNotFound) {
		writeErr(w, http.StatusNotFound, err)
		return
	}
	var input store.TaskError
	if errors.As(err, &input) {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeErr(w, http.StatusInternalServerError, err)
}
