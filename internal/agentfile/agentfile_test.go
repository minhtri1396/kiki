package agentfile

import (
	"errors"
	"os"
	"path/filepath"
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
		"teamwork:\n",
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
	if strings.Contains(frontMatter, "kind:") || strings.Contains(frontMatter, "works_with:") || strings.Contains(frontMatter, "collaboration:") {
		t.Fatalf("seeded front matter still uses the old teamwork keys:\n%s", frontMatter)
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
		if strings.TrimSpace(peer.Collaboration) == "" {
			t.Fatalf("planner peer %s has no collaboration note", peer.ID)
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
	runner, ok, err := Find(dir, "backend_runner")
	if err != nil || !ok || len(runner.WorksWith) != 1 {
		t.Fatal(err, ok)
	}
	if runner.WorksWith[0].ID != "backend_reviewer" || !strings.Contains(runner.WorksWith[0].Collaboration, "[backend_runner -> backend_reviewer]") {
		t.Fatalf("runner collaboration = %#v", runner.WorksWith[0])
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
	if len(files[0].WorksWith) != 1 || files[0].WorksWith[0].ID != "backend_reviewer" || files[0].WorksWith[0].Collaboration != "" {
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

func TestSaveKeepsCollaborationNote(t *testing.T) {
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
		ID:            "requirement_designer",
		Collaboration: "Hand over the requirement folder.\nWait for design.html.",
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
	if saved.WorksWith[0].Collaboration != "Hand over the requirement folder.\nWait for design.html." {
		t.Fatalf("collaboration = %q", saved.WorksWith[0].Collaboration)
	}
	raw, err := os.ReadFile(saved.Path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	if !strings.Contains(text, "id: requirement_designer\n") || !strings.Contains(text, "description:") || strings.Contains(text, "works_with:") || strings.Contains(text, "collaboration:") {
		t.Fatalf("front matter = %s", text)
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
	if len(file.WorksWith) != 1 || file.WorksWith[0].ID != "backend_reviewer" || file.WorksWith[0].Collaboration != "Hand the pull request over." {
		t.Fatalf("works with = %#v", file.WorksWith)
	}
}
