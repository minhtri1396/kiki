package gitignore

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestEnsureLeavesNonRepoUntouched(t *testing.T) {
	dir := t.TempDir()
	if _, ok, err := findWorkTree(dir); err != nil {
		t.Fatal(err)
	} else if ok {
		t.Skip("temp dir is inside a git work tree")
	}
	path, err := Ensure(dir, []Entry{{Path: filepath.Join(dir, "kiki.yaml")}})
	if err != nil {
		t.Fatal(err)
	}
	if path != "" {
		t.Fatalf("path = %q", path)
	}
	if _, err := os.Stat(filepath.Join(dir, ".gitignore")); !os.IsNotExist(err) {
		t.Fatal("created .gitignore outside a work tree")
	}
}

func TestEnsureAddsOnlyMissingAppFiles(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	ignore := filepath.Join(root, ".gitignore")
	existing := "# keep\nkiki.yaml\n/backend/\n"
	if err := os.WriteFile(ignore, []byte(existing), 0o644); err != nil {
		t.Fatal(err)
	}
	entries := AppEntries(
		root,
		filepath.Join(root, "agents"),
		filepath.Join(root, "kiki.yaml"),
		filepath.Join(root, "kiki.db"),
	)
	path, err := Ensure(root, entries)
	if err != nil {
		t.Fatal(err)
	}
	if path != ignore {
		t.Fatalf("path = %q", path)
	}
	got, err := os.ReadFile(ignore)
	if err != nil {
		t.Fatal(err)
	}
	want := existing + "\n" + headerComment + `
/kiki.yaml.tmp
/kiki.db
/kiki.db-shm
/kiki.db-wal
/kiki.db-journal
/website/
/mobile/
/docs/
/agents/
`
	if string(got) != want {
		t.Fatalf("gitignore:\n%s\nwant:\n%s", got, want)
	}
	again, err := Ensure(root, entries)
	if err != nil {
		t.Fatal(err)
	}
	if again != "" {
		t.Fatal("second ensure rewrote .gitignore")
	}
	got, err = os.ReadFile(ignore)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != want {
		t.Fatal("second ensure changed the file")
	}
}

func TestEnsureUsesGitRootForNestedApp(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	app := filepath.Join(root, "apps", "kiki")
	if err := os.MkdirAll(app, 0o755); err != nil {
		t.Fatal(err)
	}
	ignore := filepath.Join(root, ".gitignore")
	if err := os.WriteFile(ignore, []byte("kiki.db\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	path, err := Ensure(app, []Entry{
		{Path: filepath.Join(app, "kiki.db")},
		{Path: filepath.Join(app, "docs"), Dir: true},
	})
	if err != nil {
		t.Fatal(err)
	}
	if path != ignore {
		t.Fatalf("path = %q", path)
	}
	got, err := os.ReadFile(ignore)
	if err != nil {
		t.Fatal(err)
	}
	want := "kiki.db\n\n" + headerComment + "\n/apps/kiki/docs/\n"
	if string(got) != want {
		t.Fatalf("gitignore:\n%s\nwant:\n%s", got, want)
	}
	if _, err := os.Stat(filepath.Join(app, ".gitignore")); !os.IsNotExist(err) {
		t.Fatal("wrote .gitignore next to the app instead of the work tree root")
	}
}

func TestEnsureSkipsPathsOutsideWorkTree(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, ".git"), []byte("gitdir: /nowhere\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	outside := filepath.Join(t.TempDir(), "agents")
	path, err := Ensure(root, []Entry{{Path: outside, Dir: true}})
	if err != nil {
		t.Fatal(err)
	}
	if path != "" {
		t.Fatalf("path = %q", path)
	}
	if _, err := os.Stat(filepath.Join(root, ".gitignore")); !os.IsNotExist(err) {
		t.Fatal("created .gitignore for a path outside the work tree")
	}
}

