package server

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/agentdef"
	"kiki/internal/console"
	"kiki/internal/mcp"
)

func TestSaveAtlassianHidesTokens(t *testing.T) {
	app := newTeamTestServer(t)
	body := `{
		"workspaceId":"kiki","orcaAgent":"cursor","repos":[],
		"atlassian":{
			"enabled":true,
			"username":"a@acme.co","apiToken":"secret-jira-token",
			"jira":{"enabled":true,"url":"https://acme.atlassian.net"},
			"confluence":{"enabled":true,"url":"https://acme.atlassian.net/wiki"}
		}
	}`
	req := httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(body))
	rec := httptest.NewRecorder()
	app.handleSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `"apiToken":"secret-jira-token"`) {
		t.Fatalf("state hid the token: %s", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `"apiTokenSet":true`) || !strings.Contains(rec.Body.String(), `"url":"https://acme.atlassian.net/wiki"`) {
		t.Fatalf("state = %s", rec.Body.String())
	}
	setup, err := os.ReadFile(app.Store.File())
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(setup), "secret-jira-token") {
		t.Fatalf("setup file contains the token:\n%s", setup)
	}
	again := `{
		"workspaceId":"kiki","orcaAgent":"cursor","repos":[],
		"atlassian":{
			"enabled":true,
			"username":"a@acme.co",
			"jira":{"enabled":true,"url":"https://acme.atlassian.net"},
			"confluence":{"enabled":true,"url":"https://acme.atlassian.net/wiki"}
		}
	}`
	req = httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(again))
	rec = httptest.NewRecorder()
	app.handleSave(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	cfg, err := app.Store.Atlassian()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.APIToken != "secret-jira-token" || cfg.Jira.APIToken != "" || cfg.Confluence.APIToken != "" {
		t.Fatalf("saved = %+v", cfg)
	}
}

func TestSaveAtlassianRejectsPartialSite(t *testing.T) {
	app := newTeamTestServer(t)
	body := `{"workspaceId":"kiki","orcaAgent":"cursor","repos":[],"atlassian":{"enabled":true,"jira":{"enabled":true,"url":"https://acme.atlassian.net"}}}`
	req := httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(body))
	rec := httptest.NewRecorder()
	app.handleSave(rec, req)
	if rec.Code != http.StatusBadRequest || !strings.Contains(rec.Body.String(), "Atlassian needs an email") {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
}

func TestLaunchInstallsAtlassianMCPForTheAgentTool(t *testing.T) {
	app := newTeamTestServer(t)
	home := t.TempDir()
	app.MCPHome = home
	if err := app.Store.SaveAtlassian(mcp.Atlassian{
		Enabled:    true,
		Username:   "a@acme.co",
		APIToken:   "secret-jira-token",
		Jira:       mcp.Service{Enabled: true, URL: "https://acme.atlassian.net"},
		Confluence: mcp.Service{Enabled: true, URL: "https://acme.atlassian.net/wiki"},
	}); err != nil {
		t.Fatal(err)
	}
	_, err := app.launchOrchestrator(context.Background(), launchPrep{
		agent:    agentdef.Agent{MCPs: []agentdef.Use{{ID: "atlassian_jira"}, {ID: "atlassian_confluence"}}},
		snap:     console.Snapshot{Orchestrator: "other", WorkspaceID: "kiki"},
		tool:     "cursor",
		worktree: t.TempDir(),
	})
	if err == nil {
		t.Fatal("expected launch to stop")
	}
	path := filepath.Join(home, ".cursor", "mcp.json")
	raw, readErr := os.ReadFile(path)
	if readErr == nil {
		if !strings.Contains(string(raw), "secret-jira-token") || !strings.Contains(string(raw), `"mcp-atlassian-kiki"`) {
			t.Fatalf("mcp.json =\n%s", raw)
		}
		if !strings.Contains(err.Error(), "Agent Orchestration tool is not supported") {
			t.Fatalf("launch err = %v", err)
		}
		return
	}
	if !strings.Contains(err.Error(), "uvx is not installed") {
		t.Fatalf("launch err = %v", err)
	}
}

func TestLaunchStopsWhenTheSelectedMCPIsNotSetUp(t *testing.T) {
	app := newTeamTestServer(t)
	app.MCPHome = t.TempDir()
	if err := app.Store.SaveAtlassian(mcp.Atlassian{
		Enabled:  true,
		Username: "a@acme.co",
		APIToken: "secret-jira-token",
		Jira:     mcp.Service{Enabled: true, URL: "https://acme.atlassian.net"},
	}); err != nil {
		t.Fatal(err)
	}
	_, err := app.launchOrchestrator(context.Background(), launchPrep{
		agent:    agentdef.Agent{MCPs: []agentdef.Use{{ID: "atlassian_confluence"}}},
		snap:     console.Snapshot{Orchestrator: "other", WorkspaceID: "kiki"},
		tool:     "cursor",
		worktree: t.TempDir(),
	})
	if err == nil || !strings.Contains(err.Error(), "Atlassian - Confluence") {
		t.Fatalf("launch err = %v", err)
	}
}

func TestLaunchSkipsMCPTheAgentDoesNotUse(t *testing.T) {
	app := newTeamTestServer(t)
	home := t.TempDir()
	app.MCPHome = home
	if err := app.Store.SaveAtlassian(mcp.Atlassian{
		Enabled:  true,
		Username: "a@acme.co",
		APIToken: "secret-jira-token",
		Jira:     mcp.Service{Enabled: true, URL: "https://acme.atlassian.net"},
	}); err != nil {
		t.Fatal(err)
	}
	_, err := app.launchOrchestrator(context.Background(), launchPrep{
		snap:     console.Snapshot{Orchestrator: "other", WorkspaceID: "kiki"},
		tool:     "cursor",
		worktree: t.TempDir(),
	})
	if err == nil || !strings.Contains(err.Error(), "Agent Orchestration tool is not supported") {
		t.Fatalf("launch err = %v", err)
	}
	if _, statErr := os.Stat(filepath.Join(home, ".cursor", "mcp.json")); !os.IsNotExist(statErr) {
		t.Fatal("wrote Cursor MCP settings for an agent that does not use Atlassian")
	}
}
