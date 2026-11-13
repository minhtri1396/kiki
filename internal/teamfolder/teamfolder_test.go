package teamfolder

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestMembersSkipsHiddenAndSymlinks(t *testing.T) {
	root := t.TempDir()
	team := filepath.Join(root, "team")
	if err := os.MkdirAll(filepath.Join(team, "alice"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(team, ".hidden"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(team, "notes.txt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	outside := filepath.Join(root, "outside")
	if err := os.MkdirAll(outside, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(team, "linked")); err != nil {
		t.Fatal(err)
	}
	names, err := Members(team)
	if err != nil {
		t.Fatal(err)
	}
	if len(names) != 1 || names[0] != "alice" {
		t.Fatalf("members = %#v", names)
	}
	if _, err := Member(team, "linked"); err == nil {
		t.Fatal("symlink member was accepted")
	}
	if _, err := Member(team, "../outside"); err == nil {
		t.Fatal("path escape was accepted")
	}
}

func TestCopyReplacesAgentsAndKeepsThemOnFailure(t *testing.T) {
	root := t.TempDir()
	member := filepath.Join(root, "team", "alice", "requirement")
	agents := filepath.Join(root, "agents", "backend")
	if err := os.MkdirAll(member, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(agents, 0o755); err != nil {
		t.Fatal(err)
	}
	const shared = "shared agent\n"
	if err := os.WriteFile(filepath.Join(member, "requirement_document.md"), []byte(shared), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "agents", "notes.txt"), []byte("notes"), 0o644); err != nil {
		t.Fatal(err)
	}
	agentDir := filepath.Join(root, "agents")
	if err := Copy(filepath.Dir(member), agentDir); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(filepath.Join(agentDir, "requirement", "requirement_document.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != shared {
		t.Fatalf("copied = %q", got)
	}
	if _, err := os.Stat(filepath.Join(agentDir, "notes.txt")); !os.IsNotExist(err) {
		t.Fatal("old local file was kept")
	}
	if _, err := os.Stat(filepath.Join(agentDir, "backend")); !os.IsNotExist(err) {
		t.Fatal("old local folder was kept")
	}

	big := filepath.Join(root, "team", "bob")
	if err := os.MkdirAll(big, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(big, "big.md"), make([]byte, maxFile+1), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Copy(big, agentDir); err == nil {
		t.Fatal("oversized member folder was copied")
	}
	if _, err := os.Stat(filepath.Join(agentDir, "requirement", "requirement_document.md")); err != nil {
		t.Fatal(err)
	}
}

func TestCopyRejectsOverlap(t *testing.T) {
	root := t.TempDir()
	team := filepath.Join(root, "team")
	member := filepath.Join(team, "alice")
	if err := os.MkdirAll(member, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(member, "a.md"), []byte("a"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Copy(member, team); err == nil || !strings.Contains(err.Error(), "local agents folder") {
		t.Fatalf("overlap err = %v", err)
	}
	if _, err := os.Stat(filepath.Join(member, "a.md")); err != nil {
		t.Fatal(err)
	}
}
