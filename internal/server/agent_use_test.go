package server

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"kiki/internal/mcp"
)

func TestAgentMCPAndToolNotesReachThePrompt(t *testing.T) {
	app := newTeamTestServer(t)
	if _, err := EnsureAgentFiles(app.Root); err != nil {
		t.Fatal(err)
	}
	handler := app.Handler()
	state := serve(t, handler, http.MethodGet, "/api/state", nil)
	if catalog := stateList(t, state, "mcps"); catalog != "[]" || !strings.Contains(state, `"tools":[]`) {
		t.Fatalf("state = %s", state)
	}
	planner := agentInState(t, state, "requirement_planner")
	if !stateHasUse(planner["mcps"], "atlassian_jira") || !stateHasUse(planner["mcps"], "atlassian_confluence") {
		t.Fatalf("requirement_planner mcps = %#v", planner["mcps"])
	}
	if err := app.Store.SaveAtlassian(mcp.Atlassian{
		Enabled:  true,
		Username: "a@acme.co",
		APIToken: "secret-jira-token",
		Jira:     mcp.Service{Enabled: true, URL: "https://acme.atlassian.net"},
	}); err != nil {
		t.Fatal(err)
	}
	state = serve(t, handler, http.MethodGet, "/api/state", nil)
	mcps := stateList(t, state, "mcps")
	if !strings.Contains(mcps, `"id":"atlassian_jira","name":"Atlassian - Jira"`) || strings.Contains(mcps, "atlassian_confluence") {
		t.Fatalf("jira-only mcps = %s", mcps)
	}
	if err := app.Store.SaveAtlassian(mcp.Atlassian{
		Enabled:    true,
		Username:   "a@acme.co",
		APIToken:   "secret-jira-token",
		Jira:       mcp.Service{Enabled: true, URL: "https://acme.atlassian.net"},
		Confluence: mcp.Service{Enabled: true, URL: "https://acme.atlassian.net/wiki"},
	}); err != nil {
		t.Fatal(err)
	}
	state = serve(t, handler, http.MethodGet, "/api/state", nil)
	if !strings.Contains(state, `"id":"atlassian_jira","name":"Atlassian - Jira"`) || !strings.Contains(state, `"id":"atlassian_confluence","name":"Atlassian - Confluence"`) || !strings.Contains(state, `"tools":[]`) {
		t.Fatalf("configured mcps = %s", state)
	}

	raw := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	var agent map[string]any
	if err := json.Unmarshal([]byte(raw), &agent); err != nil {
		t.Fatal(err)
	}
	agent["mcps"] = []map[string]string{{
		"id":          "atlassian",
		"description": "Use the saved Atlassian setup.",
	}}
	legacy, err := json.Marshal(agent)
	if err != nil {
		t.Fatal(err)
	}
	serve(t, handler, http.MethodPut, "/api/agents/requirement_document", legacy)
	legacyPreview := serve(t, handler, http.MethodPost, "/api/agents/requirement_document/preview", []byte(`{}`))
	for _, want := range []string{"- Atlassian - Jira", "- Atlassian - Confluence", "Use the saved Atlassian setup."} {
		if !strings.Contains(legacyPreview, want) {
			t.Fatalf("missing %q in\n%s", want, legacyPreview)
		}
	}

	agent["mcps"] = []map[string]string{{
		"id":          "atlassian_jira",
		"description": "Read the Jira ticket.\nWrite the comment.",
	}, {
		"id":          "atlassian_confluence",
		"description": "Read the design page.",
	}}
	agent["tools"] = []map[string]string{{
		"id":          "github",
		"description": "Open the pull request.",
	}}
	body, err := json.Marshal(agent)
	if err != nil {
		t.Fatal(err)
	}
	serve(t, handler, http.MethodPut, "/api/agents/requirement_document", body)
	saved := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	if !strings.Contains(saved, "Read the Jira ticket.") || !strings.Contains(saved, "Read the design page.") || !strings.Contains(saved, `"id":"github"`) || strings.Contains(saved, "Open the pull request.") {
		t.Fatalf("saved agent = %s", saved)
	}

	preview := serve(t, handler, http.MethodPost, "/api/agents/requirement_document/preview", []byte(`{}`))
	for _, want := range []string{
		"MCP servers for this agent:",
		"- Atlassian - Jira",
		"- Atlassian - Confluence",
		"Use this MCP server:",
		"Read the Jira ticket.",
		"Write the comment.",
		"Read the design page.",
	} {
		if !strings.Contains(preview, want) {
			t.Fatalf("missing %q in\n%s", want, preview)
		}
	}
	if strings.Contains(preview, "Tools for this agent:") || strings.Contains(preview, "Open the pull request.") {
		t.Fatalf("a tool that Settings has not enabled was included:\n%s", preview)
	}

	if err := app.Store.SaveAtlassian(mcp.Atlassian{}); err != nil {
		t.Fatal(err)
	}
	preview = serve(t, handler, http.MethodPost, "/api/agents/requirement_document/preview", []byte(`{}`))
	if strings.Contains(preview, "MCP servers for this agent:") || strings.Contains(preview, "Read the Jira ticket.") {
		t.Fatalf("an MCP that Settings has not set up was included:\n%s", preview)
	}
	kept := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	if !strings.Contains(kept, "Read the Jira ticket.") || !strings.Contains(kept, "Read the design page.") || !strings.Contains(kept, `"id":"github"`) {
		t.Fatalf("clearing Settings dropped the agent notes: %s", kept)
	}
}

