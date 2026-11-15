package jobs

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestStartCodegraphWritesGitignore(t *testing.T) {
	root := t.TempDir()
	repo := filepath.Join(root, "backend")
	if err := os.MkdirAll(repo, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(filepath.Join(repo, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	manager := New()
	manager.lookPath = func(name string) (string, error) {
		return "", fmt.Errorf("command %s was not found on PATH", name)
	}
	_, err := manager.StartCodegraph(root, "backend", repo)
	if err == nil || !strings.Contains(err.Error(), "codegraph") {
		t.Fatalf("err = %v", err)
	}
	got, err := os.ReadFile(filepath.Join(repo, ".gitignore"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(got), "/.codegraph/") || !strings.Contains(string(got), "/.obsidian/") {
		t.Fatalf("gitignore:\n%s", got)
	}
}

func TestCheckOneKeepsOtherResults(t *testing.T) {
	manager := New()
	manager.lookPath = func(name string) (string, error) {
		return "", fmt.Errorf("command %s was not found on PATH", name)
	}
	manager.last.Repos = map[string]Check{"backend": {Ready: true, Summary: "keep", CheckedAt: "earlier"}}
	dir := t.TempDir()
	report, err := manager.CheckOne(context.Background(), dir, []Target{{Kind: "document", Folder: dir, Index: "obsidian"}}, "document")
	if err != nil {
		t.Fatal(err)
	}
	if report.Repos["backend"].Summary != "keep" {
		t.Fatalf("backend = %+v", report.Repos["backend"])
	}
	if report.Repos["document"].CheckedAt == "" || !strings.Contains(report.Repos["document"].Summary, "obsidian") {
		t.Fatalf("document = %+v", report.Repos["document"])
	}
	if _, err := manager.CheckOne(context.Background(), dir, nil, "document"); err == nil {
		t.Fatal("expected unknown repository")
	}
}

func TestRefreshFoldersReportsTheIndexDirectory(t *testing.T) {
	root := t.TempDir()
	docs := filepath.Join(root, "docs")
	backend := filepath.Join(root, "backend")
	if err := os.MkdirAll(filepath.Join(docs, ".codegraph"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(backend, 0o755); err != nil {
		t.Fatal(err)
	}
	manager := New()
	manager.last.Repos = map[string]Check{"website": {Summary: "keep", CheckedAt: "earlier"}}
	report, err := manager.RefreshFolders([]Target{
		{Kind: "document", Folder: docs, Index: "codegraph"},
		{Kind: "backend", Folder: backend, Index: "codegraph"},
		{Kind: "website", Folder: root, Index: "obsidian"},
	}, "codegraph")
	if err != nil {
		t.Fatal(err)
	}
	if !report.Repos["document"].DirExists || report.Repos["document"].Summary != ".codegraph exists" {
		t.Fatalf("document = %+v", report.Repos["document"])
	}
	if report.Repos["backend"].DirExists || report.Repos["backend"].Summary != ".codegraph is missing" {
		t.Fatalf("backend = %+v", report.Repos["backend"])
	}
	if report.Repos["website"].Summary != "keep" {
		t.Fatalf("website = %+v", report.Repos["website"])
	}
	if _, err := manager.RefreshFolders(nil, "git"); err == nil {
		t.Fatal("expected unknown index")
	}
}

func TestCheckUsesTheSavedIndex(t *testing.T) {
	manager := New()
	manager.lookPath = func(name string) (string, error) {
		return "", fmt.Errorf("command %s was not found on PATH", name)
	}
	dir := t.TempDir()
	report, err := manager.Check(context.Background(), dir, []Target{
		{Kind: "document", Folder: dir, Index: "codegraph"},
		{Kind: "backend", Folder: dir},
	})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(report.Repos["document"].Summary, "codegraph") {
		t.Fatalf("document = %+v", report.Repos["document"])
	}
	if report.Repos["backend"].Summary != "No index is set for this repository" {
		t.Fatalf("backend = %+v", report.Repos["backend"])
	}
}

func TestObsidianCheckRetriesWhenTheVaultCommandIsMissing(t *testing.T) {
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, ".obsidian"), 0o755); err != nil {
		t.Fatal(err)
	}
	abs, err := filepath.Abs(dir)
	if err != nil {
		t.Fatal(err)
	}
	count := filepath.Join(dir, "count")
	script := filepath.Join(dir, "obsidian")
	body := fmt.Sprintf(`#!/bin/sh
n=0
if [ -f %[1]q ]; then n=$(cat %[1]q); fi
n=$((n+1))
printf '%%s' "$n" > %[1]q
if [ "$n" -eq 1 ]; then
  printf '%%s\n' 'Command "vault" not found'
  exit 1
fi
printf '%%s\n' %[2]q
`, count, abs)
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	previous := obsidianCommandRetryDelay
	obsidianCommandRetryDelay = time.Millisecond
	t.Cleanup(func() { obsidianCommandRetryDelay = previous })

	manager := New()
	manager.lookPath = func(name string) (string, error) {
		if name != "obsidian" {
			return "", fmt.Errorf("command %s was not found on PATH", name)
		}
		return script, nil
	}
	report, err := manager.CheckOne(context.Background(), dir, []Target{{Kind: "document", Folder: dir, Index: "obsidian"}}, "document")
	if err != nil {
		t.Fatal(err)
	}
	got := report.Repos["document"]
	if !got.Ready || !strings.Contains(got.Summary, abs) {
		t.Fatalf("document = %+v", got)
	}
	tries, err := os.ReadFile(count)
	if err != nil {
		t.Fatal(err)
	}
	if string(tries) != "2" {
		t.Fatalf("tries = %s", tries)
	}
}
