package server

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/agentfile"
	"kiki/internal/jobs"
	"kiki/internal/mcp"
	"kiki/internal/orca"
	"kiki/internal/store"
)

func newTaskTestServer(t *testing.T) *Server {
	t.Helper()
	root := t.TempDir()
	db, err := store.Open(filepath.Join(root, ".kiki", "kiki.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	dir, err := agentfile.Resolve(root, "")
	if err != nil {
		t.Fatal(err)
	}
	if err := agentfile.Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	return &Server{Root: root, Store: db, Jobs: jobs.New()}
}

func TestTaskAPI(t *testing.T) {
	app := newTaskTestServer(t)
	state, err := app.snapshot()
	if err != nil {
		t.Fatal(err)
	}
	if tasks, ok := state["tasks"].([]store.Task); !ok || len(tasks) != 0 {
		t.Fatalf("tasks = %#v", state["tasks"])
	}

	req := httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"Ship billing","description":"Add invoices.","agents":[{"agentId":"requirement_planner","status":"pending"},{"agentId":"backend_runner","status":"doing"}]}`))
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
	if created.Task.Column != store.ColumnTodo || len(created.Task.Agents) != 2 {
		t.Fatalf("created = %+v", created.Task)
	}

	id := created.Task.ID
	req = httptest.NewRequest(http.MethodPut, "/api/tasks/"+id, strings.NewReader(`{"name":"Ship billing","description":"Add invoices.","column":"done","agents":[{"agentId":"backend_runner","status":"completed"}]}`))
	req.SetPathValue("id", id)
	rec = httptest.NewRecorder()
	app.handleUpdateTask(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("update status = %d body = %s", rec.Code, rec.Body.String())
	}
	var updated struct {
		Task store.Task `json:"task"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &updated); err != nil {
		t.Fatal(err)
	}
	if updated.Task.Column != store.ColumnTodo || updated.Task.DoneAt != "" || updated.Task.Started || len(updated.Task.Agents) != 1 || updated.Task.Agents[0].Status != store.TaskPending {
		t.Fatalf("updated = %+v", updated.Task)
	}

	req = httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"","description":"x"}`))
	rec = httptest.NewRecorder()
	app.handleCreateTask(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("empty name status = %d body = %s", rec.Code, rec.Body.String())
	}

	req = httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"Missing","agents":[{"agentId":"not_an_agent","status":"pending"}]}`))
	rec = httptest.NewRecorder()
	app.handleCreateTask(rec, req)
	if rec.Code != http.StatusBadRequest || !strings.Contains(rec.Body.String(), "not an agent") {
		t.Fatalf("unknown agent status = %d body = %s", rec.Code, rec.Body.String())
	}

	state, err = app.snapshot()
	if err != nil {
		t.Fatal(err)
	}
	tasks := state["tasks"].([]store.Task)
	if len(tasks) != 1 || tasks[0].Name != "Ship billing" {
		t.Fatalf("snapshot tasks = %+v", tasks)
	}

	req = httptest.NewRequest(http.MethodDelete, "/api/tasks/"+id, nil)
	req.SetPathValue("id", id)
	rec = httptest.NewRecorder()
	app.handleDeleteTask(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("delete status = %d body = %s", rec.Code, rec.Body.String())
	}
	left, err := app.Store.Tasks()
	if err != nil {
		t.Fatal(err)
	}
	if len(left) != 0 {
		t.Fatalf("left = %+v", left)
	}
}