func stateList(t *testing.T, raw, key string) string {
	t.Helper()
	var doc map[string]json.RawMessage
	if err := json.Unmarshal([]byte(raw), &doc); err != nil {
		t.Fatal(err)
	}
	return string(doc[key])
}

func agentInState(t *testing.T, raw, id string) map[string]any {
	t.Helper()
	var doc struct {
		Agents []map[string]any `json:"agents"`
	}
	if err := json.Unmarshal([]byte(raw), &doc); err != nil {
		t.Fatal(err)
	}
	for _, agent := range doc.Agents {
		if agent["id"] == id {
			return agent
		}
	}
	t.Fatalf("agent %s missing from state", id)
	return nil
}

func stateHasUse(raw any, id string) bool {
	items, _ := raw.([]any)
	for _, item := range items {
		row, _ := item.(map[string]any)
		if row["id"] == id {
			return true
		}
	}
	return false
}

func serve(t *testing.T, handler http.Handler, method, path string, body []byte) string {
	t.Helper()
	req := httptest.NewRequest(method, path, bytes.NewReader(body))
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("%s %s = %d %s", method, path, rec.Code, rec.Body.String())
	}
	return rec.Body.String()
}

func TestAgentHandoffsReachThePrompt(t *testing.T) {
	app := newTeamTestServer(t)
	if _, err := EnsureAgentFiles(app.Root); err != nil {
		t.Fatal(err)
	}
	handler := app.Handler()
	raw := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	var agent map[string]any
	if err := json.Unmarshal([]byte(raw), &agent); err != nil {
		t.Fatal(err)
	}
	agent["worksWith"] = []map[string]any{{
		"id": "requirement_designer",
		"handoffs": []map[string]any{
			{"when": "The requirement files are ready.", "send": "Design the folder.", "reply": "The design path.", "maxLoops": 2},
			{"when": "The user changes the scope.", "send": "Update the design."},
		},
	}}
	body, err := json.Marshal(agent)
	if err != nil {
		t.Fatal(err)
	}
	serve(t, handler, http.MethodPut, "/api/agents/requirement_document", body)
	saved := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	if !strings.Contains(saved, `"send":"Design the folder.","when":"The requirement files are ready."`) || !strings.Contains(saved, `"send":"Update the design.","when":"The user changes the scope."`) {
		t.Fatalf("saved handoffs are missing from the agent")
	}
	preview := serve(t, handler, http.MethodPost, "/api/agents/requirement_document/preview", []byte(`{}`))
	for _, want := range []string{
		"- agent id: requirement_designer",
		"Message 1 to requirement_designer:",
		"The requirement files are ready.",
		"Design the folder.",
		"Message 2 to requirement_designer:",
		"The user changes the scope.",
		"Update the design.",
	} {
		if !strings.Contains(preview, want) {
			t.Fatalf("missing %q in\n%s", want, preview)
		}
	}
	if !strings.Contains(preview, "ask requirement_designer to report back to you, agent id requirement_document") || !strings.Contains(preview, "Report back: no.") {
		t.Fatalf("report back choice is missing:\n%s", preview)
	}
	if !strings.Contains(saved, `"maxLoops":2`) || !strings.Contains(saved, `"reply":"The design path."`) || !strings.Contains(preview, "Allow at most 2 loops.") || !strings.Contains(preview, "The design path.") {
		t.Fatalf("reply and loop limit are missing:\n%s", preview)
	}
	if strings.Contains(preview, "Message 3 to") {
		t.Fatalf("an empty message was written:\n%s", preview)
	}
}

