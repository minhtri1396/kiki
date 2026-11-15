package server

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/tools"
)

func TestSettingsToolsReachTheAgentPrompt(t *testing.T) {
	app := newTeamTestServer(t)
	var hooks []bool
	app.RTKHooks = func(home string, on bool) error {
		if home == "" {
			t.Fatal("home is empty")
		}
		hooks = append(hooks, on)
		return nil
	}
	if _, err := EnsureAgentFiles(app.Root); err != nil {
		t.Fatal(err)
	}
	handler := app.Handler()
	state := serve(t, handler, http.MethodGet, "/api/state", nil)
	if !strings.Contains(state, `"tools":[]`) || !strings.Contains(state, `"id":"github_cli"`) || !strings.Contains(state, `"id":"rtk"`) || !strings.Contains(state, `"global":true`) {
		t.Fatalf("state = %s", state)
	}

	saveTools := func(github, rtk bool) string {
		t.Helper()
		body := `{"workspaceId":"kiki","orchestrator":"orca","repos":[],"tools":{"github_cli":` + boolJSON(github) + `,"rtk":` + boolJSON(rtk) + `}}`
		req := httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(body))
		rec := httptest.NewRecorder()
		app.handleSave(rec, req)
		if rec.Code != http.StatusOK {
			t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
		}
		return rec.Body.String()
	}
	saved := saveTools(true, true)
	if !strings.Contains(saved, `"tools":[{"id":"github_cli","name":"GitHub CLI"}]`) || !strings.Contains(saved, `"id":"rtk"`) || !strings.Contains(saved, `"global":true`) {
		t.Fatalf("enabled tools = %s", saved)
	}
	setup, err := os.ReadFile(app.Store.File())
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(setup), "github_cli") || strings.Contains(string(setup), "\nrtk:") {
		t.Fatalf("setup file contains tool settings:\n%s", setup)
	}

	raw := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	var agent map[string]any
	if err := json.Unmarshal([]byte(raw), &agent); err != nil {
		t.Fatal(err)
	}
	agent["tools"] = []map[string]string{{
		"id":          "github_cli",
		"description": "Open the pull request.",
	}, {
		"id":          "rtk",
		"description": "Shorten the output.",
	}}
	payload, err := json.Marshal(agent)
	if err != nil {
		t.Fatal(err)
	}
	serve(t, handler, http.MethodPut, "/api/agents/requirement_document", payload)
	kept := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	if !strings.Contains(kept, `"id":"github_cli"`) || strings.Contains(kept, `"id":"rtk"`) || strings.Contains(kept, "Open the pull request.") || strings.Contains(kept, "Shorten the output.") {
		t.Fatalf("saved agent = %s", kept)
	}
	preview := serve(t, handler, http.MethodPost, "/api/agents/requirement_document/preview", []byte(`{}`))
	for _, want := range []string{
		"Tools for this agent:",
		"- GitHub CLI",
		"The gh command opens pull requests, issues, and checks. Use gh for that work.",
	} {
		if !strings.Contains(preview, want) {
			t.Fatalf("missing %q in\n%s", want, preview)
		}
	}
	if strings.Contains(preview, "RTK is on") {
		t.Fatalf("the launch prompt told the model to run RTK:\n%s", preview)
	}
	if len(hooks) != 1 || !hooks[0] {
		t.Fatalf("hooks = %v", hooks)
	}
	if strings.Contains(preview, "Open the pull request.") || strings.Contains(preview, "Use this tool:") {
		t.Fatalf("the prompt used an agent-written tool note:\n%s", preview)
	}

	saveTools(true, false)
	if len(hooks) != 2 || hooks[1] {
		t.Fatalf("hooks after turning RTK off = %v", hooks)
	}
	preview = serve(t, handler, http.MethodPost, "/api/agents/requirement_document/preview", []byte(`{}`))
	if strings.Contains(preview, "RTK is on") || !strings.Contains(preview, "- GitHub CLI") {
		t.Fatalf("RTK stayed in the prompt or GitHub CLI dropped:\n%s", preview)
	}

	saveTools(false, false)
	preview = serve(t, handler, http.MethodPost, "/api/agents/requirement_document/preview", []byte(`{}`))
	if strings.Contains(preview, "Tools for this agent:") || strings.Contains(preview, "Use gh for that work.") || strings.Contains(preview, "RTK is on") {
		t.Fatalf("a tool that Settings turned off was included:\n%s", preview)
	}
	kept = serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	if !strings.Contains(kept, `"id":"github_cli"`) {
		t.Fatalf("turning Settings off dropped the tool: %s", kept)
	}
}

