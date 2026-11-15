package console

import (
	"path/filepath"
	"testing"
)

func TestApplyRootKeepsAnAbsoluteFolder(t *testing.T) {
	root := t.TempDir()
	custom := t.TempDir()
	snap, err := ApplyRoot(root, Snapshot{Repos: []Repo{
		{Kind: "backend", Folder: custom},
		{Kind: "website", Folder: "relative"},
	}})
	if err != nil {
		t.Fatal(err)
	}
	if got := snap.Repo("backend").Folder; got != custom {
		t.Fatalf("backend folder = %q, want %q", got, custom)
	}
	if got := snap.Repo("website").Folder; got != filepath.Join(root, "website") {
		t.Fatalf("website folder = %q", got)
	}
	if got := snap.Repo("document").Folder; got != filepath.Join(root, "docs") {
		t.Fatalf("document folder = %q", got)
	}
}
