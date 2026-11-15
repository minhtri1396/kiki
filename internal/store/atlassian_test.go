package store

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/mcp"
)

func TestAtlassianStaysOutOfSetupFile(t *testing.T) {
	dir := t.TempDir()
	db, err := Open(filepath.Join(dir, "kiki.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	cfg := mcp.Atlassian{
		Enabled:  true,
		Username: "a@acme.co",
		APIToken: "secret-jira-token",
		Jira:     mcp.Service{Enabled: true, URL: "https://acme.atlassian.net"},
		Confluence: mcp.Service{
			Enabled: true, URL: "https://acme.atlassian.net/wiki", Own: true, Username: "wiki@acme.co", APIToken: "secret-wiki-token",
		},
	}
	if err := db.SaveAtlassian(cfg); err != nil {
		t.Fatal(err)
	}
	got, err := db.Atlassian()
	if err != nil {
		t.Fatal(err)
	}
	if got.APIToken != "secret-jira-token" || got.Confluence.APIToken != "secret-wiki-token" || !got.Confluence.Own || !got.Enabled {
		t.Fatalf("loaded = %+v", got)
	}
	info, err := os.Stat(filepath.Join(dir, "mcp.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("mode = %o", info.Mode().Perm())
	}
	if _, err := os.Stat(filepath.Join(dir, "atlassian.yaml")); !os.IsNotExist(err) {
		t.Fatal("credentials were written to atlassian.yaml")
	}
	setup, err := os.ReadFile(db.File())
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(setup), "secret-jira-token") || strings.Contains(string(setup), "atlassian") {
		t.Fatalf("setup file contains Atlassian settings:\n%s", setup)
	}
	secrets, err := os.ReadFile(filepath.Join(dir, "mcp.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	text := string(secrets)
	if !strings.Contains(text, "secret-jira-token") || !strings.Contains(text, "Agents do not read this file") || !strings.Contains(text, "atlassian:") {
		t.Fatalf("mcp file =\n%s", text)
	}
	if err := db.SaveAtlassian(mcp.Atlassian{}); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, "mcp.yaml")); !os.IsNotExist(err) {
		t.Fatal("empty setup left the credentials file")
	}
}

func TestAtlassianFileMovesIntoMCPFile(t *testing.T) {
	dir := t.TempDir()
	db, err := Open(filepath.Join(dir, "kiki.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	legacy := filepath.Join(dir, "atlassian.yaml")
	body := "enabled: true\nusername: a@acme.co\napi_token: secret-jira-token\njira:\n  enabled: true\n  url: https://acme.atlassian.net\n"
	if err := os.WriteFile(legacy, []byte(body), 0o600); err != nil {
		t.Fatal(err)
	}
	got, err := db.Atlassian()
	if err != nil {
		t.Fatal(err)
	}
	if got.APIToken != "secret-jira-token" || !got.Jira.Enabled {
		t.Fatalf("loaded = %+v", got)
	}
	if _, err := os.Stat(legacy); !os.IsNotExist(err) {
		t.Fatal("legacy atlassian.yaml was kept")
	}
	raw, err := os.ReadFile(filepath.Join(dir, "mcp.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(raw), "atlassian:") || !strings.Contains(string(raw), "secret-jira-token") {
		t.Fatalf("mcp file =\n%s", raw)
	}
}
