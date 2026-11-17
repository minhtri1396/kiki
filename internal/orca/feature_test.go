package orca

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestFeatureNames(t *testing.T) {
	project, err := FeatureProjectName("studio", "12", "backend")
	if err != nil || project != "studio-12-backend" {
		t.Fatalf("project = %q err = %v", project, err)
	}
	id := "550e8400-e29b-41d4-a716-446655440000"
	named, err := FeatureProjectName("studio", id, "backend")
	if err != nil || named != "studio-"+id+"-backend" {
		t.Fatalf("uuid project = %q err = %v", named, err)
	}
	ulid := "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	named, err = FeatureProjectName("studio", ulid, "backend")
	if err != nil || named != "studio-"+ulid+"-backend" {
		t.Fatalf("ulid project = %q err = %v", named, err)
	}
	if _, err := FeatureProjectName("studio", "0", "backend"); err == nil {
		t.Fatal("group id 0 was accepted")
	}
	if !ValidTaskID(id) || ValidTaskID("550E8400-E29B-41D4-A716-446655440000") {
		t.Fatal("uuid task id check failed")
	}
	if !ValidTaskID(ulid) || ValidTaskID("01ARZ3NDEKTSV4RRFFQ69G5FAI") || ValidTaskID(strings.ToLower(ulid)) {
		t.Fatal("ulid task id check failed")
	}
	if _, err := FeatureProjectName("", "12", "backend"); err == nil {
		t.Fatal("empty workspace was accepted")
	}

	branch, err := FeatureBranch("ge-12", "add_shopping_cart", "document")
	if err != nil || branch != "feat/GE-12-add_shopping_cart-docs" {
		t.Fatalf("branch = %q err = %v", branch, err)
	}
	plain, err := FeatureBranch("", "add_cart", "backend")
	if err != nil || plain != "feat/add_cart-backend" {
		t.Fatalf("plain = %q err = %v", plain, err)
	}
	if _, err := FeatureBranch("not-a-key", "add_cart", "backend"); err == nil {
		t.Fatal("ticket was accepted")
	}
	if _, err := FeatureBranch("", "Add Cart", "backend"); err == nil {
		t.Fatal("slug was accepted")
	}
}

func TestSameGitRepo(t *testing.T) {
	root := t.TempDir()
	git := func(dir string, args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=kiki",
			"GIT_AUTHOR_EMAIL=kiki@example.com",
			"GIT_COMMITTER_NAME=kiki",
			"GIT_COMMITTER_EMAIL=kiki@example.com",
		)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	git(root, "init")
	git(root, "commit", "--allow-empty", "-m", "init")
	checkout := filepath.Join(t.TempDir(), "task")
	git(root, "worktree", "add", "-b", "feat/sample", checkout)
	if err := sameGitRepo(root, checkout); err != nil {
		t.Fatal(err)
	}
	other := t.TempDir()
	git(other, "init")
	if err := sameGitRepo(root, other); err == nil {
		t.Fatal("a different repository was accepted")
	}
}
