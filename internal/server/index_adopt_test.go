package server

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRememberPresentIndexesSavesTheFolderThatExists(t *testing.T) {
	app := newTeamTestServer(t)
	if err := app.Store.EnsureLayout(app.Root); err != nil {
		t.Fatal(err)
	}
	backend := filepath.Join(app.Root, "backend")
	docs := filepath.Join(app.Root, "docs")
	website := filepath.Join(app.Root, "website")
	mobile := filepath.Join(app.Root, "mobile")
	for _, dir := range []string{
		filepath.Join(backend, ".codegraph"),
		filepath.Join(docs, ".obsidian"),
		filepath.Join(website, ".codegraph"),
		filepath.Join(website, ".obsidian"),
		filepath.Join(mobile, ".codegraph"),
	} {
		if err := os.Mkdir(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	snap, err := app.load()
	if err != nil {
		t.Fatal(err)
	}
	if err := app.Store.SaveRepoIndex(snap, "mobile", "obsidian"); err != nil {
		t.Fatal(err)
	}
	snap, err = app.load()
	if err != nil {
		t.Fatal(err)
	}
	snap, err = app.rememberPresentIndexes(snap)
	if err != nil {
		t.Fatal(err)
	}
	if snap.Repo("backend").Index != "codegraph" {
		t.Fatalf("backend = %q", snap.Repo("backend").Index)
	}
	if snap.Repo("document").Index != "obsidian" {
		t.Fatalf("document = %q", snap.Repo("document").Index)
	}
	if snap.Repo("website").Index != "" {
		t.Fatalf("website = %q", snap.Repo("website").Index)
	}
	if snap.Repo("mobile").Index != "obsidian" {
		t.Fatalf("mobile = %q", snap.Repo("mobile").Index)
	}

	saved, err := app.load()
	if err != nil {
		t.Fatal(err)
	}
	if saved.Repo("backend").Index != "codegraph" || saved.Repo("document").Index != "obsidian" || saved.Repo("website").Index != "" || saved.Repo("mobile").Index != "obsidian" {
		t.Fatalf("saved backend=%q document=%q website=%q mobile=%q", saved.Repo("backend").Index, saved.Repo("document").Index, saved.Repo("website").Index, saved.Repo("mobile").Index)
	}
}
