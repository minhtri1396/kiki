package guard

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestDecideAllowsSelectedJiraAndBlocksConfluence(t *testing.T) {
	root := t.TempDir()
	policy := Policy{
		AgentID:   "reader",
		Workspace: "kiki",
		MCPs:      []string{"atlassian_jira"},
		Roots:     []string{root},
	}
	jira := policy.Decide(root, "", Classify("beforeMCPExecution", []byte(`{"server":"mcp-atlassian-kiki","tool_name":"jira_search"}`)))
	if !jira.Allow {
		t.Fatalf("jira = %+v", jira)
	}
	page := policy.Decide(root, "", Classify("beforeMCPExecution", []byte(`{"server":"mcp-atlassian-kiki","tool_name":"confluence_get_page"}`)))
	if page.Allow || !strings.Contains(page.Message, "confluence_get_page") {
		t.Fatalf("confluence = %+v", page)
	}
	other := policy.Decide(root, "", []Target{{Kind: "mcp", Name: "mcp-codegraph-kiki"}})
	if other.Allow {
		t.Fatal("codegraph was allowed")
	}
}

func TestDecideKeepsRepositoryWorkInsideTheSelectedFolder(t *testing.T) {
	root := t.TempDir()
	outside := t.TempDir()
	policy := Policy{AgentID: "reader", Roots: []string{root}}
	inside := policy.Decide(root, "", Classify("beforeReadFile", []byte(`{"file_path":"`+filepath.Join(root, "notes.md")+`"}`)))
	if !inside.Allow {
		t.Fatalf("inside = %+v", inside)
	}
	out := policy.Decide(root, "", Classify("beforeShellExecution", []byte(`{"command":"cat `+filepath.Join(outside, "secret")+`"}`)))
	if out.Allow || !strings.Contains(out.Message, outside) {
		t.Fatalf("outside = %+v", out)
	}
	shell := policy.Decide(root, "", Classify("preToolUse", []byte(`{"tool_name":"Shell","tool_input":{"command":"ls"}}`)))
	if !shell.Allow {
		t.Fatalf("shell = %+v", shell)
	}
}

func TestDecideBlocksGitHubCLIAndRTKUnlessTheyAreSet(t *testing.T) {
	root := t.TempDir()
	without := Policy{AgentID: "reader", Roots: []string{root}}
	gh := without.Decide(root, "", Classify("beforeShellExecution", []byte(`{"command":"gh pr create"}`)))
	if gh.Allow || !strings.Contains(gh.Message, "GitHub CLI") {
		t.Fatalf("gh = %+v", gh)
	}
	rtk := without.Decide(root, "", Classify("preToolUse", []byte(`{"tool_name":"Shell","tool_input":{"command":"rtk git status"}}`)))
	if rtk.Allow || !strings.Contains(rtk.Message, "RTK is off") {
		t.Fatalf("rtk = %+v", rtk)
	}
	with := Policy{AgentID: "reader", Tools: []string{"github_cli"}, Roots: []string{root}, RTK: true}
	if out := with.Decide(root, "", Classify("beforeShellExecution", []byte(`{"command":"sudo -n gh pr list"}`))); !out.Allow {
		t.Fatalf("allowed gh = %+v", out)
	}
	if out := with.Decide(root, "", Classify("beforeShellExecution", []byte(`{"command":"rtk gh pr list"}`))); !out.Allow {
		t.Fatalf("allowed rtk gh = %+v", out)
	}
	if out := with.Decide(root, "", Classify("beforeShellExecution", []byte(`{"command":"ls"}`))); !out.Allow {
		t.Fatalf("ls = %+v", out)
	}
	other := t.TempDir()
	if out := with.Decide(other, "", Classify("beforeShellExecution", []byte(`{"command":"rtk git status"}`))); out.Allow {
		t.Fatalf("rtk outside = %+v", out)
	}
}

