package server

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/agentfile"
)

func TestToolsMustBeSetUpBeforeLaunch(t *testing.T) {
	err := toolsReady([]string{"preview"}, nil)
	if err == nil || !strings.Contains(err.Error(), "preview") {
		t.Fatalf("missing tool = %v", err)
	}
	if err := toolsReady(nil, nil); err != nil {
		t.Fatal(err)
	}
}

func TestGuardrailBlocksWhatTheAgentDidNotSelect(t *testing.T) {
	app := newTeamTestServer(t)
	if err := app.Store.EnsureLayout(app.Root); err != nil {
		t.Fatal(err)
	}
	if _, err := EnsureAgentFiles(app.Root); err != nil {
		t.Fatal(err)
	}
	dir, err := agentfile.Resolve(app.Root, "")
	if err != nil {
		t.Fatal(err)
	}
	files, err := agentfile.List(dir)
	if err != nil {
		t.Fatal(err)
	}
	var reader agentfile.File
	var other agentfile.File
	for _, file := range files {
		switch file.ID {
		case "requirement_document":
			file.Enabled = true
			file.Repos = []agentfile.Repository{{Kind: "document"}}
			file.MCPs = nil
			file.Tools = nil
			file.WorksWith = nil
			reader = file
		case "requirement_planner":
			file.Enabled = false
			file.Repos = []agentfile.Repository{{Kind: "document"}}
			file.MCPs = nil
			other = file
		default:
			file.Enabled = false
		}
		if err := agentfile.Save(dir, file.ID, file); err != nil {
			t.Fatalf("save %s: %v", file.ID, err)
		}
	}
	if reader.ID == "" || other.ID == "" {
		t.Fatal("missing agents")
	}
	snap, err := app.load()
	if err != nil {
		t.Fatal(err)
	}
	doc := snap.Repo("document").Folder
	backend := snap.Repo("backend").Folder
	if doc == "" || backend == "" {
		t.Fatalf("folders document=%q backend=%q", doc, backend)
	}

	blocked := askGuard(t, app, doc, "", "beforeMCPExecution", `{"server":"mcp-atlassian-kiki","tool_name":"jira_search"}`)
	if blocked.allow || !strings.Contains(blocked.message, "mcp-atlassian-kiki") {
		t.Fatalf("mcp = %+v", blocked)
	}
	file, err := json.Marshal(map[string]string{"file_path": filepath.Join(backend, "main.go")})
	if err != nil {
		t.Fatal(err)
	}
	outside := askGuard(t, app, doc, "", "beforeReadFile", string(file))
	if outside.allow || !strings.Contains(outside.message, backend) {
		t.Fatalf("repo = %+v", outside)
	}
	peer := askGuard(t, app, doc, "", "subagentStart", `{"subagent_type":"requirement_planner"}`)
	if peer.allow || !strings.Contains(peer.message, "requirement_planner") {
		t.Fatalf("agent = %+v", peer)
	}

	reader.MCPs = []agentfile.Use{{ID: "atlassian_jira"}}
	reader.WorksWith = []agentfile.Collaborator{{ID: "requirement_planner"}}
	if err := agentfile.Save(dir, reader.ID, reader); err != nil {
		t.Fatal(err)
	}
	allowed := askGuard(t, app, doc, "", "beforeMCPExecution", `{"server":"mcp-atlassian-kiki","tool_name":"jira_search"}`)
	if !allowed.allow {
		t.Fatalf("jira = %+v", allowed)
	}
	page := askGuard(t, app, doc, "", "preToolUse", `{"tool_name":"mcp__atlassian__confluence_get_page"}`)
	if page.allow {
		t.Fatal("confluence was allowed")
	}
	mate := askGuard(t, app, doc, "", "subagentStart", `{"subagent_type":"requirement_planner"}`)
	if !mate.allow {
		t.Fatalf("planner = %+v", mate)
	}

	other.Enabled = true
	if err := agentfile.Save(dir, other.ID, other); err != nil {
		t.Fatal(err)
	}
	ambiguous := askGuard(t, app, doc, "", "beforeShellExecution", `{"command":"ls"}`)
	if ambiguous.allow || !strings.Contains(ambiguous.message, "requirement_document") {
		t.Fatalf("ambiguous = %+v", ambiguous)
	}
	app.bindGuard(reader.ID, doc, false)
	started := askGuard(t, app, doc, "", "beforeMCPExecution", `{"server":"mcp-atlassian-kiki","tool_name":"jira_search"}`)
	if !started.allow {
		t.Fatalf("started = %+v", started)
	}
	named := askGuard(t, app, doc, other.ID, "beforeMCPExecution", `{"server":"mcp-atlassian-kiki"}`)
	if named.allow {
		t.Fatal("the other agent was allowed to use Jira")
	}
}

