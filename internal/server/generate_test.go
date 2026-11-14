package server

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"kiki/internal/agentdef"
	"kiki/internal/console"
)

func TestGenerateRejectsEmptyText(t *testing.T) {
	app := newTeamTestServer(t)
	req := httptest.NewRequest(http.MethodPost, "/api/generate", strings.NewReader(`{"kind":"prompt","text":"  "}`))
	rec := httptest.NewRecorder()
	app.handleGenerate(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "Enter some text first.") {
		t.Fatalf("body = %s", rec.Body.String())
	}
}

func TestSaveKeepsGenerateChoice(t *testing.T) {
	app := newTeamTestServer(t)
	body := `{"workspaceId":"kiki","orcaAgent":"cursor","generateTool":"claude","generateModel":"sonnet","repos":[]}`
	req := httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(body))
	rec := httptest.NewRecorder()
	app.handleSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `"generateTool":"claude"`) || !strings.Contains(rec.Body.String(), `"generateModel":"sonnet"`) {
		t.Fatalf("state = %s", rec.Body.String())
	}
	snap, err := app.Store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.GenerateTool != "claude" || snap.GenerateModel != "sonnet" {
		t.Fatalf("saved = %s %s", snap.GenerateTool, snap.GenerateModel)
	}
	if snap.Orchestrator != "orca" || !strings.Contains(rec.Body.String(), `"orchestrator":"orca"`) {
		t.Fatalf("orchestrator = %s state = %s", snap.Orchestrator, rec.Body.String())
	}
}

func TestSaveRejectsUnknownOrchestrator(t *testing.T) {
	app := newTeamTestServer(t)
	body := `{"workspaceId":"kiki","orcaAgent":"cursor","orchestrator":"other","repos":[]}`
	req := httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(body))
	rec := httptest.NewRecorder()
	app.handleSave(rec, req)
	if rec.Code != http.StatusBadRequest || !strings.Contains(rec.Body.String(), "Agent Orchestration tool is not supported") {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
}

func TestOrchestratorLaunchFollowsTheSavedTool(t *testing.T) {
	app := newTeamTestServer(t)
	_, err := app.launchOrchestrator(context.Background(), launchPrep{
		snap: console.Snapshot{Orchestrator: "other"},
	})
	if err == nil || !strings.Contains(err.Error(), "Agent Orchestration tool is not supported") {
		t.Fatalf("launch err = %v", err)
	}
	command, project, err := app.previewOrchestrator(launchPrep{
		snap:     console.Snapshot{WorkspaceID: "kiki", Orchestrator: "orca"},
		kind:     "document",
		tool:     "cursor",
		worktree: t.TempDir(),
		agent:    agentdef.Agent{ID: "requirement_document"},
	})
	if err != nil {
		t.Fatal(err)
	}
	if project != "kiki-document" || !strings.Contains(command, "\norca ") {
		t.Fatalf("command = %q project = %q", command, project)
	}
}
