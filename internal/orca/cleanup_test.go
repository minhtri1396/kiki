package orca

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRemoveTask(t *testing.T) {
	log := filepath.Join(t.TempDir(), "args")
	script := filepath.Join(t.TempDir(), "orca")
	body := `#!/bin/sh
printf '%s\n' "$*" >> "` + log + `"
if [ "$1" = "status" ]; then
  printf '%s\n' '{"ok":true,"result":{"app":{"running":true}}}'
  exit 0
fi
if [ "$1" = "project" ] && [ "$2" = "setups" ]; then
  printf '%s\n' '{"ok":true,"result":{"setups":[{"id":"setup_doc","displayName":"kiki-4-document","projectId":"p1","path":"/tmp/doc"},{"id":"setup_main","displayName":"kiki-document","projectId":"p0","path":"/tmp/main"}]}}'
  exit 0
fi
if [ "$1" = "worktree" ] && [ "$2" = "list" ]; then
  printf '%s\n' '{"ok":true,"result":{"worktrees":[{"path":"/tmp/doc-wt","branch":"refs/heads/feat/GE-12-ship_billing-docs","displayName":"12"}]}}'
  exit 0
fi
if [ "$1" = "worktree" ]; then
  printf '%s\n' '{"ok":false,"error":{"code":"selector_not_found","message":"selector_not_found"}}'
  exit 0
fi
if [ "$1" = "terminal" ]; then
  printf '%s\n' '{"ok":false,"error":{"code":"terminal_handle_stale","message":"terminal_handle_stale"}}'
  exit 0
fi
printf '%s\n' '{"ok":true}'
`
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	var methods []string
	client := &Client{
		Bin: script,
		Runtime: func(ctx context.Context, method string, params any) (json.RawMessage, error) {
			methods = append(methods, method)
			if method == "projectGroup.list" {
				return json.RawMessage(`{"groups":[{"id":"g1","name":"4"},{"id":"g2","name":"other"}]}`), nil
			}
			raw, _ := json.Marshal(params)
			if !strings.Contains(string(raw), `"groupId":"g1"`) {
				t.Fatalf("delete params = %s", raw)
			}
			return json.RawMessage(`{}`), nil
		},
	}
	err := client.RemoveTask(context.Background(), TaskScope{
		ID:        "12",
		GroupID:   "4",
		Workspace: "kiki",
		Slug:      "ship_billing",
		Ticket:    "GE-12",
		Repos:     []string{"document", "document"},
		Folders:   map[string]string{"document": "/tmp/doc"},
		Terminals: []string{"term_a", "term_a"},
	})
	if err != nil {
		t.Fatal(err)
	}
	text, err := os.ReadFile(log)
	if err != nil {
		t.Fatal(err)
	}
	got := string(text)
	for _, want := range []string{
		"terminal close --terminal term_a --tab --json",
		"worktree list --repo path:/tmp/doc --json",
		"worktree rm --worktree path:/tmp/doc-wt --json",
		"project setup-delete --setup setup_doc --force --json",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("log missing %q\n%s", want, got)
		}
	}
	if strings.Contains(got, "setup_main") || strings.Count(got, "terminal close") != 1 || strings.Count(got, "worktree rm") != 1 {
		t.Fatalf("log = %s", got)
	}
	if strings.Join(methods, ",") != "projectGroup.list,projectGroup.delete" {
		t.Fatalf("methods = %v", methods)
	}
}

func TestRemoveTaskKeepsSharedGroup(t *testing.T) {
	log := filepath.Join(t.TempDir(), "args")
	script := filepath.Join(t.TempDir(), "orca")
	body := `#!/bin/sh
printf '%s\n' "$*" >> "` + log + `"
if [ "$1" = "status" ]; then
  printf '%s\n' '{"ok":true,"result":{"app":{"running":true}}}'
  exit 0
fi
if [ "$1" = "project" ] && [ "$2" = "setups" ]; then
  printf '%s\n' '{"ok":true,"result":{"setups":[{"id":"setup_doc","displayName":"kiki-4-document","projectId":"p1","path":"/tmp/doc"}]}}'
  exit 0
fi
if [ "$1" = "worktree" ] && [ "$2" = "list" ]; then
  printf '%s\n' '{"ok":true,"result":{"worktrees":[{"path":"/tmp/doc-wt","displayName":"12"}]}}'
  exit 0
fi
printf '%s\n' '{"ok":true}'
`
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	var methods []string
	client := &Client{
		Bin: script,
		Runtime: func(ctx context.Context, method string, params any) (json.RawMessage, error) {
			methods = append(methods, method)
			return json.RawMessage(`{}`), nil
		},
	}
	err := client.RemoveTask(context.Background(), TaskScope{
		ID:        "12",
		GroupID:   "4",
		Workspace: "kiki",
		Slug:      "ship_billing",
		Ticket:    "GE-12",
		Repos:     []string{"document"},
		Folders:   map[string]string{"document": "/tmp/doc"},
		KeepGroup: true,
		KeepRepos: []string{"document"},
	})
	if err != nil {
		t.Fatal(err)
	}
	text, err := os.ReadFile(log)
	if err != nil {
		t.Fatal(err)
	}
	got := string(text)
	if !strings.Contains(got, "worktree rm --worktree path:/tmp/doc-wt --json") {
		t.Fatalf("log = %s", got)
	}
	if strings.Contains(got, "setup-delete") || len(methods) != 0 {
		t.Fatalf("shared group was removed\nlog=%s\nmethods=%v", got, methods)
	}
}

func TestOrcaGone(t *testing.T) {
	if !orcaGone([]byte(`{"ok":false,"error":{"code":"runtime_error","message":"Project host setup not found: x"}}`)) {
		t.Fatal("setup not found was kept")
	}
	if orcaGone([]byte(`{"ok":false,"error":{"code":"runtime_error","message":"still open"}}`)) {
		t.Fatal("a real failure was ignored")
	}
}