func TestGuardrailBlocksWritesWhenTheRepositoryIsReadonly(t *testing.T) {
	app := newTeamTestServer(t)
	if err := app.Store.EnsureLayout(app.Root); err != nil {
		t.Fatal(err)
	}
	if _, err := EnsureAgentFiles(app.Root); err != nil {
		t.Fatal(err)
	}
	dir, err := agentfile.Resolve(app.Root, "")
	if err != nil {
		t.Fatal(err)
	}
	files, err := agentfile.List(dir)
	if err != nil {
		t.Fatal(err)
	}
	var reader agentfile.File
	for _, file := range files {
		if file.ID == "requirement_document" {
			file.Enabled = true
			file.Repos = []agentfile.Repository{{Kind: "document", Readonly: true}, {Kind: "backend"}}
			file.MCPs = nil
			file.Tools = nil
			file.WorksWith = nil
			reader = file
		} else {
			file.Enabled = false
		}
		if err := agentfile.Save(dir, file.ID, file); err != nil {
			t.Fatalf("save %s: %v", file.ID, err)
		}
	}
	if reader.ID == "" {
		t.Fatal("missing agent")
	}
	snap, err := app.load()
	if err != nil {
		t.Fatal(err)
	}
	doc := snap.Repo("document").Folder
	backend := snap.Repo("backend").Folder
	note, err := json.Marshal(map[string]string{"file_path": filepath.Join(doc, "notes.md")})
	if err != nil {
		t.Fatal(err)
	}
	read := askGuard(t, app, doc, reader.ID, "beforeReadFile", string(note))
	if !read.allow {
		t.Fatalf("read = %+v", read)
	}
	edit := askGuard(t, app, doc, reader.ID, "preToolUse", `{"tool_name":"Write","tool_input":`+string(note)+`}`)
	if edit.allow || !strings.Contains(edit.message, "readonly") {
		t.Fatalf("edit = %+v", edit)
	}
	status := askGuard(t, app, doc, reader.ID, "beforeShellExecution", `{"command":"git status"}`)
	if !status.allow {
		t.Fatalf("status = %+v", status)
	}
	commit := askGuard(t, app, doc, reader.ID, "beforeShellExecution", `{"command":"git commit -am msg"}`)
	if commit.allow || !strings.Contains(commit.message, "readonly") {
		t.Fatalf("commit = %+v", commit)
	}
	code, err := json.Marshal(map[string]string{"path": filepath.Join(backend, "main.go")})
	if err != nil {
		t.Fatal(err)
	}
	change := askGuard(t, app, backend, reader.ID, "preToolUse", `{"tool_name":"Edit","tool_input":`+string(code)+`}`)
	if !change.allow {
		t.Fatalf("backend edit = %+v", change)
	}
}

type guardAnswer struct {
	allow   bool
	message string
}

func askGuard(t *testing.T, app *Server, cwd, agent, event, input string) guardAnswer {
	t.Helper()
	body, err := json.Marshal(map[string]any{
		"cwd":   cwd,
		"agent": agent,
		"event": event,
		"input": json.RawMessage(input),
	})
	if err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPost, "/api/guardrail", strings.NewReader(string(body)))
	rec := httptest.NewRecorder()
	app.handleGuardrail(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	var payload struct {
		Allow   bool   `json:"allow"`
		Message string `json:"message"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	return guardAnswer{allow: payload.Allow, message: payload.Message}
}