func TestAgentRepositoryGuideReachesThePrompt(t *testing.T) {
	app := newTeamTestServer(t)
	if _, err := EnsureAgentFiles(app.Root); err != nil {
		t.Fatal(err)
	}
	handler := app.Handler()
	raw := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	var agent map[string]any
	if err := json.Unmarshal([]byte(raw), &agent); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(raw, `"guide":"AGENTS.md"`) {
		t.Fatalf("a repository without a guide did not report the default: %s", raw)
	}
	agent["repos"] = []map[string]any{{"kind": "document", "guide": "RULES.md", "description": "Write the requirements.", "readonly": false}}
	body, err := json.Marshal(agent)
	if err != nil {
		t.Fatal(err)
	}
	serve(t, handler, http.MethodPut, "/api/agents/requirement_document", body)
	saved := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	if !strings.Contains(saved, `"guide":"RULES.md"`) {
		t.Fatal("saved guide file is missing from the agent")
	}
	preview := serve(t, handler, http.MethodPost, "/api/agents/requirement_document/preview", []byte(`{}`))
	if !strings.Contains(preview, "RULES.md (read it and follow it") || !strings.Contains(preview, "Write the requirements.") {
		t.Fatalf("preview does not name the guide file:\n%s", preview)
	}
}

func TestLaunchWaitForReachesThePrompt(t *testing.T) {
	app := newTeamTestServer(t)
	if _, err := EnsureAgentFiles(app.Root); err != nil {
		t.Fatal(err)
	}
	handler := app.Handler()
	plain := serve(t, handler, http.MethodPost, "/api/agents/requirement_designer/preview", []byte(`{"note":"Add a cart."}`))
	if strings.Contains(plain, "WAIT FOR A REQUEST.") {
		t.Fatalf("an agent turned on by the user was told to wait:\n%s", plain)
	}
	waiting := serve(t, handler, http.MethodPost, "/api/agents/requirement_designer/preview", []byte(`{"note":"Add a cart.","waitFor":["requirement_document","no_such_agent","requirement_designer"]}`))
	for _, want := range []string{"WAIT FOR A REQUEST.", "requirement_document (Document Writer)"} {
		if !strings.Contains(waiting, want) {
			t.Fatalf("missing %q in\n%s", want, waiting)
		}
	}
	if strings.Contains(waiting, "no_such_agent") {
		t.Fatalf("an unknown agent was named:\n%s", waiting)
	}
}

func TestAgentHandoffsRequireWhenAndSend(t *testing.T) {
	app := newTeamTestServer(t)
	if _, err := EnsureAgentFiles(app.Root); err != nil {
		t.Fatal(err)
	}
	handler := app.Handler()
	raw := serve(t, handler, http.MethodGet, "/api/agents/requirement_document", nil)
	var agent map[string]any
	if err := json.Unmarshal([]byte(raw), &agent); err != nil {
		t.Fatal(err)
	}
	for name, pair := range map[string]map[string]any{
		"Send when":    {"when": "", "send": "Design it."},
		"What to send": {"when": "The files are ready.", "send": "  "},
	} {
		agent["worksWith"] = []map[string]any{{"id": "requirement_designer", "handoffs": []map[string]any{pair}}}
		body, err := json.Marshal(agent)
		if err != nil {
			t.Fatal(err)
		}
		req := httptest.NewRequest(http.MethodPut, "/api/agents/requirement_document", bytes.NewReader(body))
		rec := httptest.NewRecorder()
		handler.ServeHTTP(rec, req)
		if rec.Code != http.StatusBadRequest || !strings.Contains(rec.Body.String(), name) {
			t.Fatalf("%s: got %d %s", name, rec.Code, rec.Body.String())
		}
	}
}
