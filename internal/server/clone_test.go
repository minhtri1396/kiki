package server

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestFolderEmptyIgnoresIndexSetup(t *testing.T) {
	dir := t.TempDir()
	if empty, err := folderEmpty(dir); err != nil || !empty {
		t.Fatalf("empty dir: empty=%v err=%v", empty, err)
	}

	if err := os.Mkdir(filepath.Join(dir, ".codegraph"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".codegraph", "db"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(dir, ".obsidian"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".obsidian", "app.json"), []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".DS_Store"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if empty, err := folderEmpty(dir); err != nil || !empty {
		t.Fatalf("setup only: empty=%v err=%v", empty, err)
	}

	if err := os.WriteFile(filepath.Join(dir, "README.md"), []byte("hi\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if empty, err := folderEmpty(dir); err != nil || empty {
		t.Fatalf("with a file: empty=%v err=%v", empty, err)
	}
}

func TestFolderEmptyMissingPath(t *testing.T) {
	empty, err := folderEmpty(filepath.Join(t.TempDir(), "missing"))
	if err != nil || !empty {
		t.Fatalf("empty=%v err=%v", empty, err)
	}
}

func TestGitCloneReplacesIndexSetup(t *testing.T) {
	src := t.TempDir()
	runGit(t, src, "init", "-b", "main")
	if err := os.WriteFile(filepath.Join(src, "README.md"), []byte("hello\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, src, "add", "README.md")
	runGit(t, src, "commit", "-m", "init")

	dest := t.TempDir()
	marker := filepath.Join(dest, ".obsidian", "app.json")
	if err := os.MkdirAll(filepath.Dir(marker), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(marker, []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(filepath.Join(dest, ".codegraph"), 0o755); err != nil {
		t.Fatal(err)
	}
	if empty, err := folderEmpty(dest); err != nil || !empty {
		t.Fatalf("before clone: empty=%v err=%v", empty, err)
	}

	if err := gitClone(context.Background(), src, dest, ""); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(filepath.Join(dest, "README.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != "hello\n" {
		t.Fatalf("README = %q", got)
	}
	if _, err := os.Stat(filepath.Join(dest, ".git")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(marker); !os.IsNotExist(err) {
		t.Fatal("obsidian setup was left in the cloned folder")
	}
	if _, err := os.Stat(filepath.Join(dest, ".codegraph")); !os.IsNotExist(err) {
		t.Fatal("codegraph setup was left in the cloned folder")
	}
}

func TestGitCloneKeepsSetupWhenCloneFails(t *testing.T) {
	dest := t.TempDir()
	marker := filepath.Join(dest, ".obsidian", "app.json")
	if err := os.MkdirAll(filepath.Dir(marker), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(marker, []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	err := gitClone(context.Background(), filepath.Join(t.TempDir(), "missing.git"), dest, "")
	if err == nil {
		t.Fatal("expected clone to fail")
	}
	if _, err := os.Stat(marker); err != nil {
		t.Fatal("setup was removed after a failed clone")
	}
}

func TestGitCloneSparseChecksOutOneFolder(t *testing.T) {
	src := t.TempDir()
	runGit(t, src, "init", "-b", "main")
	writeRepoFile(t, src, "README.md", "root\n")
	writeRepoFile(t, src, "product/kiki/note.md", "hello\n")
	writeRepoFile(t, src, "other/secret.md", "secret\n")
	runGit(t, src, "add", ".")
	runGit(t, src, "commit", "-m", "init")

	dest := t.TempDir()
	if err := gitClone(context.Background(), src, dest, "product/kiki"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dest, "product", "kiki", "note.md")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dest, "README.md")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dest, "other", "secret.md")); !os.IsNotExist(err) {
		t.Fatal("other folder was checked out")
	}

	if err := os.WriteFile(filepath.Join(dest, "product", "kiki", "note.md"), []byte("edited\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, dest, "add", "product/kiki/note.md")
	runGit(t, dest, "commit", "-m", "edit")
	tree := gitOutput(t, dest, "ls-tree", "-r", "--name-only", "HEAD")
	for _, name := range []string{"README.md", "product/kiki/note.md", "other/secret.md"} {
		if !strings.Contains(tree, name) {
			t.Fatalf("commit tree missing %s:\n%s", name, tree)
		}
	}
	status := gitOutput(t, dest, "status", "--porcelain")
	if strings.Contains(status, "other/") {
		t.Fatalf("status touches the other folder:\n%s", status)
	}
}

func TestGitCloneSparsePullKeepsTheOtherFolderOut(t *testing.T) {
	src := t.TempDir()
	runGit(t, src, "init", "-b", "main")
	writeRepoFile(t, src, "README.md", "root\n")
	writeRepoFile(t, src, "product/kiki/note.md", "hello\n")
	writeRepoFile(t, src, "other/secret.md", "secret\n")
	runGit(t, src, "add", ".")
	runGit(t, src, "commit", "-m", "init")

	dest := t.TempDir()
	if err := gitClone(context.Background(), src, dest, "product/kiki"); err != nil {
		t.Fatal(err)
	}
	writeRepoFile(t, src, "product/kiki/note.md", "from remote\n")
	writeRepoFile(t, src, "other/secret.md", "still secret\n")
	runGit(t, src, "add", ".")
	runGit(t, src, "commit", "-m", "remote")
	runGit(t, dest, "pull")

	got, err := os.ReadFile(filepath.Join(dest, "product", "kiki", "note.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != "from remote\n" {
		t.Fatalf("note = %q", got)
	}
	if _, err := os.Stat(filepath.Join(dest, "other", "secret.md")); !os.IsNotExist(err) {
		t.Fatal("pull checked out the other folder")
	}
	secret := gitOutput(t, dest, "show", "HEAD:other/secret.md")
	if secret != "still secret\n" {
		t.Fatalf("remote file = %q", secret)
	}
}

func TestGitCloneSparseRejectsAMissingFolder(t *testing.T) {
	src := t.TempDir()
	runGit(t, src, "init", "-b", "main")
	writeRepoFile(t, src, "README.md", "hello\n")
	runGit(t, src, "add", "README.md")
	runGit(t, src, "commit", "-m", "init")

	dest := t.TempDir()
	marker := filepath.Join(dest, ".obsidian", "app.json")
	if err := os.MkdirAll(filepath.Dir(marker), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(marker, []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	err := gitClone(context.Background(), src, dest, "missing/folder")
	if err == nil || !strings.Contains(err.Error(), "not in this repository") {
		t.Fatal(err)
	}
	if _, err := os.Stat(marker); err != nil {
		t.Fatal("setup was removed after a failed clone")
	}
	if _, err := os.Stat(filepath.Join(dest, "README.md")); !os.IsNotExist(err) {
		t.Fatal("a failed sparse clone was left in the folder")
	}
}

func writeRepoFile(t *testing.T, root, name, text string) {
	t.Helper()
	path := filepath.Join(root, filepath.FromSlash(name))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
}

func gitOutput(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
	return string(out)
}

func runGit(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir, "-c", "user.email=kiki@example.com", "-c", "user.name=Kiki"}, args...)...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
}
