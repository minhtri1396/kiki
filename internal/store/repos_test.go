package store

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"kiki/internal/console"
)

func TestRepoFilesKeepTheSavedIndex(t *testing.T) {
	root := t.TempDir()
	db, err := Open(Path(root))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	if err := db.EnsureLayout(root); err != nil {
		t.Fatal(err)
	}
	backend, err := os.ReadFile(filepath.Join(root, ".kiki", "repos", "backend.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(backend), "id: backend") || strings.Contains(string(backend), "kind:") || !strings.Contains(string(backend), filepath.Join(root, "backend")) {
		t.Fatalf("backend file:\n%s", backend)
	}
	if strings.Contains(string(backend), "index:") {
		t.Fatalf("index was set before setup:\n%s", backend)
	}

	snap, err := db.Load()
	if err != nil {
		t.Fatal(err)
	}
	if err := db.SaveRepoIndex(snap, "document", "obsidian"); err != nil {
		t.Fatal(err)
	}
	if err := db.SaveRepoIndex(snap, "backend", "nope"); err == nil {
		t.Fatal("expected an unknown index to be rejected")
	}
	snap, err = db.Load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("document").Index != "obsidian" || snap.Repo("backend").Index != "" {
		t.Fatalf("indexes = document %q backend %q", snap.Repo("document").Index, snap.Repo("backend").Index)
	}

	for i := range snap.Repos {
		if snap.Repos[i].Kind == "backend" {
			snap.Repos[i].Services = []string{"api"}
		}
	}
	if err := db.Save(snap); err != nil {
		t.Fatal(err)
	}
	if err := db.EnsureLayout(root); err != nil {
		t.Fatal(err)
	}
	snap, err = db.Load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("document").Index != "obsidian" {
		t.Fatalf("document index = %q", snap.Repo("document").Index)
	}
	if got := snap.Repo("backend").Services; len(got) != 1 || got[0] != "api" {
		t.Fatalf("services = %v", got)
	}
	doc, err := os.ReadFile(filepath.Join(root, ".kiki", "repos", "document.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(doc), "index: obsidian") {
		t.Fatalf("document file:\n%s", doc)
	}
	if err := db.SaveRepoIndex(snap, "document", ""); err != nil {
		t.Fatal(err)
	}
	snap, err = db.Load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("document").Index != "" {
		t.Fatalf("document index = %q", snap.Repo("document").Index)
	}
	doc, err = os.ReadFile(filepath.Join(root, ".kiki", "repos", "document.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(doc), "\nindex:") {
		t.Fatalf("index was kept:\n%s", doc)
	}
	if got := snap.Repo("backend").Services; len(got) != 1 || got[0] != "api" {
		t.Fatalf("services = %v", got)
	}
}

func TestRemoveIndexFolderDeletesOnlyThatDirectory(t *testing.T) {
	repo := t.TempDir()
	index := filepath.Join(repo, ".codegraph")
	if err := os.Mkdir(index, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(index, "db"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	kept := filepath.Join(repo, "AGENTS.md")
	if err := os.WriteFile(kept, []byte("rules"), 0o644); err != nil {
		t.Fatal(err)
	}
	other := filepath.Join(repo, ".obsidian")
	if err := os.Mkdir(other, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := RemoveIndexFolder(repo, "codegraph"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(index); !os.IsNotExist(err) {
		t.Fatal(".codegraph still exists")
	}
	if _, err := os.Stat(kept); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(other); err != nil {
		t.Fatal(".obsidian was removed")
	}
	if err := RemoveIndexFolder(repo, "codegraph"); err != nil {
		t.Fatal(err)
	}
	linkTarget := t.TempDir()
	if err := os.WriteFile(filepath.Join(linkTarget, "keep"), []byte("y"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(linkTarget, index); err != nil {
		t.Fatal(err)
	}
	if err := RemoveIndexFolder(repo, "codegraph"); err == nil {
		t.Fatal("expected a link to be rejected")
	}
	if _, err := os.Stat(filepath.Join(linkTarget, "keep")); err != nil {
		t.Fatal("link target was removed")
	}
}

func TestRepoFileReadsLegacyKind(t *testing.T) {
	root := t.TempDir()
	db, err := Open(Path(root))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	if err := db.EnsureLayout(root); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(root, ".kiki", "repos", "backend.yaml")
	legacy := "kind: backend\nfolder: " + filepath.Join(root, "backend") + "\nservices:\n    - api\n"
	if err := os.WriteFile(path, []byte(legacy), 0o644); err != nil {
		t.Fatal(err)
	}
	snap, err := db.Load()
	if err != nil {
		t.Fatal(err)
	}
	got := snap.Repo("backend").Services
	if len(got) != 1 || got[0] != "api" {
		t.Fatalf("services = %v", got)
	}
}

func TestRepoFilesKeepTheSparseCheckout(t *testing.T) {
	root := t.TempDir()
	db, err := Open(Path(root))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	if err := db.EnsureLayout(root); err != nil {
		t.Fatal(err)
	}
	snap, err := db.Load()
	if err != nil {
		t.Fatal(err)
	}
	if err := db.SaveRepoSparse(snap, "document", "product/kiki"); err != nil {
		t.Fatal(err)
	}
	if err := db.SaveRepoSparse(snap, "document", "../secret"); err == nil {
		t.Fatal("expected an invalid folder to be rejected")
	}
	snap, err = db.Load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("document").Sparse != "product/kiki" {
		t.Fatalf("sparse = %q", snap.Repo("document").Sparse)
	}
	for i := range snap.Repos {
		if snap.Repos[i].Kind == "backend" {
			snap.Repos[i].Services = []string{"api"}
		}
	}
	if err := db.Save(snap); err != nil {
		t.Fatal(err)
	}
	snap, err = db.Load()
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("document").Sparse != "product/kiki" {
		t.Fatalf("sparse after save = %q", snap.Repo("document").Sparse)
	}
	doc, err := os.ReadFile(filepath.Join(root, ".kiki", "repos", "document.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(doc), "sparse: product/kiki") {
		t.Fatalf("document file:\n%s", doc)
	}
}

func TestSaveRepoIndexRequiresAKnownRepository(t *testing.T) {
	root := t.TempDir()
	db, err := Open(Path(root))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	err = db.SaveRepoIndex(console.Snapshot{}, "docs", "obsidian")
	if err == nil {
		t.Fatal("expected an unknown repository")
	}
}
