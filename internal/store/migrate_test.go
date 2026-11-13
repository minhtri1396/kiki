package store

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestMigrateMovesAppFilesAndLeavesRepos(t *testing.T) {
	root := t.TempDir()
	oldAgents := filepath.Join(root, "agents")
	if err := os.MkdirAll(filepath.Join(oldAgents, "requirement"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(oldAgents, "requirement", "requirement_document.md"), []byte("agent"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "backend"), 0o755); err != nil {
		t.Fatal(err)
	}
	setup := "workspace_id: studio\norca_agent: cursor\nagent_path: " + oldAgents + "\n"
	if err := os.WriteFile(filepath.Join(root, "kiki.yaml"), []byte(setup), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "models.yaml"), []byte("agents: []\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "kiki.db"), []byte("db"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Migrate(root); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(root, "kiki.yaml")); !os.IsNotExist(err) {
		t.Fatal("setup file stayed beside the app")
	}
	if _, err := os.Stat(filepath.Join(root, "backend")); err != nil {
		t.Fatal(err)
	}
	moved := filepath.Join(root, ".kiki", "agents", "requirement", "requirement_document.md")
	if _, err := os.Stat(moved); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(filepath.Join(root, ".kiki", "kiki.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	if strings.Contains(text, "agent_path:") || !strings.Contains(text, "workspace_id: studio") {
		t.Fatalf("setup =\n%s", text)
	}
	if _, err := os.Stat(filepath.Join(root, "models.yaml")); !os.IsNotExist(err) {
		t.Fatal("old models.yaml was kept")
	}
	if _, err := os.Stat(filepath.Join(root, ".kiki", "models.yaml")); !os.IsNotExist(err) {
		t.Fatal("models.yaml was copied into .kiki")
	}
	if _, err := os.Stat(filepath.Join(root, ".kiki", "kiki.db")); err != nil {
		t.Fatal(err)
	}
	if err := Migrate(root); err != nil {
		t.Fatal(err)
	}
}

func TestMigrateMovesCustomAgentsIntoKiki(t *testing.T) {
	root := t.TempDir()
	custom := filepath.Join(root, "shared-agents", "requirement")
	if err := os.MkdirAll(custom, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(custom, "requirement_document.md"), []byte("shared"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "agents"), 0o755); err != nil {
		t.Fatal(err)
	}
	setup := "workspace_id: studio\norca_agent: cursor\nagent_path: " + filepath.Dir(custom) + "\n"
	if err := os.WriteFile(filepath.Join(root, "kiki.yaml"), []byte(setup), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Migrate(root); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(root, "agents")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Dir(custom)); !os.IsNotExist(err) {
		t.Fatal("custom agent folder was kept")
	}
	got, err := os.ReadFile(filepath.Join(root, ".kiki", "agents", "requirement", "requirement_document.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != "shared" {
		t.Fatalf("agent file = %q", got)
	}
	raw, err := os.ReadFile(Path(root))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), "agent_path:") {
		t.Fatalf("setup =\n%s", raw)
	}
}
