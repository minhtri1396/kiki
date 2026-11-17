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
		MCPs: []agentdef.Use{{
			ID:          "atlassian_jira",
			Name:        "Atlassian - Jira",
			Description: "Read the Jira ticket.\nWrite the comment.",
			BoardID:     "12",
			BoardName:   "KIKI",
		}, {
			ID:          "atlassian_confluence",
			Name:        "Atlassian - Confluence",
			Description: "Read the design page.",
			SpaceID:     "111",
			SpaceKey:    "ENG",
			SpaceName:   "Engineering",
			ParentID:    "99",
			ParentTitle: "Requirements",
		}},
		Tools: []agentdef.Use{{ID: "github"}},
	}, "Body\n", Snapshot{
		Root:        "/app",
		SetupFile:   "/app/kiki.yaml",
		WorkspaceID: "kiki",
		Repos:       []Repo{{Kind: "document", Folder: "/app/docs", Index: "obsidian"}},
	}, "", []Peer{{
		ID: "requirement_designer",
		Handoffs: []Handoff{
			{When: "The requirement files are ready.", Send: "Hand over the requirement folder.\nWait for design.html.", Reply: "The design path.\nOpen questions.", MaxLoops: 3},
			{Send: "Say the folder is blocked."},
			{When: "No content", Send: " "},
		},
	}})
	for _, want := range []string{
		"KIKI started you with `orca agent launch`.",
		"Orca worktree: ",
		"You were started in the Orca worktree.",
		"Your agent id is requirement_document.",
		"- document",
		"Work in this repository:",
		"Write the requirement files in this folder.",
		"- agent id: requirement_designer",
		"When a condition is met, send that content to the agent through Orca",
		"Message 1 to requirement_designer:",
		"Send when:\n      The requirement files are ready.",
		"Hand over the requirement folder.",
		"Report back: yes. In this message, ask requirement_designer to report back to you, agent id requirement_document, when it has finished the work.",
		"The report from requirement_designer must contain:\n      The design path.\n      Open questions.",
		"Loop: this exchange can go back and forth between you and requirement_designer.",
		"Allow at most 3 loops.",
		"when the Send when condition of message 1 is met again after the report from requirement_designer",
		"\"loop 1 of 3\"",
		"After loop 3, do not send this message again",
		"Message 2 to requirement_designer:",
		"Send when: (no condition set)",
		"Say the folder is blocked.",
		"Report back: no. Do not ask requirement_designer to report back.",
		"Wait for design.html.",
		"MCP servers for this agent:",
		"- Atlassian - Jira",
		"Board: KIKI (12)",
		"Use this MCP server:",
		"Space: Engineering (ENG)",
		"Parent page: Requirements (99)",
		"Read the Jira ticket.",
		"Write the comment.",
		"- Atlassian - Confluence",
		"Read the design page.",
		"Tools for this agent:",
		"- github",
		"### document — Obsidian",
		"Index file: /app/.kiki/repos/document.yaml",
		"obsidian vault=<name> search query=\"<words>\" limit=20",
		"Read /app/docs/AGENTS.md before you search notes",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in\n%s", want, text)
		}
	}
	if strings.Count(text, "Loop: this exchange") != 1 || strings.Count(text, "must contain:") != 1 {
		t.Fatalf("a message without a loop or a reply got one:\n%s", text)
	}
	if strings.Contains(text, "Message 3 to") || strings.Contains(text, "No content") {
		t.Fatalf("a message with nothing to send was written:\n%s", text)
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

func TestPromptLeavesRTKToTheHook(t *testing.T) {
	text := Prompt(agentdef.Agent{
		ID:    "backend_runner",
		Title: "Backend Runner",
		Repos: []agentdef.Repo{{Kind: "backend"}},
		Tools: []agentdef.Use{{
			ID:          "github_cli",
			Name:        "GitHub CLI",
			Description: "The gh command opens pull requests, issues, and checks. Use gh for that work.",
		}},
	}, "Body\n", Snapshot{
		Root:  "/app",
		Repos: []Repo{{Kind: "backend", Folder: "/app/backend"}},
	}, "", nil)
	if strings.Contains(text, "RTK is on") || strings.Contains(text, "Use this tool:") || !strings.Contains(text, "- GitHub CLI") {
		t.Fatalf("prompt =\n%s", text)
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

func TestPromptSaysWhenARepositoryIsReadonly(t *testing.T) {
	text := Prompt(agentdef.Agent{
		ID:    "requirement_planner",
		Title: "Planner",
		Repos: []agentdef.Repo{
			{Kind: "document", Readonly: true},
			{Kind: "backend", Description: "Change the API."},
		},
	}, "Body\n", Snapshot{
		Root: "/app",
		Repos: []Repo{
			{Kind: "document", Folder: "/app/docs"},
			{Kind: "backend", Folder: "/app/backend"},
		},
	}, "", nil)
	if !strings.Contains(text, "Readonly: this agent can only read this repository.") {
		t.Fatalf("missing readonly note:\n%s", text)
	}
	if strings.Count(text, "Readonly: this agent can only read this repository.") != 1 {
		t.Fatalf("readonly note should be only on document:\n%s", text)
	}
}

func TestPromptNamesTheGuideFileOfEachRepository(t *testing.T) {
	text := Prompt(agentdef.Agent{
		ID:    "backend_runner",
		Title: "Backend Runner",
		Repos: []agentdef.Repo{{Kind: "backend", Guide: "RULES.md"}, {Kind: "document"}},
	}, "Body\n", Snapshot{
		Root: "/app",
		Repos: []Repo{
			{Kind: "backend", Folder: "/app/backend", Services: []string{"api"}, Index: "codegraph"},
			{Kind: "document", Folder: "/app/docs", Index: "obsidian"},
		},
	}, "", nil)
	for _, want := range []string{
		"guide file: /app/backend/RULES.md (read it and follow it",
		"guide file: /app/docs/AGENTS.md (read it and follow it",
		"read /app/backend/RULES.md",
		"/app/backend/<project root>/RULES.md",
		"If RULES.md is missing",
		"Read /app/docs/AGENTS.md before you search notes",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in\n%s", want, text)
		}
	}
	if strings.Contains(text, "/app/backend/AGENTS.md") {
		t.Fatalf("backend still names AGENTS.md:\n%s", text)
	}
}

func TestPromptWaitingTellsASupportingAgentToWait(t *testing.T) {
	agent := agentdef.Agent{ID: "requirement_designer", Title: "Designer", Repos: []agentdef.Repo{{Kind: "document"}}}
	snap := Snapshot{Root: "/app", Repos: []Repo{{Kind: "document", Folder: "/app/docs"}}}
	text := PromptWaiting(agent, "Body\n", snap, "Add a cart.", nil, []Requester{
		{ID: "requirement_document", Title: "Document Writer"},
		{ID: "requirement_designer"},
		{ID: " "},
	})
	for _, want := range []string{
		"WAIT FOR A REQUEST.",
		"listed under Agents Coordination by requirement_document (Document Writer).",
		"wait for a request from requirement_document (Document Writer), sent through Orca to your agent id",
		"It is context for the request that will reach you. It is not an instruction to start now.",
		"===== USER MESSAGE =====\nAdd a cart.",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in\n%s", want, text)
		}
	}
	if strings.Contains(text, "Treat that message as the user message in your terminal") || strings.Contains(text, "from requirement_designer") {
		t.Fatalf("waiting prompt still treats the message as an instruction or waits for itself:\n%s", text)
	}
	plain := PromptWaiting(agent, "Body\n", snap, "Add a cart.", nil, []Requester{{ID: "requirement_designer"}})
	if strings.Contains(plain, "WAIT FOR A REQUEST.") || !strings.Contains(plain, "Treat that message as the user message in your terminal") {
		t.Fatalf("an agent that only lists itself was told to wait:\n%s", plain)
	}
}