func TestEnsureReplacesUnignoredPath(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	ignore := filepath.Join(root, ".gitignore")
	if err := os.WriteFile(ignore, []byte("kiki.yaml\n!kiki.yaml\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Ensure(root, []Entry{{Path: filepath.Join(root, "kiki.yaml")}}); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(ignore)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(string(got), "\n/kiki.yaml\n") {
		t.Fatalf("gitignore:\n%s", got)
	}
}

func TestEnsureDoesNotRepeatAPathInsideAnIgnoredDirectory(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	if _, err := Ensure(root, []Entry{
		{Path: filepath.Join(root, "docs"), Dir: true},
		{Path: filepath.Join(root, "docs", "vault"), Dir: true},
	}); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(filepath.Join(root, ".gitignore"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(got), "vault") {
		t.Fatalf("nested path was listed separately:\n%s", got)
	}
}

func TestEnsureKeepsCRLF(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	ignore := filepath.Join(root, ".gitignore")
	if err := os.WriteFile(ignore, []byte("keep\r\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Ensure(root, []Entry{{Path: filepath.Join(root, "agents"), Dir: true}}); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(ignore)
	if err != nil {
		t.Fatal(err)
	}
	want := "keep\r\n\r\n# KIKI generated files\r\n/agents/\r\n"
	if string(got) != want {
		t.Fatalf("gitignore:\n%q\nwant:\n%q", got, want)
	}
}

func TestIndexIgnoresCodegraphAndObsidian(t *testing.T) {
	root := t.TempDir()
	cmd := exec.Command("git", "init", "-b", "main")
	cmd.Dir = root
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git init: %v\n%s", err, out)
	}
	if _, err := Index(root, ".codegraph"); err != nil {
		t.Fatal(err)
	}
	if _, err := Index(root, ".obsidian"); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(filepath.Join(root, ".gitignore"))
	if err != nil {
		t.Fatal(err)
	}
	want := indexHeader + "\n/.codegraph/\n/.obsidian/\n"
	if string(got) != want {
		t.Fatalf("gitignore:\n%s\nwant:\n%s", got, want)
	}
	if err := os.MkdirAll(filepath.Join(root, ".codegraph"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, ".codegraph", "db"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, ".obsidian"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, ".obsidian", "app.json"), []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "README.md"), []byte("keep\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := exec.Command("git", "-C", root, "check-ignore", "-q", ".codegraph/db").Run(); err != nil {
		t.Fatal(".codegraph/db should be ignored")
	}
	if err := exec.Command("git", "-C", root, "check-ignore", "-q", ".obsidian/app.json").Run(); err != nil {
		t.Fatal(".obsidian/app.json should be ignored")
	}
	if err := exec.Command("git", "-C", root, "check-ignore", "-q", "README.md").Run(); err == nil {
		t.Fatal("README.md should stay available to commit")
	}
	again, err := Index(root, ".codegraph")
	if err != nil {
		t.Fatal(err)
	}
	if again != "" {
		t.Fatal("second index setup rewrote .gitignore")
	}
}

func TestIndexUsesTheRepositoryGitignore(t *testing.T) {
	parent := t.TempDir()
	if err := os.Mkdir(filepath.Join(parent, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	repo := filepath.Join(parent, "backend")
	if err := os.MkdirAll(repo, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(filepath.Join(repo, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	path, err := Index(repo, ".codegraph")
	if err != nil {
		t.Fatal(err)
	}
	if path != filepath.Join(repo, ".gitignore") {
		t.Fatalf("path = %q", path)
	}
	if _, err := os.Stat(filepath.Join(parent, ".gitignore")); !os.IsNotExist(err) {
		t.Fatal("parent .gitignore was written")
	}
}

func TestIndexUsesParentWorkTree(t *testing.T) {
	parent := t.TempDir()
	if err := os.Mkdir(filepath.Join(parent, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	repo := filepath.Join(parent, "docs")
	if err := os.MkdirAll(repo, 0o755); err != nil {
		t.Fatal(err)
	}
	if _, err := Index(repo, ".obsidian"); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(filepath.Join(parent, ".gitignore"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != indexHeader+"\n/docs/.obsidian/\n" {
		t.Fatalf("gitignore:\n%s", got)
	}
}

func TestIndexLeavesExistingRule(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	ignore := filepath.Join(root, ".gitignore")
	existing := ".codegraph/\n"
	if err := os.WriteFile(ignore, []byte(existing), 0o644); err != nil {
		t.Fatal(err)
	}
	path, err := Index(root, ".codegraph")
	if err != nil {
		t.Fatal(err)
	}
	if path != "" {
		t.Fatal("existing rule was treated as missing")
	}
	got, err := os.ReadFile(ignore)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != existing {
		t.Fatalf("gitignore:\n%s", got)
	}
}

func TestIndexLeavesNonRepoUntouched(t *testing.T) {
	dir := t.TempDir()
	if _, ok, err := findWorkTree(dir); err != nil {
		t.Fatal(err)
	} else if ok {
		t.Skip("temp dir is inside a git work tree")
	}
	path, err := Index(dir, ".codegraph")
	if err != nil {
		t.Fatal(err)
	}
	if path != "" {
		t.Fatalf("path = %q", path)
	}
	if _, err := os.Stat(filepath.Join(dir, ".gitignore")); !os.IsNotExist(err) {
		t.Fatal("created .gitignore outside a work tree")
	}
}

func TestAnchoredAgentsPatternDoesNotIgnoreInternalAgents(t *testing.T) {
	root := t.TempDir()
	cmd := exec.Command("git", "init")
	cmd.Dir = root
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git init: %v\n%s", err, out)
	}
	internal := filepath.Join(root, "internal", "agents", "requirement", "planner.md")
	if err := os.MkdirAll(filepath.Dir(internal), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(internal, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "agents"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "agents", "note.md"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Ensure(root, []Entry{{Path: filepath.Join(root, "agents"), Dir: true}}); err != nil {
		t.Fatal(err)
	}
	if err := exec.Command("git", "-C", root, "check-ignore", "-q", "agents/note.md").Run(); err != nil {
		t.Fatal("agents/note.md should be ignored")
	}
	if err := exec.Command("git", "-C", root, "check-ignore", "-q", "internal/agents/requirement/planner.md").Run(); err == nil {
		t.Fatal("internal/agents should stay tracked")
	}
}