func TestStartChecksTheAgentToolHooks(t *testing.T) {
	app := newTeamTestServer(t)
	home := t.TempDir()
	app.MCPHome = home
	app.RTKHooks = func(string, bool) error {
		t.Fatal("rtk init ran when the hook was already set")
		return nil
	}
	if err := app.Store.SaveTools(tools.Settings{RTK: true}); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(home, ".cursor"), 0o755); err != nil {
		t.Fatal(err)
	}
	body := `{"version":1,"hooks":{"preToolUse":[{"command":"rtk hook cursor","matcher":"Shell"}]}}`
	if err := os.WriteFile(filepath.Join(home, ".cursor", "hooks.json"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := app.ensureAgentHooks("cursor"); err != nil {
		t.Fatal(err)
	}

	off := newTeamTestServer(t)
	off.MCPHome = t.TempDir()
	if err := off.ensureAgentHooks("gemini"); err != nil {
		t.Fatal(err)
	}

	missing := newTeamTestServer(t)
	missing.MCPHome = t.TempDir()
	if err := missing.Store.SaveTools(tools.Settings{RTK: true}); err != nil {
		t.Fatal(err)
	}
	err := missing.ensureAgentHooks("claude")
	if err == nil || !strings.Contains(err.Error(), "Claude RTK hook") {
		t.Fatalf("err = %v", err)
	}

	quiet := newTeamTestServer(t)
	quietHome := t.TempDir()
	quiet.MCPHome = quietHome
	offGuard := false
	if err := quiet.Store.SaveTools(tools.Settings{Guardrails: &offGuard}); err != nil {
		t.Fatal(err)
	}
	if err := quiet.ensureAgentHooks("cursor"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(quietHome, ".cursor", "hooks.json")); !os.IsNotExist(err) {
		t.Fatal("a hook was written while guardrails and RTK were off")
	}
}

func TestSaveSyncsGuardrailsWithTheSetting(t *testing.T) {
	app := newTeamTestServer(t)
	var got []bool
	app.GuardHooks = func(home string, on bool) error {
		if home == "" {
			t.Fatal("home is empty")
		}
		got = append(got, on)
		return nil
	}
	body := `{"workspaceId":"kiki","orchestrator":"orca","repos":[],"tools":{"github_cli":false,"rtk":false,"guardrails":false}}`
	req := httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(body))
	rec := httptest.NewRecorder()
	app.handleSave(rec, req)
	if rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), `"guardrails":false`) {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if len(got) != 1 || got[0] {
		t.Fatalf("guardrails = %v", got)
	}
	body = `{"workspaceId":"kiki","orchestrator":"orca","repos":[],"tools":{"github_cli":true,"rtk":false}}`
	req = httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(body))
	rec = httptest.NewRecorder()
	app.handleSave(rec, req)
	if rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), `"guardrails":false`) {
		t.Fatalf("omitted guardrails changed the setting: %d %s", rec.Code, rec.Body.String())
	}
	body = `{"workspaceId":"kiki","orchestrator":"orca","repos":[],"tools":{"github_cli":false,"rtk":false,"guardrails":true}}`
	req = httptest.NewRequest(http.MethodPut, "/api/setup", strings.NewReader(body))
	rec = httptest.NewRecorder()
	app.handleSave(rec, req)
	if rec.Code != http.StatusOK || !strings.Contains(rec.Body.String(), `"guardrails":true`) {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if len(got) != 3 || !got[2] {
		t.Fatalf("guardrails = %v", got)
	}
}

func boolJSON(on bool) string {
	if on {
		return "true"
	}
	return "false"
}
