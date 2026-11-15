package store

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/tools"
)

func TestToolsStayOutOfSetupFile(t *testing.T) {
	dir := t.TempDir()
	db, err := Open(filepath.Join(dir, "kiki.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	if err := db.SaveTools(tools.Settings{GitHubCLI: true, RTK: true}); err != nil {
		t.Fatal(err)
	}
	got, err := db.Tools()
	if err != nil {
		t.Fatal(err)
	}
	if !got.GitHubCLI || !got.RTK || got.Empty() {
		t.Fatalf("loaded = %+v", got)
	}
	setup, err := os.ReadFile(db.File())
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(setup), "github_cli") || strings.Contains(string(setup), "rtk") {
		t.Fatalf("setup file contains tool settings:\n%s", setup)
	}
	raw, err := os.ReadFile(filepath.Join(dir, "tools.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	if !strings.Contains(text, "github_cli: true") || !strings.Contains(text, "rtk: true") || !strings.Contains(text, "Agents do not read this file") {
		t.Fatalf("tools file =\n%s", text)
	}
	off := false
	if err := db.SaveTools(tools.Settings{Guardrails: &off}); err != nil {
		t.Fatal(err)
	}
	kept, err := os.ReadFile(filepath.Join(dir, "tools.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(kept), "guardrails: false") {
		t.Fatalf("tools file =\n%s", kept)
	}
	loaded, err := db.Tools()
	if err != nil {
		t.Fatal(err)
	}
	if loaded.GuardrailsOn() {
		t.Fatal("guardrails stayed on")
	}
	if err := db.SaveTools(tools.Settings{}); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, "tools.yaml")); !os.IsNotExist(err) {
		t.Fatal("turning every tool off left the file")
	}
	cleared, err := db.Tools()
	if err != nil {
		t.Fatal(err)
	}
	if !cleared.Empty() {
		t.Fatalf("cleared = %+v", cleared)
	}
}
