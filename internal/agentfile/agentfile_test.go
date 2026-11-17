package agentfile

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestResolveDefaultsToAgentsBesideApp(t *testing.T) {
	root := t.TempDir()
	got, err := Resolve(root, "")
	if err != nil {
		t.Fatal(err)
	}
	if got != filepath.Join(root, ".kiki", "agents") {
		t.Fatalf("path = %s", got)
	}
}

func TestResolveRejectsRelativePath(t *testing.T) {
	root := t.TempDir()
	if _, err := Resolve(root, "agents"); err == nil {
		t.Fatal("expected an error for a relative agent path")
	}
}

func TestEnsureCreatesFolderAndAgentFiles(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(filepath.Join(dir, "requirement", "requirement_document.md"))
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	for _, want := range []string{
		"id: requirement_document\n",
		"name: Document Writer\n",
		"ai_tool: cursor\n",
		"model: auto\n",
		"enabled: true\n",
		"agents_coordination:\n",
		"id: requirement_designer\n",
		"description:",
		"tell the user to run agent id requirement_designer",
		"id: document\n",
		"business_requirement.md and product_requirement.md in this folder.",
		"# Agent: Requirement Document Writer",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in\n%s", want, text)
		}
	}
	frontMatter, body, splitOK := split(text)
	if !splitOK {
		t.Fatal("seeded document file has no front matter")
	}
	if strings.Contains(frontMatter, "kind:") || strings.Contains(frontMatter, "teamwork:") || strings.Contains(frontMatter, "works_with:") || strings.Contains(frontMatter, "collaboration:") {
		t.Fatalf("seeded front matter still uses an old Agents Coordination key:\n%s", frontMatter)
	}
	if strings.Contains(body, "## Works with agents") || strings.Contains(body, "**Agent ID:**") || strings.Contains(body, "**Document repository:**") {
		t.Fatal("seeded prompt repeats fields the launch prompt already includes")
	}
	designer, err := os.ReadFile(filepath.Join(dir, "requirement", "requirement_designer.md"))
	if err != nil {
		t.Fatal(err)
	}
	front, _, ok := split(string(designer))
	if !ok {
		t.Fatal("designer file has no front matter")
	}
	if !strings.Contains(front, "id: requirement_document\n") || !strings.Contains(front, "Do not message agent id requirement_document") {
		t.Fatalf("designer front matter is missing the collaboration note:\n%s", front)
	}
	planner, ok, err := Find(dir, "requirement_planner")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if len(planner.WorksWith) != 5 {
		t.Fatalf("planner works with = %#v", planner.WorksWith)
	}
	for _, peer := range planner.WorksWith {
		if len(peer.Handoffs) == 0 || strings.TrimSpace(peer.Handoffs[0].Send) == "" {
			t.Fatalf("planner peer %s has no message", peer.ID)
		}
	}
	if len(planner.Repos) != 4 {
		t.Fatalf("planner repos = %#v", planner.Repos)
	}
	for _, repo := range planner.Repos {
		if strings.TrimSpace(repo.Description) == "" {
			t.Fatalf("planner repo %s has no description", repo.Kind)
		}
	}
	if len(planner.MCPs) != 2 || planner.MCPs[0].ID != "atlassian_jira" || planner.MCPs[1].ID != "atlassian_confluence" || !strings.Contains(planner.MCPs[0].Description, "Do not call Jira until the user gives a requirement folder.") || !strings.Contains(planner.MCPs[1].Description, "Do not create, edit, or delete a page.") {
		t.Fatalf("planner mcps = %#v", planner.MCPs)
	}
	document, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if len(document.MCPs) != 0 || len(document.Tools) != 0 {
		t.Fatalf("document uses = %#v %#v", document.MCPs, document.Tools)
	}
	runner, ok, err := Find(dir, "backend_runner")
	if err != nil || !ok || len(runner.WorksWith) != 1 {
		t.Fatal(err, ok)
	}
	if runner.WorksWith[0].ID != "backend_reviewer" || len(runner.WorksWith[0].Handoffs) != 1 || !strings.Contains(runner.WorksWith[0].Handoffs[0].Send, "[backend_runner -> backend_reviewer]") {
		t.Fatalf("runner collaboration = %#v", runner.WorksWith[0])
	}
	if len(runner.MCPs) != 2 || runner.MCPs[0].ID != "atlassian_jira" || runner.MCPs[1].ID != "atlassian_confluence" || !strings.Contains(runner.MCPs[0].Description, "Do not search Jira for work.") || !strings.Contains(runner.MCPs[1].Description, "when the ticket or the user names it") {
		t.Fatalf("runner mcps = %#v", runner.MCPs)
	}
	reviewer, ok, err := Find(dir, "backend_reviewer")
	if err != nil || !ok || len(reviewer.MCPs) != 2 || reviewer.MCPs[0].ID != "atlassian_jira" || reviewer.MCPs[1].ID != "atlassian_confluence" || !strings.Contains(reviewer.MCPs[0].Description, "Do not set a ticket to Done.") || !strings.Contains(reviewer.MCPs[1].Description, "when the ticket, the pull request, or the user names it") {
		t.Fatalf("reviewer mcps = %#v %v", reviewer.MCPs, ok)
	}
	if !strings.Contains(front, "ai_tool: cursor\n") || !strings.Contains(front, "model: auto\n") {
		t.Fatalf("designer did not use the default tool and model:\n%s", front)
	}
}