func TestDecideBlocksToolsAndAgentsThatWereNotSelected(t *testing.T) {
	root := t.TempDir()
	policy := Policy{
		AgentID: "reader",
		Tools:   []string{"preview"},
		Agents:  []string{"planner"},
		Roots:   []string{root},
	}
	tool := policy.Decide(root, "", []Target{{Kind: "tool", Name: "web_search"}})
	if tool.Allow || !strings.Contains(tool.Message, "web_search") {
		t.Fatalf("tool = %+v", tool)
	}
	okTool := policy.Decide(root, "", []Target{{Kind: "tool", Name: "preview"}})
	if !okTool.Allow {
		t.Fatalf("preview = %+v", okTool)
	}
	peer := policy.Decide(root, "", Classify("subagentStart", []byte(`{"subagent_type":"reviewer"}`)))
	if peer.Allow || !strings.Contains(peer.Message, "reviewer") {
		t.Fatalf("peer = %+v", peer)
	}
	allowed := policy.Decide(root, "", Classify("preToolUse", []byte(`{"tool_name":"Task","tool_input":{"subagent_type":"planner"}}`)))
	if !allowed.Allow {
		t.Fatalf("planner = %+v", allowed)
	}
	launch := policy.Decide(root, "", Classify("beforeShellExecution", []byte(`{"command":"orca agent launch --agent reviewer"}`)))
	if launch.Allow {
		t.Fatal("launch was allowed")
	}
}

func TestPromptHasNoInteractionToBlock(t *testing.T) {
	raw := []byte(`{"hook_event_name":"UserPromptSubmit","prompt":"use the other repository"}`)
	if got := Classify("UserPromptSubmit", raw); len(got) != 0 {
		t.Fatalf("targets = %+v", got)
	}
	result := Policy{AgentID: "reader"}.Decide("/tmp", "", nil)
	if !result.Allow {
		t.Fatalf("prompt = %+v", result)
	}
}

func TestResolveUsesTheStartedAgentWhenSeveralShareARepository(t *testing.T) {
	root := t.TempDir()
	a := Policy{AgentID: "reader", Roots: []string{root}, MCPs: []string{"atlassian_jira"}}
	b := Policy{AgentID: "writer", Roots: []string{root}}
	q := Query{
		Cwd:      root,
		Policies: []Policy{a, b},
		Bindings: []Binding{{AgentID: "reader", Worktree: root}},
	}
	got := Resolve(q)
	if got.Status != StatusUse || got.Policy.AgentID != "reader" {
		t.Fatalf("resolve = %+v", got)
	}
	q.Bindings = nil
	got = Resolve(q)
	if got.Status != StatusDeny || !strings.Contains(got.Message, "reader") || !strings.Contains(got.Message, "writer") {
		t.Fatalf("ambiguous = %+v", got)
	}
	q.Agent = "writer"
	got = Resolve(q)
	if got.Status != StatusUse || got.Policy.AgentID != "writer" {
		t.Fatalf("named = %+v", got)
	}
}

func TestResolveAllowsAFolderThatIsNotARepository(t *testing.T) {
	repo := t.TempDir()
	other := t.TempDir()
	got := Resolve(Query{
		Cwd:       other,
		Policies:  []Policy{{AgentID: "reader", Roots: []string{repo}}},
		Workspace: []string{repo},
	})
	if got.Status != StatusOutside {
		t.Fatalf("outside = %+v", got)
	}
	got = Resolve(Query{
		Cwd:       repo,
		Workspace: []string{repo},
	})
	if got.Status != StatusDeny || !strings.Contains(got.Message, "no agent") {
		t.Fatalf("empty = %+v", got)
	}
}

