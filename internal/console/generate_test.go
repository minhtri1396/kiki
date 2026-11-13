package console

import (
	"context"
	"errors"
	"strings"
	"testing"
)

func TestRewritePromptKinds(t *testing.T) {
	prompt, err := RewritePrompt("prompt", "requirement_document", "  Write the docs.  ")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(prompt, "prompt for agent requirement_document") || !strings.Contains(prompt, "Rewrite the text below in English.") || !strings.HasSuffix(prompt, "Write the docs.") {
		t.Fatalf("prompt = %q", prompt)
	}
	repo, err := RewritePrompt("repo", "document", "Keep the vault.")
	if err != nil || !strings.Contains(repo, "document repository") {
		t.Fatalf("repo = %q err = %v", repo, err)
	}
	note, err := RewritePrompt("teamwork", "requirement_designer", "Hand over the folder.")
	if err != nil || !strings.Contains(note, "requirement_designer") {
		t.Fatalf("note = %q err = %v", note, err)
	}
	if _, err := RewritePrompt("prompt", "", "   "); err == nil {
		t.Fatal("empty text was accepted")
	}
	if _, err := RewritePrompt("other", "", "text"); err == nil {
		t.Fatal("unknown kind was accepted")
	}
}

func TestPreviewCommandLabelsThePrompt(t *testing.T) {
	command, err := PreviewCommand("cursor", "auto", "repo", "backend", "Ship the API.")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(command, "cursor-agent --print") || !strings.Contains(command, "--workspace {{workspace}} {{prompt}}") || strings.Contains(command, "Ship the API.") {
		t.Fatalf("command = %q", command)
	}
}

func TestGenerateArgsUsesPrintMode(t *testing.T) {
	cursor, args, err := GenerateArgs("cursor", "composer-2.5", "Rewrite this.")
	if err != nil {
		t.Fatal(err)
	}
	if cursor != "cursor-agent" || !containsArg(args, "--print") || !containsArg(args, "--mode") || args[len(args)-1] != "Rewrite this." {
		t.Fatalf("cursor = %s %v", cursor, args)
	}
	if !containsPair(args, "--model", "composer-2.5") {
		t.Fatalf("model missing: %v", args)
	}
	claude, claudeArgs, err := GenerateArgs("claude", "sonnet", "Rewrite this.")
	if err != nil || claude != "claude" || !containsArg(claudeArgs, "--print") || !containsArg(claudeArgs, "--bare") {
		t.Fatalf("claude = %s %v err = %v", claude, claudeArgs, err)
	}
	codex, codexArgs, err := GenerateArgs("codex", "gpt-5.6-sol", "Rewrite this.")
	if err != nil || codex != "codex" || codexArgs[0] != "exec" || !containsArg(codexArgs, "--sandbox") {
		t.Fatalf("codex = %s %v err = %v", codex, codexArgs, err)
	}
	gemini, geminiArgs, err := GenerateArgs("gemini", "gemini-2.5-flash", "Rewrite this.")
	if err != nil || gemini != "gemini" || !containsPair(geminiArgs, "--prompt", "Rewrite this.") || !containsPair(geminiArgs, "--model", "gemini-2.5-flash") {
		t.Fatalf("gemini = %s %v err = %v", gemini, geminiArgs, err)
	}
	if _, _, err := GenerateArgs("cursor", "bad model", "text"); err == nil {
		t.Fatal("model with a space was accepted")
	}
}

