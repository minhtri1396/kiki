package server

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"kiki/internal/orca"
	"kiki/internal/store"
)

func TestOrcaRunStatus(t *testing.T) {
	started := time.Now().Add(-time.Minute).UTC().Format(time.RFC3339)
	if got := orcaRunStatus("exited", false, started, time.Now()); got != "exited" {
		t.Fatalf("exited = %s", got)
	}
	if got := orcaRunStatus("running", true, time.Now().UTC().Format(time.RFC3339), time.Now()); got != "running" {
		t.Fatalf("fresh idle = %s", got)
	}
	if got := orcaRunStatus("running", true, started, time.Now()); got != "idle" {
		t.Fatalf("idle = %s", got)
	}
	if got := orcaRunStatus("running", false, started, time.Now()); got != "running" {
		t.Fatalf("busy = %s", got)
	}
}

func TestSyncTaskRuns(t *testing.T) {
	app := newTaskTestServer(t)
	script := filepath.Join(t.TempDir(), "orca")
	if err := os.WriteFile(script, []byte(`#!/bin/sh
if [ "$1" = "terminal" ] && [ "$2" = "read" ]; then
  printf '%s\n' '{"ok":true,"result":{"terminal":{"handle":"term_a","status":"exited","tail":["hello","world"]}}}'
  exit 0
fi
printf '%s\n' '{"ok":false,"error":{"code":"timeout","message":"timeout"}}'
`), 0o755); err != nil {
		t.Fatal(err)
	}
	app.Orca = &orca.Client{Bin: script}
	req := httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"Ship billing","agents":[{"agentId":"backend_runner","status":"doing"}]}`))
	rec := httptest.NewRecorder()
	app.handleCreateTask(rec, req)
	if rec.Code != http.StatusCreated {
		t.Fatalf("create status = %d body = %s", rec.Code, rec.Body.String())
	}
	var created struct {
		Task store.Task `json:"task"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &created); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTaskRun(created.Task.ID, "backend_runner", "term_a"); err != nil {
		t.Fatal(err)
	}
	req = httptest.NewRequest(http.MethodPost, "/api/tasks/sync", nil)
	rec = httptest.NewRecorder()
	app.handleSyncTasks(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("sync status = %d body = %s", rec.Code, rec.Body.String())
	}
	var synced struct {
		Tasks []store.Task `json:"tasks"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &synced); err != nil {
		t.Fatal(err)
	}
	if len(synced.Tasks) != 1 || len(synced.Tasks[0].Agents) != 1 || synced.Tasks[0].Agents[0].Status != store.TaskReview {
		t.Fatalf("tasks = %+v", synced.Tasks)
	}
	if len(synced.Tasks[0].Runs) != 1 || synced.Tasks[0].Runs[0].Status != "exited" || synced.Tasks[0].Runs[0].Log != "hello\nworld" {
		t.Fatalf("runs = %+v", synced.Tasks[0].Runs)
	}
}

func TestTaskReplyAndApprove(t *testing.T) {
	app := newTaskTestServer(t)
	logPath := filepath.Join(t.TempDir(), "args")
	script := filepath.Join(t.TempDir(), "orca")
	body := "#!/bin/sh\nprintf '%s\\n' \"$*\" >> " + logPath + "\nprintf '%s\\n' '{\"ok\":true}'\n"
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	app.Orca = &orca.Client{Bin: script}
	req := httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"Ship billing","agents":[{"agentId":"backend_runner","status":"doing"}]}`))
	rec := httptest.NewRecorder()
	app.handleCreateTask(rec, req)
	if rec.Code != http.StatusCreated {
		t.Fatalf("create status = %d body = %s", rec.Code, rec.Body.String())
	}
	var created struct {
		Task store.Task `json:"task"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &created); err != nil {
		t.Fatal(err)
	}
	id := created.Task.ID
	if err := app.Store.SaveTaskRun(created.Task.ID, "backend_runner", "term_a"); err != nil {
		t.Fatal(err)
	}
	req = httptest.NewRequest(http.MethodPost, "/api/tasks/"+id+"/agents/backend_runner/reply", strings.NewReader(`{"text":"keep going"}`))
	req.SetPathValue("id", id)
	req.SetPathValue("agentId", "backend_runner")
	rec = httptest.NewRecorder()
	app.handleTaskReply(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("early reply status = %d body = %s", rec.Code, rec.Body.String())
	}
	if _, err := app.Store.RecordTaskRun(created.Task.ID, "backend_runner", "idle", "Thinking\nTwo shapes fit this task.", store.TaskReview, "Which one should I write?", []string{"Short brief", "Full specification"}); err != nil {
		t.Fatal(err)
	}
	req = httptest.NewRequest(http.MethodPost, "/api/tasks/"+id+"/agents/backend_runner/reply", strings.NewReader(`{"text":"Short brief"}`))
	req.SetPathValue("id", id)
	req.SetPathValue("agentId", "backend_runner")
	rec = httptest.NewRecorder()
	app.handleTaskReply(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("reply status = %d body = %s", rec.Code, rec.Body.String())
	}
	var replied struct {
		Task store.Task `json:"task"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &replied); err != nil {
		t.Fatal(err)
	}
	if replied.Task.Agents[0].Status != store.TaskDoing || replied.Task.Runs[0].AskPrompt != "" || replied.Task.Runs[0].Status != "running" {
		t.Fatalf("replied = %+v", replied.Task)
	}
	sent, err := os.ReadFile(logPath)
	if err != nil || !strings.Contains(string(sent), "--text Short brief --enter") {
		t.Fatalf("sent = %q err = %v", sent, err)
	}
	if _, err := app.Store.RecordTaskRun(created.Task.ID, "backend_runner", "exited", "I wrote the short brief.", store.TaskReview, "", nil); err != nil {
		t.Fatal(err)
	}
	req = httptest.NewRequest(http.MethodPost, "/api/tasks/"+id+"/agents/backend_runner/approve", nil)
	req.SetPathValue("id", id)
	req.SetPathValue("agentId", "backend_runner")
	rec = httptest.NewRecorder()
	app.handleTaskApprove(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("approve status = %d body = %s", rec.Code, rec.Body.String())
	}
	var approved struct {
		Task store.Task `json:"task"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &approved); err != nil {
		t.Fatal(err)
	}
	if approved.Task.Agents[0].Status != store.TaskCompleted {
		t.Fatalf("approved = %+v", approved.Task.Agents)
	}
}