func TestTaskBranch(t *testing.T) {
	app := newTaskTestServer(t)
	calls := 0
	app.BranchNamer = func(ctx context.Context, tool, model, name string) (string, error) {
		calls++
		if name == "Ship the cart" {
			return "Ship the cart", nil
		}
		return "Add shopping cart.", nil
	}
	req := httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"GE-12 Thêm giỏ hàng","description":"See also ZZ-9."}`))
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
	branch := func() map[string]any {
		t.Helper()
		req := httptest.NewRequest(http.MethodPost, "/api/tasks/"+id+"/branch", nil)
		req.SetPathValue("id", id)
		rec := httptest.NewRecorder()
		app.handleTaskBranch(rec, req)
		if rec.Code != http.StatusOK {
			t.Fatalf("branch status = %d body = %s", rec.Code, rec.Body.String())
		}
		var body map[string]any
		if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
			t.Fatal(err)
		}
		return body
	}
	first := branch()
	if first["slug"] != "add_shopping_cart" || first["ticket"] != "GE-12" || calls != 1 {
		t.Fatalf("first = %#v calls = %d", first, calls)
	}
	second := branch()
	if second["slug"] != "add_shopping_cart" || calls != 1 {
		t.Fatalf("second = %#v calls = %d", second, calls)
	}
	if _, err := app.Store.UpdateTask(created.Task.ID, store.TaskInput{Name: "Ship the cart", Description: "Follow ZZ-9."}); err != nil {
		t.Fatal(err)
	}
	third := branch()
	if third["slug"] != "ship_the_cart" || third["ticket"] != "ZZ-9" || calls != 2 {
		t.Fatalf("third = %#v calls = %d", third, calls)
	}
}

func TestDeleteTaskRemovesOrca(t *testing.T) {
	app := newTaskTestServer(t)
	group, err := app.Store.CreateGroup(store.GroupInput{Name: "Billing", EpicKey: "KIKI-8", EpicBoardID: "12", EpicBoardName: "KIKI"})
	if err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"GE-12 Ship billing","description":"Add invoices.","groupId":"`+group.ID+`","agents":[{"agentId":"requirement_planner"}]}`))
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
	if _, err := app.Store.SetTaskBranch(created.Task.ID, "ship_billing", "GE-12 Ship billing"); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTaskRun(created.Task.ID, "requirement_planner", "term_plan"); err != nil {
		t.Fatal(err)
	}
	id := created.Task.ID
	log := filepath.Join(t.TempDir(), "args")
	script := filepath.Join(t.TempDir(), "orca")
	body := `#!/bin/sh
printf '%s\n' "$*" >> "` + log + `"
if [ "$1" = "status" ]; then
  printf '%s\n' '{"ok":true,"result":{"app":{"running":true}}}'
  exit 0
fi
if [ "$1" = "project" ] && [ "$2" = "setups" ]; then
  printf '%s\n' '{"ok":true,"result":{"setups":[{"id":"setup_doc","displayName":"kiki-` + group.ID + `-document","projectId":"p1","path":"/tmp/doc"}]}}'
  exit 0
fi
printf '%s\n' '{"ok":true}'
`
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	var methods []string
	app.Orca = &orca.Client{
		Bin: script,
		Runtime: func(ctx context.Context, method string, params any) (json.RawMessage, error) {
			methods = append(methods, method)
			if method == "projectGroup.list" {
				return json.RawMessage(`{"groups":[{"id":"g1","name":"` + group.ID + `"}]}`), nil
			}
			return json.RawMessage(`{}`), nil
		},
	}
	req = httptest.NewRequest(http.MethodDelete, "/api/tasks/"+id, nil)
	req.SetPathValue("id", id)
	rec = httptest.NewRecorder()
	app.handleDeleteTask(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("delete status = %d body = %s", rec.Code, rec.Body.String())
	}
	text, err := os.ReadFile(log)
	if err != nil {
		t.Fatal(err)
	}
	got := string(text)
	for _, want := range []string{
		"terminal close --terminal term_plan --tab --json",
		"worktree rm --worktree branch:feat/GE-12-ship_billing-docs --json",
		"project setup-delete --setup setup_doc --force --json",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("log missing %q\n%s", want, got)
		}
	}
	if strings.Join(methods, ",") != "projectGroup.list,projectGroup.delete" {
		t.Fatalf("methods = %v", methods)
	}
	left, err := app.Store.Tasks()
	if err != nil {
		t.Fatal(err)
	}
	if len(left) != 0 {
		t.Fatalf("left = %+v", left)
	}

	req = httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"GE-9 Keep","description":"Stay.","agents":[{"agentId":"requirement_planner"}]}`))
	rec = httptest.NewRecorder()
	app.handleCreateTask(rec, req)
	if err := json.Unmarshal(rec.Body.Bytes(), &created); err != nil {
		t.Fatal(err)
	}
	if _, err := app.Store.SetTaskBranch(created.Task.ID, "keep_me", "GE-9 Keep"); err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveTaskRun(created.Task.ID, "requirement_planner", "term_keep"); err != nil {
		t.Fatal(err)
	}
	keep := created.Task.ID
	fail := filepath.Join(t.TempDir(), "fail-orca")
	if err := os.WriteFile(fail, []byte(`#!/bin/sh
if [ "$1" = "status" ]; then
  printf '%s\n' '{"ok":true,"result":{"app":{"running":true}}}'
  exit 0
