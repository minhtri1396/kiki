package server

import (
	"context"
	"net/http"
	"strings"
	"time"

	"kiki/internal/orca"
	"kiki/internal/store"
)

const runIdleGrace = 20 * time.Second

func (s *Server) handleSyncTasks(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), 25*time.Second)
	defer cancel()
	tasks, err := s.syncTaskRuns(ctx)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if tasks == nil {
		tasks = []store.Task{}
	}
	writeJSON(w, http.StatusOK, map[string]any{"tasks": tasks})
}

func (s *Server) rememberTaskRun(prep launchPrep, raw string) error {
	if strings.TrimSpace(prep.taskID) == "" {
		return nil
	}
	if !orca.ValidTaskID(prep.taskID) {
		return store.TaskError{Message: "Task ID is not valid."}
	}
	handle := orca.TerminalHandle(raw)
	if handle == "" {
		return store.TaskError{Message: "Orca did not return a terminal for the agent."}
	}
	return s.Store.SaveTaskRun(prep.taskID, prep.agent.ID, handle)
}

func (s *Server) syncTaskRuns(ctx context.Context) ([]store.Task, error) {
	runs, err := s.Store.OpenRuns()
	if err != nil {
		return nil, err
	}
	client := s.Orca
	if client == nil {
		client = &orca.Client{}
	}
	seen := map[string]store.Task{}
	var order []string
	for _, run := range runs {
		if ctx.Err() != nil {
			break
		}
		task, ok := s.refreshRun(ctx, client, run)
		if !ok {
			continue
		}
		if _, exists := seen[task.ID]; !exists {
			order = append(order, task.ID)
		}
		seen[task.ID] = task
	}
	out := make([]store.Task, 0, len(order))
	for _, id := range order {
		out = append(out, seen[id])
	}
	return out, nil
}

func (s *Server) refreshRun(ctx context.Context, client *orca.Client, run store.TaskRun) (store.Task, bool) {
	callCtx, cancel := context.WithTimeout(ctx, 8*time.Second)
	view, err := client.ReadTerminal(callCtx, run.Terminal)
	cancel()
	if err != nil {
		return store.Task{}, false
	}
	idle := false
	if view.Status == "running" {
		callCtx, cancel = context.WithTimeout(ctx, 8*time.Second)
		idle, err = client.TerminalIdle(callCtx, run.Terminal)
		cancel()
		if err != nil {
			idle = false
		}
	}
	text, ask := orca.Transcript(view.Log)
	status := orcaRunStatus(view.Status, idle, run.StartedAt, time.Now())
	if idle && strings.TrimSpace(text) != "" && view.Status == "running" {
		status = "idle"
	}
	phase := ""
	switch {
	case strings.TrimSpace(ask.Prompt) != "":
		phase = store.TaskReview
	case status == "idle" || status == "exited":
		phase = store.TaskReview
	case status == "running":
		phase = store.TaskDoing
	}
	task, err := s.Store.RecordTaskRun(run.TaskID, run.AgentID, status, text, phase, ask.Prompt, ask.Options)
	if err != nil {
		return store.Task{}, false
	}
	return task, true
}

func (s *Server) handleTaskReply(w http.ResponseWriter, r *http.Request) {
	id, ok := taskID(w, r)
	if !ok {
		return
	}
	agentID := strings.TrimSpace(r.PathValue("agentId"))
	var body struct {
		Text string `json:"text"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	text := strings.TrimSpace(body.Text)
	if text == "" {
		writeTaskErr(w, store.TaskError{Message: "Enter a reply."})
		return
	}
	task, err := s.Store.Task(id)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	run, err := reviewRun(task, agentID)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 20*time.Second)
	defer cancel()
	client := s.Orca
	if client == nil {
		client = &orca.Client{}
	}
	if err := client.SendText(ctx, run.Terminal, text); err != nil {
		writeErr(w, http.StatusBadGateway, err)
		return
	}
	task, err = s.Store.ResumeAgent(id, agentID)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"task": task})
}

func (s *Server) handleTaskApprove(w http.ResponseWriter, r *http.Request) {
	id, ok := taskID(w, r)
	if !ok {
		return
	}
	agentID := strings.TrimSpace(r.PathValue("agentId"))
	task, err := s.Store.ApproveAgent(id, agentID)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"task": task})
}

func reviewRun(task store.Task, agentID string) (store.TaskRun, error) {
	waiting := false
	for _, agent := range task.Agents {
		if agent.AgentID == agentID {
			waiting = agent.Status == store.TaskReview
		}
	}
	if !waiting {
		return store.TaskRun{}, store.TaskError{Message: "This agent is not waiting for review."}
	}
	for _, run := range task.Runs {
		if run.AgentID == agentID && strings.TrimSpace(run.Terminal) != "" {
			return run, nil
		}
	}
	return store.TaskRun{}, store.TaskError{Message: "This agent has not been started."}
}

func orcaRunStatus(process string, idle bool, startedAt string, now time.Time) string {
	switch process {
	case "exited":
		return "exited"
	case "running":
		if idle {
			started, err := time.Parse(time.RFC3339, startedAt)
			if err == nil && now.Sub(started) >= runIdleGrace {
				return "idle"
			}
		}
		return "running"
	default:
		return "unknown"
	}
}
