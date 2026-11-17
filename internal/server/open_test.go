package server

import (
	"net/http"
	"net/http/httptest"
	"runtime"
	"strings"
	"testing"

	"kiki/internal/mcp"
	"kiki/internal/store"
)

func TestPageCommandOpensOnlyIndexPages(t *testing.T) {
	cmd, err := pageCommand("https://obsidian.md/help/cli")
	if err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS == "darwin" && (cmd.Path == "" || len(cmd.Args) != 2 || cmd.Args[1] != "https://obsidian.md/help/cli") {
		t.Fatalf("args = %#v", cmd.Args)
	}
	if cmd.Args[len(cmd.Args)-1] != "https://obsidian.md/help/cli" {
		t.Fatalf("args = %#v", cmd.Args)
	}
	if _, err := pageCommand("https://id.atlassian.com/manage-profile/security/api-tokens"); err != nil {
		t.Fatal(err)
	}
	if err := openIndexPage("https://example.com"); err == nil {
		t.Fatal("expected a rejected page")
	}
	if err := openIndexPage("https://obsidian.md/help/cli/../download"); err == nil {
		t.Fatal("expected a rejected page")
	}
}

func TestJiraBrowsePage(t *testing.T) {
	page, err := jiraBrowsePage("https://acme.atlassian.net/", "kiki-12")
	if err != nil {
		t.Fatal(err)
	}
	if page != "https://acme.atlassian.net/browse/KIKI-12" {
		t.Fatalf("page = %s", page)
	}
	page, err = jiraBrowsePage("https://jira.example.com/jira", "KIKI-12")
	if err != nil {
		t.Fatal(err)
	}
	if page != "https://jira.example.com/jira/browse/KIKI-12" {
		t.Fatalf("page = %s", page)
	}
	if _, err := jiraBrowsePage("", "KIKI-12"); err == nil {
		t.Fatal("expected a missing site")
	}
	if _, err := jiraBrowsePage("https://acme.atlassian.net", "not a key"); err == nil {
		t.Fatal("expected a rejected key")
	}
	if _, err := jiraBrowsePage("https://user:pass@acme.atlassian.net", "KIKI-12"); err == nil {
		t.Fatal("expected a rejected site")
	}
}

func TestOpenJiraUsesTheSystemBrowser(t *testing.T) {
	app := newTaskTestServer(t)
	if err := app.Store.SaveAtlassian(mcp.Atlassian{
		Jira: mcp.Service{URL: "https://acme.atlassian.net"},
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := app.Store.CreateGroup(store.GroupInput{Name: "Billing", EpicKey: "KIKI-12", EpicBoardID: "1", EpicBoardName: "KIKI"}); err != nil {
		t.Fatal(err)
	}
	opened := ""
	prev := startBrowser
	startBrowser = func(page string) error {
		opened = page
		return nil
	}
	t.Cleanup(func() { startBrowser = prev })

	req := httptest.NewRequest(http.MethodPost, "/api/jira/open", strings.NewReader(`{"key":"kiki-12"}`))
	rec := httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
	if opened != "https://acme.atlassian.net/browse/KIKI-12" {
		t.Fatalf("opened = %s", opened)
	}

	req = httptest.NewRequest(http.MethodPost, "/api/jira/open", strings.NewReader(`{"key":"KIKI-99"}`))
	rec = httptest.NewRecorder()
	app.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d body = %s", rec.Code, rec.Body.String())
	}
}