func TestGenerateStreamLogsThenDiff(t *testing.T) {
	previous := startGenerateCommand
	previousExists := commandExists
	t.Cleanup(func() {
		startGenerateCommand = previous
		commandExists = previousExists
	})
	commandExists = func(name string) bool { return name == "cursor-agent" }
	var gotDir, gotBin string
	var gotArgs []string
	stdout := strings.Join([]string{
		`{"type":"thinking","subtype":"delta","text":"Checking"}`,
		`{"type":"thinking","subtype":"completed"}`,
		`{"type":"assistant","message":{"content":[{"type":"text","text":"Clearer"}]}}`,
		`{"type":"assistant","message":{"content":[{"type":"text","text":" text."}]}}`,
		`{"type":"assistant","message":{"content":[{"type":"text","text":"Clearer text."}]}}`,
		`{"type":"result","subtype":"success","result":"Clearer text."}`,
	}, "\n")
	startGenerateCommand = func(ctx context.Context, dir, bin string, args ...string) (commandRun, error) {
		gotDir, gotBin, gotArgs = dir, bin, append([]string(nil), args...)
		return commandRun{
			Stdout: strings.NewReader(stdout),
			Stderr: strings.NewReader("warning\n"),
			Wait:   func() error { return nil },
		}, nil
	}
	var logs []string
	var done GenerateEvent
	var command string
	err := GenerateStream(context.Background(), "cursor", "auto", "repo", "backend", "Ship the API.", func(ev GenerateEvent) error {
		if ev.Type == "command" {
			command = ev.Command
		}
		if ev.Type == "log" {
			logs = append(logs, ev.Channel+":"+ev.Text)
		}
		if ev.Type == "done" {
			done = ev
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if gotBin != "cursor-agent" || gotDir == "" || !containsPair(gotArgs, "--workspace", gotDir) {
		t.Fatalf("ran %s in %s with %v", gotBin, gotDir, gotArgs)
	}
	if !strings.Contains(command, "cursor-agent") || !strings.Contains(command, "--workspace "+gotDir) || !strings.Contains(command, "{{prompt}}") || strings.Contains(command, "Ship the API.") {
		t.Fatalf("command = %q", command)
	}
	joined := strings.Join(logs, "")
	if !strings.Contains(joined, "think:Checking") || !strings.Contains(joined, "text:Clearer") || !strings.Contains(joined, "text: text.") || !strings.Contains(joined, "text:warning") {
		t.Fatalf("logs = %#v", logs)
	}
	if strings.Contains(joined, "text:Clearer text.") {
		t.Fatal("the repeated full message was logged again")
	}
	if done.Text != "Clearer text." {
		t.Fatalf("text = %q", done.Text)
	}
	var kinds []string
	for _, line := range done.Diff {
		kinds = append(kinds, line.Kind+":"+line.Text)
	}
	foundDel, foundAdd := false, false
	for _, line := range done.Diff {
		if line.Kind == "del" && line.Text == "Ship the API." {
			foundDel = true
		}
		if line.Kind == "add" && line.Text == "Clearer text." {
			foundAdd = true
		}
	}
	if !foundDel || !foundAdd {
		t.Fatalf("diff = %v", kinds)
	}
}

func TestGenerateStreamShowsThinkingDeltas(t *testing.T) {
	previous := startGenerateCommand
	previousExists := commandExists
	t.Cleanup(func() {
		startGenerateCommand = previous
		commandExists = previousExists
	})
	commandExists = func(string) bool { return true }
	stdout := strings.Join([]string{
		`{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"Look at "}}}`,
		`{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"the wording."}}}`,
		`{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Release the API."}}}`,
		`{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"Look at the wording."},{"type":"text","text":"Release the API."}]}}`,
		`{"type":"result","subtype":"success","result":"Release the API."}`,
	}, "\n")
	startGenerateCommand = func(ctx context.Context, dir, bin string, args ...string) (commandRun, error) {
		return commandRun{Stdout: strings.NewReader(stdout), Stderr: strings.NewReader(""), Wait: func() error { return nil }}, nil
	}
	var logs []string
	err := GenerateStream(context.Background(), "claude", "sonnet", "prompt", "planner", "Ship the API.", func(ev GenerateEvent) error {
		if ev.Type == "log" {
			logs = append(logs, ev.Channel+":"+ev.Text)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	joined := strings.Join(logs, "|")
	if joined != "think:Look at |think:the wording.|text:Release the API." {
		t.Fatalf("logs = %q", joined)
	}
}

func TestTextDiffHunk(t *testing.T) {
	diff := TextDiff("alpha\nbeta\ngamma\n", "alpha\nBETA\ngamma\ndelta\n")
	if len(diff) == 0 || diff[0].Kind != "hunk" || !strings.HasPrefix(diff[0].Text, "@@ ") {
		t.Fatalf("diff = %#v", diff)
	}
	got := make([]string, 0, len(diff))
	for _, line := range diff[1:] {
		got = append(got, line.Kind+":"+line.Text)
	}
	want := []string{"same:alpha", "del:beta", "add:BETA", "same:gamma", "add:delta"}
	if strings.Join(got, "|") != strings.Join(want, "|") {
		t.Fatalf("rows = %v", got)
	}
	if TextDiff("same\n", "same\n") != nil {
		t.Fatal("identical text produced a diff")
	}
}

func TestGenerateTextReportsMissingCLI(t *testing.T) {
	previous := commandExists
	t.Cleanup(func() { commandExists = previous })
	commandExists = func(string) bool { return false }
	_, err := GenerateText(context.Background(), "claude", "sonnet", "prompt", "planner", "Plan the work.")
	if err == nil || !strings.Contains(err.Error(), "claude is not installed") {
		t.Fatalf("err = %v", err)
	}
	var input InputError
	if errors.As(err, &input) {
		t.Fatal("a missing CLI was treated as bad input")
	}
}

func containsArg(args []string, want string) bool {
	for _, arg := range args {
		if arg == want {
			return true
		}
	}
	return false
}

func containsPair(args []string, name, value string) bool {
	for i := 0; i+1 < len(args); i++ {
		if args[i] == name && args[i+1] == value {
			return true
		}
	}
	return false
}
