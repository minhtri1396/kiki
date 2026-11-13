package console

import (
	"strings"
	"testing"

	"kiki/internal/agentdef"
)

func TestPromptNamesCollaboratorsByID(t *testing.T) {
	text := Prompt(agentdef.Agent{
		ID:    "requirement_document",
		Title: "Document Writer",
		Repos: []agentdef.Repo{{
			Kind:        "document",
			Description: "Write the requirement files in this folder.",
		}},
	}, "Body\n", Snapshot{
		Root:        "/app",
		SetupFile:   "/app/kiki.yaml",
		WorkspaceID: "kiki",
		Repos:       []Repo{{Kind: "document", Folder: "/app/docs", Index: "obsidian"}},
	}, "", []Peer{{
		ID:   "requirement_designer",
		Note: "Hand over the requirement folder.\nWait for design.html.",
	}})
	for _, want := range []string{
		"Your agent id is requirement_document.",
		"- document",
		"Work in this repository:",
		"Write the requirement files in this folder.",
		"- agent id: requirement_designer",
		"Hand over the requirement folder.",
		"Wait for design.html.",
		"### document — Obsidian",
		"Index file: /app/.kiki/repos/document.yaml",
		"obsidian vault=<name> search query=\"<words>\" limit=20",
		"Read /app/docs/AGENTS.md before you search notes",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in\n%s", want, text)
		}
	}
	if strings.Contains(text, "codegraph explore") || strings.Contains(text, "Obsidian vault:") {
		t.Fatalf("document agent prompt includes an index it is not set to use:\n%s", text)
	}
}

func TestPromptAddsCodeGraphOnlyForCodeRepos(t *testing.T) {
	text := Prompt(agentdef.Agent{
		ID:    "backend_runner",
		Title: "Backend Runner",
		Repos: []agentdef.Repo{{Kind: "backend"}},
	}, "Body\n", Snapshot{
		Root:  "/app",
		Repos: []Repo{{Kind: "backend", Folder: "/app/backend", Services: []string{"api"}, Index: "codegraph"}},
	}, "", nil)
	for _, want := range []string{
		"### backend — CodeGraph",
		"Folder: /app/backend",
		"Index file: /app/.kiki/repos/backend.yaml",
		"Services: api",
		"read /app/backend/AGENTS.md",
		"codegraph explore -p /app/backend",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in\n%s", want, text)
		}
	}
	if strings.Contains(text, "obsidian vault=") {
		t.Fatalf("backend agent prompt includes Obsidian:\n%s", text)
	}
}

func TestSparsePath(t *testing.T) {
	got, err := SparsePath(" /product/kiki/ ")
	if err != nil || got != "product/kiki" {
		t.Fatalf("got %q err %v", got, err)
	}
	if got, err := SparsePath("/product/kiki"); err != nil || got != "product/kiki" {
		t.Fatalf("leading slash got %q err %v", got, err)
	}
	if got, err := SparsePath(""); err != nil || got != "" {
		t.Fatalf("empty got %q err %v", got, err)
	}
	for _, raw := range []string{"../secret", "product/../../etc", "product/.git", `product\kiki`, "foo:bar"} {
		if _, err := SparsePath(raw); err == nil {
			t.Fatalf("accepted %q", raw)
		}
	}
}

func TestPromptNamesTheSparseCheckout(t *testing.T) {
	text := Prompt(agentdef.Agent{
		ID:    "requirement_document",
		Title: "Document Writer",
		Repos: []agentdef.Repo{{Kind: "document"}},
	}, "Body\n", Snapshot{
		Root:  "/app",
		Repos: []Repo{{Kind: "document", Folder: "/app/docs", Sparse: "product/kiki"}},
	}, "", nil)
	for _, want := range []string{
		"checkout: product/kiki",
		"sparse checkout of product/kiki",
		"Read and change files only inside product/kiki",
		"git commit, git push, and git pull",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in\n%s", want, text)
		}
	}
}

func TestPromptOmitsIndexUntilOneIsSaved(t *testing.T) {
	text := Prompt(agentdef.Agent{
		ID:    "backend_runner",
		Title: "Backend Runner",
		Repos: []agentdef.Repo{{Kind: "backend"}, {Kind: "document"}},
	}, "Body\n", Snapshot{
		Root: "/app",
		Repos: []Repo{
			{Kind: "backend", Folder: "/app/backend"},
			{Kind: "document", Folder: "/app/docs", Index: "obsidian"},
		},
	}, "", nil)
	if !strings.Contains(text, "### document — Obsidian") {
		t.Fatalf("missing the saved Obsidian index:\n%s", text)
	}
	if strings.Contains(text, "### backend — CodeGraph") || strings.Contains(text, "codegraph explore") {
		t.Fatalf("backend has no saved index and should be omitted:\n%s", text)
	}
}
