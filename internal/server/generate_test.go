package server

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
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
}