func TestEnsureMovesFlatFileAndLeavesPrompt(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	flat := filepath.Join(dir, "backend_runner.md")
	body := "---\nid: backend_runner\nname: Senior Backend Developer\ndescription: Implements a backend ticket.\nai_tool: cursor\nmodel: auto\nrepos:\n    - backend\nworks_with: backend_reviewer\n---\n\nCustom prompt line.\n"
	if err := os.WriteFile(flat, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(flat); !os.IsNotExist(err) {
		t.Fatal("flat agent file was left at the top of the agent folder")
	}
	moved, err := os.ReadFile(filepath.Join(dir, "backend", "backend_runner.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(moved) != body {
		t.Fatal("moving into the category folder changed the file")
	}
	files, err := List(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 1 || files[0].Category != "backend" || files[0].ID != "backend_runner" {
		t.Fatalf("files = %+v", files)
	}
	if len(files[0].WorksWith) != 1 || files[0].WorksWith[0].ID != "backend_reviewer" || len(files[0].WorksWith[0].Handoffs) != 0 {
		t.Fatalf("works with = %#v", files[0].WorksWith)
	}
	if _, err := os.Stat(filepath.Join(dir, "requirement", "requirement_document.md")); !os.IsNotExist(err) {
		t.Fatal("Ensure created a catalog agent after the folder already had an agent file")
	}
}

func TestSaveRewritesFileAndMovesCategory(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	current, ok, err := Find(dir, "backend_runner")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	next := current
	next.ID = "backend_builder"
	next.Name = "Backend Builder"
	next.Category = "platform"
	next.Enabled = false
	next.AITool = "codex"
	next.Model = "gpt-5.6-sol"
	next.Prompt = current.Prompt + "Custom prompt line.\n"
	next.WorksWith = []Collaborator{{ID: "backend_reviewer"}, {ID: "website_runner"}}
	if err := Save(dir, "backend_runner", next); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, "backend", "backend_runner.md")); !os.IsNotExist(err) {
		t.Fatal("old agent file still exists")
	}
	raw, err := os.ReadFile(filepath.Join(dir, "platform", "backend_builder.md"))
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	for _, want := range []string{
		"id: backend_builder\n",
		"name: Backend Builder\n",
		"enabled: false\n",
		"ai_tool: codex\n",
		"model: gpt-5.6-sol\n",
		"- backend_reviewer\n",
		"- website_runner\n",
		"Custom prompt line.\n",
		"**Merge rights:**",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("missing %q in\n%s", want, text)
		}
	}
	if _, err := os.Stat(filepath.Join(dir, "backend", "backend_reviewer.md")); err != nil {
		t.Fatal(err)
	}
	again := next
	again.Name = "Taken"
	if err := Save(dir, "backend_reviewer", again); !errors.Is(err, ErrExists) {
		t.Fatalf("err = %v", err)
	}
}

