package store

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/console"
)

func TestSaveKeepsTeamFolder(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "kiki.yaml")
	initial := "workspace_id: kiki\norca_agent: cursor\ngoogle_drive:\n  refresh_token: refresh-secret-token\n"
	if err := os.WriteFile(path, []byte(initial), 0o644); err != nil {
		t.Fatal(err)
	}
	db, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	err = db.SaveTeamFolder(TeamFolder{
		Path:   "  /tmp/kiki-team  ",
		Member: "  alice  ",
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := db.Save(console.Snapshot{WorkspaceID: "studio", OrcaAgent: "cursor"}); err != nil {
		t.Fatal(err)
	}
	cfg, err := db.TeamFolder()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Path != "/tmp/kiki-team" || cfg.Member != "alice" {
		t.Fatalf("team = %+v", cfg)
	}
	snap, err := db.Load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.WorkspaceID != "studio" {
		t.Fatalf("workspace = %s", snap.WorkspaceID)
	}
	raw, err := os.ReadFile(db.File())
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	if !strings.Contains(text, "path: /tmp/kiki-team") || !strings.Contains(text, "member: alice") || !strings.Contains(text, "workspace_id: studio") {
		t.Fatalf("setup file =\n%s", text)
	}
	if strings.Contains(text, "refresh-secret-token") || strings.Contains(text, "google_drive") {
		t.Fatalf("google drive settings were kept:\n%s", text)
	}
}