fi
printf '%s\n' '{"ok":false,"error":{"code":"runtime_error","message":"still open"}}'
`), 0o755); err != nil {
		t.Fatal(err)
	}
	app.Orca = &orca.Client{Bin: fail}
	req = httptest.NewRequest(http.MethodDelete, "/api/tasks/"+keep, nil)
	req.SetPathValue("id", keep)
	rec = httptest.NewRecorder()
	app.handleDeleteTask(rec, req)
	if rec.Code != http.StatusBadGateway {
		t.Fatalf("failed delete status = %d body = %s", rec.Code, rec.Body.String())
	}
	kept, err := app.Store.Task(created.Task.ID)
	if err != nil || !kept.Started || kept.Name != "GE-9 Keep" {
		t.Fatalf("kept = %+v err = %v", kept, err)
	}
}

func TestTaskBelongsToGroup(t *testing.T) {
	app := newTaskTestServer(t)
	state, err := app.snapshot()
	if err != nil {
		t.Fatal(err)
	}
	if groups, ok := state["groups"].([]store.Group); !ok || len(groups) != 0 {
		t.Fatalf("groups = %#v", state["groups"])
	}
	group, err := app.Store.CreateGroup(store.GroupInput{Name: "Billing", EpicKey: "KIKI-8", EpicBoardID: "12", EpicBoardName: "KIKI"})
	if err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPost, "/api/tasks", strings.NewReader(`{"name":"Invoice page","description":"Add it.","groupId":"`+group.ID+`"}`))
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
	if created.Task.GroupID != group.ID || created.Task.EpicKey != "KIKI-8" || created.Task.EpicOwner {
		t.Fatalf("created = %+v", created.Task)
	}
	req = httptest.NewRequest(http.MethodPost, "/api/groups", strings.NewReader(`{"name":"Accounts","description":"No Jira."}`))
	rec = httptest.NewRecorder()
	app.handleCreateGroup(rec, req)
	if rec.Code != http.StatusCreated {
		t.Fatalf("group status = %d body = %s", rec.Code, rec.Body.String())
	}
	var made struct {
		Group store.Group `json:"group"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &made); err != nil {
		t.Fatal(err)
	}
	if made.Group.Name != "Accounts" || made.Group.EpicKey != "" || made.Group.Description != "No Jira." {
		t.Fatalf("group = %+v", made.Group)
	}
}

func TestDeleteGroupKeepsTaskOnTheBoard(t *testing.T) {
	app := newTaskTestServer(t)
	group, err := app.Store.CreateGroup(store.GroupInput{Name: "Billing", EpicKey: "KIKI-8", EpicBoardID: "12", EpicBoardName: "KIKI"})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := app.Store.CreateTask(store.TaskInput{Name: "Invoice page", GroupID: group.ID}); err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodDelete, "/api/groups/"+group.ID, nil)
	rec := httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	state, err := app.snapshot()
	if err != nil {
		t.Fatal(err)
	}
	groups, ok := state["groups"].([]store.Group)
	if !ok || len(groups) != 1 || !groups[0].Deleted || groups[0].Name != "Billing" || groups[0].EpicKey != "KIKI-8" {
		t.Fatalf("groups = %#v", state["groups"])
	}
	tasks, ok := state["tasks"].([]store.Task)
	if !ok || len(tasks) != 1 || tasks[0].GroupID != group.ID || tasks[0].EpicKey != "KIKI-8" {
		t.Fatalf("tasks = %#v", state["tasks"])
	}
	req = httptest.NewRequest(http.MethodDelete, "/api/groups/01ARZ3NDEKTSV4RRFFQ69G5FAV", nil)
	rec = httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusNotFound {
		t.Fatalf("missing status = %d body = %s", rec.Code, rec.Body.String())
	}
}

func TestDeleteGroupCanDeleteTheJiraTicket(t *testing.T) {
	app := newTaskTestServer(t)
	if err := app.Store.SaveAtlassian(mcp.Atlassian{
		Enabled:  true,
		Username: "a@acme.co",
		APIToken: "secret-jira",
		Jira:     mcp.Service{Enabled: true, URL: "https://acme.atlassian.net"},
	}); err != nil {
		t.Fatal(err)
	}
	group, err := app.Store.CreateGroup(store.GroupInput{Name: "Billing", EpicKey: "KIKI-12", EpicBoardID: "12", EpicBoardName: "KIKI"})
	if err != nil {
		t.Fatal(err)
	}
	previous := mcp.DeleteJSON
	var deleted string
	mcp.DeleteJSON = func(ctx context.Context, rawURL, user, token string) (int, []byte, error) {
		deleted = rawURL
		return http.StatusNoContent, nil, nil
	}
	t.Cleanup(func() { mcp.DeleteJSON = previous })

	req := httptest.NewRequest(http.MethodDelete, "/api/groups/"+group.ID, strings.NewReader(`{"jira":true}`))
	rec := httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if deleted != "https://acme.atlassian.net/rest/api/3/issue/KIKI-12" {
		t.Fatalf("deleted = %s", deleted)
	}
	saved, err := app.Store.Group(group.ID)
	if err != nil || !saved.Deleted {
		t.Fatalf("group = %+v err = %v", saved, err)
	}

	other, err := app.Store.CreateGroup(store.GroupInput{Name: "Accounts", EpicKey: "KIKI-13", EpicBoardID: "12", EpicBoardName: "KIKI"})
	if err != nil {
		t.Fatal(err)
	}
	mcp.DeleteJSON = func(ctx context.Context, rawURL, user, token string) (int, []byte, error) {
		return http.StatusBadRequest, []byte(`{"errorMessages":["The issue can not be deleted because it has sub-tasks."]}`), nil
	}
	req = httptest.NewRequest(http.MethodDelete, "/api/groups/"+other.ID, strings.NewReader(`{"jira":true}`))
	rec = httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusBadRequest || !strings.Contains(rec.Body.String(), "sub-tasks") {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	kept, err := app.Store.Group(other.ID)
	if err != nil || kept.Deleted {
		t.Fatalf("group = %+v err = %v", kept, err)
	}
}