func TestReadonlyRepositoryAllowsReadsAndBlocksWrites(t *testing.T) {
	docs := t.TempDir()
	code := t.TempDir()
	policy := Policy{
		AgentID:  "planner",
		Roots:    []string{docs, code},
		ReadOnly: []string{docs},
	}
	note := filepath.Join(docs, "notes.md")
	read := policy.Decide(docs, "", Classify("beforeReadFile", []byte(`{"file_path":"`+note+`"}`)))
	if !read.Allow {
		t.Fatalf("read = %+v", read)
	}
	status := policy.Decide(docs, "", Classify("beforeShellExecution", []byte(`{"command":"git status"}`)))
	if !status.Allow {
		t.Fatalf("status = %+v", status)
	}
	listed := policy.Decide(docs, "", Classify("preToolUse", []byte(`{"tool_name":"Shell","tool_input":{"command":"ls"}}`)))
	if !listed.Allow {
		t.Fatalf("ls = %+v", listed)
	}
	edit := policy.Decide(docs, "", Classify("preToolUse", []byte(`{"tool_name":"Write","tool_input":{"path":"`+note+`"}}`)))
	if edit.Allow || !strings.Contains(edit.Message, "readonly") {
		t.Fatalf("edit = %+v", edit)
	}
	commit := policy.Decide(docs, "", Classify("beforeShellExecution", []byte(`{"command":"git commit -am msg"}`)))
	if commit.Allow || !strings.Contains(commit.Message, "readonly") {
		t.Fatalf("commit = %+v", commit)
	}
	redirect := policy.Decide(docs, "", Classify("beforeShellExecution", []byte(`{"command":"echo hi > notes.md"}`)))
	if redirect.Allow {
		t.Fatalf("redirect = %+v", redirect)
	}
	inplace := policy.Decide(docs, "", Classify("beforeShellExecution", []byte(`{"command":"sed -i '' 's/a/b/' notes.md"}`)))
	if inplace.Allow {
		t.Fatalf("sed = %+v", inplace)
	}
	codeFile := filepath.Join(code, "main.go")
	change := policy.Decide(code, "", Classify("preToolUse", []byte(`{"tool_name":"Edit","tool_input":{"path":"`+codeFile+`"}}`)))
	if !change.Allow {
		t.Fatalf("writable repo = %+v", change)
	}
	cross := policy.Decide(code, "", Classify("preToolUse", []byte(`{"tool_name":"Edit","tool_input":{"path":"`+note+`"}}`)))
	if cross.Allow || !strings.Contains(cross.Message, note) {
		t.Fatalf("cross edit = %+v", cross)
	}
}

func TestReadonlyWorktreeBlocksWrites(t *testing.T) {
	repo := t.TempDir()
	work := t.TempDir()
	got := Resolve(Query{
		Cwd:   work,
		Agent: "reader",
		Policies: []Policy{{
			AgentID:  "reader",
			Roots:    []string{repo},
			ReadOnly: []string{repo},
		}},
		Bindings: []Binding{{AgentID: "reader", Worktree: work, ReadOnly: true}},
	})
	if got.Status != StatusUse {
		t.Fatalf("resolve = %+v", got)
	}
	file := filepath.Join(work, "main.go")
	edit := got.Policy.Decide(work, "", Classify("preToolUse", []byte(`{"tool_name":"Edit","tool_input":{"path":"`+file+`"}}`)))
	if edit.Allow || !strings.Contains(edit.Message, "readonly") {
		t.Fatalf("worktree edit = %+v", edit)
	}
	read := got.Policy.Decide(work, "", Classify("beforeReadFile", []byte(`{"file_path":"`+file+`"}`)))
	if !read.Allow {
		t.Fatalf("worktree read = %+v", read)
	}
}

func TestReplyUsesTheToolHookShape(t *testing.T) {
	cursor := Reply("cursor", "beforeMCPExecution", false, "blocked")
	if !strings.Contains(cursor, `"permission":"deny"`) || !strings.Contains(cursor, "blocked") {
		t.Fatalf("cursor = %s", cursor)
	}
	claude := Reply("claude", "PreToolUse", false, "blocked")
	if !strings.Contains(claude, `"permissionDecision":"deny"`) {
		t.Fatalf("claude = %s", claude)
	}
	prompt := Reply("codex", "UserPromptSubmit", true, "")
	if strings.TrimSpace(prompt) != "{}" {
		t.Fatalf("prompt = %s", prompt)
	}
	gemini := Reply("gemini", "BeforeTool", false, "blocked")
	if !strings.Contains(gemini, `"decision":"deny"`) {
		t.Fatalf("gemini = %s", gemini)
	}
}