func TestSaveKeepsHandoffs(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	current, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	next := current
	next.WorksWith = []Collaborator{{
		ID: "requirement_designer",
		Handoffs: []Handoff{
			{When: "business_requirement.md and product_requirement.md are ready", Send: "Hand over the requirement folder.\nWait for design.html.", Reply: " The design path. ", MaxLoops: 3},
			{When: "  ", Send: "Answer the designer questions.", MaxLoops: 5},
			{When: "", Send: "  "},
		},
	}}
	if err := Save(dir, "requirement_document", next); err != nil {
		t.Fatal(err)
	}
	saved, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if len(saved.WorksWith) != 1 || saved.WorksWith[0].ID != "requirement_designer" {
		t.Fatalf("works with = %#v", saved.WorksWith)
	}
	want := []Handoff{
		{When: "business_requirement.md and product_requirement.md are ready", Send: "Hand over the requirement folder.\nWait for design.html.", Reply: "The design path.", MaxLoops: 3},
		{Send: "Answer the designer questions."},
	}
	if !reflect.DeepEqual(saved.WorksWith[0].Handoffs, want) {
		t.Fatalf("handoffs = %#v", saved.WorksWith[0].Handoffs)
	}
	raw, err := os.ReadFile(saved.Path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	if !strings.Contains(text, "agents_coordination:\n") || !strings.Contains(text, "id: requirement_designer\n") || !strings.Contains(text, "handoffs:\n") || !strings.Contains(text, "when: business_requirement.md") || !strings.Contains(text, "send:") || !strings.Contains(text, "max_loops: 3") || !strings.Contains(text, "reply: The design path.") || strings.Contains(text, "max_loops: 5") || strings.Contains(text, "description: Hand over") || strings.Contains(text, "\nteamwork:") || strings.Contains(text, "works_with:") || strings.Contains(text, "collaboration:") {
		t.Fatalf("front matter = %s", text)
	}
}

func TestSaveKeepsMCPAndToolNotes(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	current, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if current.MCPs != nil || current.Tools != nil {
		t.Fatalf("seeded uses = %#v %#v", current.MCPs, current.Tools)
	}
	next := current
	next.MCPs = []Use{{
		ID:          "atlassian_jira",
		Description: "Read the Jira ticket.\nWrite the comment.",
		BoardID:     "12",
		BoardName:   "KIKI",
	}, {
		ID:          "atlassian_confluence",
		Description: "Read the design page.",
		SpaceID:     "111",
		SpaceKey:    "ENG",
		SpaceName:   "Engineering",
		ParentID:    "99",
		ParentTitle: "Requirements",
	}}
	next.Tools = []Use{{ID: "github"}}
	if err := Save(dir, "requirement_document", next); err != nil {
		t.Fatal(err)
	}
	saved, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if len(saved.MCPs) != 2 || saved.MCPs[0].ID != "atlassian_jira" || saved.MCPs[0].Description != "Read the Jira ticket.\nWrite the comment." || saved.MCPs[0].BoardID != "12" || saved.MCPs[0].BoardName != "KIKI" || saved.MCPs[1].SpaceKey != "ENG" || saved.MCPs[1].ParentTitle != "Requirements" {
		t.Fatalf("mcps = %#v", saved.MCPs)
	}
	if len(saved.Tools) != 1 || saved.Tools[0].ID != "github" || saved.Tools[0].Description != "" {
		t.Fatalf("tools = %#v", saved.Tools)
	}
	raw, err := os.ReadFile(saved.Path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	if !strings.Contains(text, "mcps:\n") || !strings.Contains(text, "id: atlassian_jira\n") || !strings.Contains(text, "board_id: \"12\"\n") || !strings.Contains(text, "id: atlassian_confluence\n") || !strings.Contains(text, "space_key: ENG\n") || !strings.Contains(text, "parent_title: Requirements\n") || !strings.Contains(text, "tools:\n") || !strings.Contains(text, "- github\n") {
		t.Fatalf("front matter = %s", text)
	}
	next.MCPs = nil
	next.Tools = []Use{{ID: "GitHub"}}
	if err := Save(dir, "requirement_document", next); err == nil {
		t.Fatal("tool id with a capital letter was accepted")
	}
}

func TestParseAcceptsLegacyRepoKind(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents", "requirement")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	body := "---\nid: requirement_document\nname: Document Writer\ndescription: Writes files.\nai_tool: cursor\nmodel: auto\nrepos:\n    - kind: document\n      description: Write the requirement here.\n---\n\nPrompt.\n"
	if err := os.WriteFile(filepath.Join(dir, "requirement_document.md"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	file, ok, err := Find(filepath.Dir(dir), "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if len(file.Repos) != 1 || file.Repos[0].Kind != "document" || file.Repos[0].Description != "Write the requirement here." {
		t.Fatalf("repos = %#v", file.Repos)
	}
}

func TestParseAcceptsLegacyTeamworkKeys(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents", "backend")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	body := "---\nid: backend_runner\nname: Senior Backend Developer\ndescription: Implements a backend ticket.\nai_tool: cursor\nmodel: auto\nrepos:\n    - backend\nworks_with:\n    - id: backend_reviewer\n      collaboration: Hand the pull request over.\n---\n\nPrompt.\n"
	if err := os.WriteFile(filepath.Join(dir, "backend_runner.md"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	file, ok, err := Find(filepath.Dir(dir), "backend_runner")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if len(file.WorksWith) != 1 || file.WorksWith[0].ID != "backend_reviewer" || !reflect.DeepEqual(file.WorksWith[0].Handoffs, []Handoff{{Send: "Hand the pull request over."}}) {
		t.Fatalf("works with = %#v", file.WorksWith)
	}
}

func TestParseAcceptsPreviousTeamworkKey(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents", "backend")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	body := "---\nid: backend_runner\nname: Senior Backend Developer\ndescription: Implements a backend ticket.\nai_tool: cursor\nmodel: auto\nrepos:\n    - backend\nteamwork:\n    - id: backend_reviewer\n      description: Hand the pull request over.\n---\n\nPrompt.\n"
	if err := os.WriteFile(filepath.Join(dir, "backend_runner.md"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	file, ok, err := Find(filepath.Dir(dir), "backend_runner")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if len(file.WorksWith) != 1 || file.WorksWith[0].ID != "backend_reviewer" || !reflect.DeepEqual(file.WorksWith[0].Handoffs, []Handoff{{Send: "Hand the pull request over."}}) {
		t.Fatalf("works with = %#v", file.WorksWith)
	}
}

func TestSaveKeepsReadonlyRepository(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	current, ok, err := Find(dir, "requirement_planner")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	next := current
	next.Repos = []Repository{
		{Kind: "document", Description: "Read the notes.", Readonly: true},
		{Kind: "backend", Readonly: true},
		{Kind: "website", Description: "Change the website."},
	}
	if err := Save(dir, current.ID, next); err != nil {
		t.Fatal(err)
	}
	saved, ok, err := Find(dir, current.ID)
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	if len(saved.Repos) != 3 || !saved.Repos[0].Readonly || saved.Repos[0].Description != "Read the notes." || !saved.Repos[1].Readonly || saved.Repos[1].Description != "" || saved.Repos[2].Readonly {
		t.Fatalf("repos = %#v", saved.Repos)
	}
	raw, err := os.ReadFile(saved.Path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	if !strings.Contains(text, "readonly: true") || strings.Count(text, "readonly: true") != 2 {
		t.Fatalf("readonly was not stored on the two chosen repositories:\n%s", text)
	}
}

func TestSaveRejectsConditionWithoutMessage(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	current, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	next := current
	next.WorksWith = []Collaborator{{ID: "requirement_designer", Handoffs: []Handoff{{When: "the files are ready"}}}}
	if err := Save(dir, "requirement_document", next); err == nil {
		t.Fatal("a condition with nothing to send was saved")
	}
}

func TestSaveKeepsRepositoryGuide(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	current, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	next := current
	next.Repos = []Repository{{Kind: "document", Guide: " RULES.md ", Description: "Write the requirement."}}
	if err := Save(dir, "requirement_document", next); err != nil {
		t.Fatal(err)
	}
	saved, _, err := Find(dir, "requirement_document")
	if err != nil || len(saved.Repos) != 1 || saved.Repos[0].Guide != "RULES.md" {
		t.Fatalf("repos = %#v err = %v", saved.Repos, err)
	}
	raw, err := os.ReadFile(saved.Path)
	if err != nil || !strings.Contains(string(raw), "guide: RULES.md") {
		t.Fatalf("front matter = %s err = %v", raw, err)
	}

	next.Repos = []Repository{{Kind: "document", Guide: "AGENTS.md", Description: "Write the requirement."}}
	if err := Save(dir, "requirement_document", next); err != nil {
		t.Fatal(err)
	}
	saved, _, err = Find(dir, "requirement_document")
	if err != nil || saved.Repos[0].Guide != "" {
		t.Fatalf("default guide was stored: %#v err = %v", saved.Repos, err)
	}
	raw, _ = os.ReadFile(saved.Path)
	if strings.Contains(string(raw), "guide:") {
		t.Fatalf("default guide was written:\n%s", raw)
	}

	for _, bad := range []string{"docs/AGENTS.md", "../AGENTS.md", "AGENTS.txt", ".md", "a\\b.md"} {
		next.Repos = []Repository{{Kind: "document", Guide: bad}}
		if err := Save(dir, "requirement_document", next); err == nil {
			t.Fatalf("guide %q was accepted", bad)
		}
	}
}

func TestSaveRejectsLoopLimitOutsideRange(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	current, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	next := current
	for _, limit := range []int{-1, MaxHandoffLoops + 1} {
		next.WorksWith = []Collaborator{{ID: "requirement_designer", Handoffs: []Handoff{{Send: "Design it.", Reply: "The design path.", MaxLoops: limit}}}}
		if err := Save(dir, "requirement_document", next); err == nil {
			t.Fatalf("loop limit %d was saved", limit)
		}
	}
	next.WorksWith = []Collaborator{{ID: "requirement_designer", Handoffs: []Handoff{{Send: "Design it.", Reply: "The design path.", MaxLoops: MaxHandoffLoops}}}}
	if err := Save(dir, "requirement_document", next); err != nil {
		t.Fatal(err)
	}
}

func TestHandoffWithoutReplyDoesNotReportBack(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "agents")
	if err := Ensure(dir, "cursor"); err != nil {
		t.Fatal(err)
	}
	current, ok, err := Find(dir, "requirement_document")
	if err != nil || !ok {
		t.Fatal(err, ok)
	}
	next := current
	next.WorksWith = []Collaborator{{ID: "requirement_designer", Handoffs: []Handoff{{Send: "Design it.", Reply: "   ", MaxLoops: 4}}}}
	if err := Save(dir, "requirement_document", next); err != nil {
		t.Fatal(err)
	}
	saved, _, err := Find(dir, "requirement_document")
	if err != nil {
		t.Fatal(err)
	}
	if got := saved.WorksWith[0].Handoffs; len(got) != 1 || got[0].Reply != "" || got[0].MaxLoops != 0 {
		t.Fatalf("handoffs = %#v", got)
	}
}
