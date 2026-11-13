package console

import "testing"

func TestParseCursorModelsGroups(t *testing.T) {
	text := `
Available models

auto - Auto (default)
composer-2.5 - Composer 2.5
cursor-grok-4.5-high - Grok 4.5
grok-4.7-high - Grok 4.7  High
gpt-5.3-codex - Codex 5.3
claude-opus-4-8-high - Claude Opus 4.8 1M (current)

Tip: use --model <id> to switch.
`
	models := ParseCursorModels(text)
	if len(models) != 6 {
		t.Fatalf("got %d models", len(models))
	}
	want := []Model{
		{ID: "auto", Label: "Auto", Default: true, Group: "Cursor Models"},
		{ID: "composer-2.5", Label: "Composer 2.5", Group: "Cursor Models"},
		{ID: "cursor-grok-4.5-high", Label: "Grok 4.5", Group: "Cursor Models"},
		{ID: "grok-4.7-high", Label: "Grok 4.7  High", Group: "Cursor Models"},
		{ID: "gpt-5.3-codex", Label: "Codex 5.3", Group: "Other Models"},
		{ID: "claude-opus-4-8-high", Label: "Claude Opus 4.8 1M", Group: "Other Models"},
	}
	for i, model := range want {
		if models[i] != model {
			t.Fatalf("model %d: got %+v want %+v", i, models[i], model)
		}
	}
}

func TestParseClaudeModelDoc(t *testing.T) {
	text := `
| Provider | ` + "`opus`" + ` | ` + "`sonnet`" + ` | ` + "`haiku`" + ` |
| Anthropic API | Opus 5.5 | Sonnet 5.5 | Haiku 5.5 |
| Microsoft Foundry | Opus 4.6 | Sonnet 4.5 | Haiku 4.5 |

Unless you set it, the fable alias resolves to Fable 5 in gateway sessions and to Fable 5.2 everywhere else.
`
	models := ParseClaudeModelDoc(text)
	if len(models) != 4 {
		t.Fatalf("got %d models", len(models))
	}
	want := []Model{
		{ID: "fable", Label: "Fable 5.2"},
		{ID: "opus", Label: "Opus 5.5"},
		{ID: "sonnet", Label: "Sonnet 5.5", Default: true},
		{ID: "haiku", Label: "Haiku 5.5"},
	}
	for i, model := range want {
		if models[i] != model {
			t.Fatalf("model %d: got %+v want %+v", i, models[i], model)
		}
	}
}

func TestParseClaudeModelDocIgnoresOtherRows(t *testing.T) {
	text := "| Amazon Bedrock | Opus 5.5 | Sonnet 4.5 | Haiku 4.5 |\n"
	if models := ParseClaudeModelDoc(text); models != nil {
		t.Fatalf("expected no models, got %+v", models)
	}
}

func TestInstalledAgents(t *testing.T) {
	previous := commandExists
	t.Cleanup(func() { commandExists = previous })
	commandExists = func(name string) bool {
		return name == "claude" || name == "cursor-agent"
	}
	got := InstalledAgents()
	want := []string{"cursor", "claude"}
	if len(got) != len(want) {
		t.Fatalf("got %v", got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("got %v want %v", got, want)
		}
	}
}