func TestInstallKeepsExistingHooks(t *testing.T) {
	home := t.TempDir()
	cursor := filepath.Join(home, ".cursor", "hooks.json")
	if err := os.MkdirAll(filepath.Dir(cursor), 0o755); err != nil {
		t.Fatal(err)
	}
	original := []byte("{\n  \"version\": 1,\n  \"hooks\": {\n    \"beforeShellExecution\": [\n      {\"command\": \"echo keep\"}\n    ]\n  }\n}\n")
	if err := os.WriteFile(cursor, original, 0o644); err != nil {
		t.Fatal(err)
	}
	gemini := filepath.Join(home, ".gemini", "settings.json")
	if err := os.MkdirAll(filepath.Dir(gemini), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(gemini, []byte("{\n  \"mcpServers\": {\"keep\": {\"command\": \"uvx\"}}\n}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	broken := filepath.Join(home, ".codex", "hooks.json")
	if err := os.MkdirAll(filepath.Dir(broken), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(broken, []byte("{"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Install(home, "/Applications/KIKI"); err == nil || !strings.Contains(err.Error(), "Codex") {
		t.Fatalf("install = %v", err)
	}
	text, err := os.ReadFile(cursor)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(text), "echo keep") || !strings.Contains(string(text), "guardrail cursor") {
		t.Fatalf("cursor hooks = %s", text)
	}
	if _, err := os.ReadFile(broken); err != nil {
		t.Fatal(err)
	}
	same, err := os.ReadFile(broken)
	if err != nil {
		t.Fatal(err)
	}
	if string(same) != "{" {
		t.Fatalf("broken file changed: %s", same)
	}
	if err := os.Remove(broken); err != nil {
		t.Fatal(err)
	}
	if err := Install(home, "/Applications/KIKI"); err != nil {
		t.Fatal(err)
	}
	if err := Install(home, "/Applications/KIKI"); err != nil {
		t.Fatal(err)
	}
	text, err = os.ReadFile(cursor)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Count(string(text), "guardrail cursor") != len(cursorEvents) {
		t.Fatalf("cursor count = %d\n%s", strings.Count(string(text), "guardrail cursor"), text)
	}
	settings, err := os.ReadFile(gemini)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(settings), `"keep"`) || strings.Count(string(settings), "guardrail gemini") != len(geminiEvents) {
		t.Fatalf("gemini = %s", settings)
	}
	for _, tool := range []string{"cursor", "claude", "codex", "gemini"} {
		if err := HooksReady(home, tool); err != nil {
			t.Fatalf("%s: %v", tool, err)
		}
	}
	if err := Remove(home); err != nil {
		t.Fatal(err)
	}
	text, err = os.ReadFile(cursor)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(text), "guardrail cursor") || !strings.Contains(string(text), "echo keep") {
		t.Fatalf("cursor hooks after removal = %s", text)
	}
	for _, tool := range []string{"cursor", "claude", "codex", "gemini"} {
		if err := HooksReady(home, tool); err == nil {
			t.Fatalf("%s guardrail hook stayed", tool)
		}
	}
}

func TestGateAllowsWhenTheAppIsDownAndBlocksWhenItRefuses(t *testing.T) {
	home := t.TempDir()
	repo := t.TempDir()
	other := t.TempDir()
	if err := Publish(home, Endpoint{URL: "http://127.0.0.1:9", Workspace: "kiki", Paths: []string{repo}}); err != nil {
		t.Fatal(err)
	}
	down := func(string, []byte) (bool, string, error) {
		return false, "", os.ErrNotExist
	}
	owned := Gate(home, "cursor", []byte(`{"cwd":"`+repo+`","hook_event_name":"beforeShellExecution","command":"ls"}`), "", down)
	if !strings.Contains(owned, `"permission":"allow"`) {
		t.Fatalf("app down = %s", owned)
	}
	called := false
	outside := Gate(home, "cursor", []byte(`{"cwd":"`+other+`","command":"ls"}`), "", func(string, []byte) (bool, string, error) {
		called = true
		return false, "", os.ErrNotExist
	})
	if called || !strings.Contains(outside, `"permission":"allow"`) {
		t.Fatalf("outside called=%v reply=%s", called, outside)
	}
	refused := Gate(home, "cursor", []byte(`{"cwd":"`+repo+`","hook_event_name":"beforeMCPExecution","server":"mcp-other"}`), "", func(string, []byte) (bool, string, error) {
		return false, "Guardrail: this agent cannot use the MCP server mcp-other.", nil
	})
	if !strings.Contains(refused, `"permission":"deny"`) || !strings.Contains(refused, "mcp-other") {
		t.Fatalf("refused = %s", refused)
	}
}
