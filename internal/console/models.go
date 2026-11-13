package console

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"os/exec"
	"regexp"
	"strings"
	"time"
)

// Model is one provider model for an AI tool.
// Group is the dropdown section, used for Cursor's Cursor Models and Other Models.
type Model struct {
	ID      string
	Label   string
	Default bool
	Group   string
}

// ToolCommands are the CLIs Orca looks up for each AI tool.
var ToolCommands = map[string]string{
	"cursor": "cursor-agent",
	"claude": "claude",
	"codex":  "codex",
	"gemini": "gemini",
}

// commandExists reports whether a CLI is on PATH. Tests replace it.
var commandExists = func(name string) bool {
	_, err := exec.LookPath(name)
	return err == nil
}

// InstalledAgents is OrcaAgents limited to tools whose CLI is on PATH.
func InstalledAgents() []string {
	out := make([]string, 0, len(OrcaAgents))
	for _, name := range OrcaAgents {
		bin := ToolCommands[name]
		if bin != "" && commandExists(bin) {
			out = append(out, name)
		}
	}
	return out
}

// ModelCatalog is the model list Orca uses for each AI tool.
// Cursor is a short fallback. The running app replaces it with `cursor-agent models` when that command succeeds.
// Claude labels are the versions those aliases resolve to on the Anthropic API.
// The running app replaces them with the current Claude Code model config when that page can be read.
// The id stays the alias, which is what `orca agent launch --model` accepts.
func ModelCatalog() map[string][]Model {
	return map[string][]Model{
		"cursor": {
			{ID: "auto", Label: "Auto", Default: true, Group: cursorModelsGroup},
			{ID: "composer-2.5", Label: "Composer 2.5", Group: cursorModelsGroup},
			{ID: "gpt-5.3-codex", Label: "GPT-5.3 Codex", Group: otherModelsGroup},
			{ID: "claude-opus-4-8", Label: "Claude Opus 4.8", Group: otherModelsGroup},
		},
		"claude": {
			{ID: "fable", Label: "Fable 5.2"},
			{ID: "opus", Label: "Opus 5.5"},
			{ID: "sonnet", Label: "Sonnet 5.5", Default: true},
			{ID: "haiku", Label: "Haiku 5.5"},
		},
		"codex": {
			{ID: "gpt-5.6-sol", Label: "GPT-5.6 Sol", Default: true},
			{ID: "gpt-5.6-terra", Label: "GPT-5.6 Terra"},
			{ID: "gpt-5.6-luna", Label: "GPT-5.6 Luna"},
			{ID: "gpt-5.5", Label: "GPT-5.5"},
			{ID: "gpt-5.2-codex", Label: "GPT-5.2 Codex"},
		},
		"gemini": {
			{ID: "gemini-3-pro-preview", Label: "Gemini 3 Pro Preview", Default: true},
			{ID: "gemini-3-flash-preview", Label: "Gemini 3 Flash Preview"},
			{ID: "gemini-2.5-pro", Label: "Gemini 2.5 Pro"},
			{ID: "gemini-2.5-flash", Label: "Gemini 2.5 Flash"},
		},
	}
}

// ResolveModel checks a model id for an AI tool. An empty model uses that tool's default.
func ResolveModel(catalog map[string][]Model, tool, model string) (string, error) {
	tool = strings.TrimSpace(tool)
	model = strings.TrimSpace(model)
	if !contains(OrcaAgents, tool) {
		return "", fmt.Errorf("AI Tool is not supported")
	}
	models := catalog[tool]
	if model == "" {
		for _, item := range models {
			if item.Default {
				return item.ID, nil
			}
		}
		if len(models) > 0 {
			return models[0].ID, nil
		}
		return "", fmt.Errorf("model is not supported")
	}
	for _, item := range models {
		if item.ID == model {
			return item.ID, nil
		}
	}
	return "", fmt.Errorf("model is not supported")
}

// LiveModels asks the AI tool's own CLI for the models it can launch.
// Claude has no model-list command, so that tool uses the Claude Code model config.
func LiveModels(ctx context.Context, tool string) []Model {
	switch strings.TrimSpace(tool) {
	case "cursor":
		return CursorModels(ctx)
	case "claude":
		return ClaudeModels(ctx)
	case "codex":
		return commandModels(ctx, "codex", "models")
	case "gemini":
		return commandModels(ctx, "gemini", "models")
	default:
		return nil
	}
}

func commandModels(ctx context.Context, bin string, args ...string) []Model {
	if !commandExists(bin) {
		return nil
	}
	if _, ok := ctx.Deadline(); !ok {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, 15*time.Second)
		defer cancel()
	}
	out, err := exec.CommandContext(ctx, bin, args...).Output()
	if err != nil {
		return nil
	}
	return ParseCursorModels(string(out))
}

// CursorModels asks the local Cursor CLI which models it can launch.
func CursorModels(ctx context.Context) []Model {
	if _, ok := ctx.Deadline(); !ok {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, 15*time.Second)
		defer cancel()
	}
	cmd := exec.CommandContext(ctx, "cursor-agent", "models")
	out, err := cmd.Output()
	if err != nil {
		return nil
	}
	return ParseCursorModels(string(out))
}

// ParseCursorModels reads `cursor-agent models` text.
func ParseCursorModels(text string) []Model {
	var models []Model
	seen := map[string]bool{}
	var defaultID string
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSpace(line)
		id, label, ok := strings.Cut(line, " - ")
		if !ok {
			continue
		}
		id = strings.TrimSpace(id)
		label = strings.TrimSpace(label)
		if id == "" || strings.ContainsAny(id, " \t") || seen[id] {
			continue
		}
		if strings.Contains(strings.ToLower(label), "default") {
			defaultID = id
		}
		if cut, _, found := strings.Cut(label, "("); found {
			label = strings.TrimSpace(cut)
		}
		if label == "" {
			label = id
		}
		seen[id] = true
		models = append(models, Model{ID: id, Label: label, Group: cursorModelGroup(id)})
	}
	if len(models) == 0 {
		return nil
	}
	if defaultID == "" {
		defaultID = models[0].ID
	}
	for i := range models {
		models[i].Default = models[i].ID == defaultID
	}
	return models
}

const (
	cursorModelsGroup = "Cursor Models"
	otherModelsGroup  = "Other Models"
)

// cursorModelGroup matches the Cursor picker: Auto, Composer, and Grok
// are Cursor Models. Named provider models are Other Models.
// Grok 4.5 and 4.6 use a cursor-grok- id. Grok 4.7 uses grok-4.7-.
func cursorModelGroup(id string) string {
	id = strings.ToLower(strings.TrimSpace(id))
	if id == "auto" || strings.HasPrefix(id, "composer-") || strings.HasPrefix(id, "cursor-") || strings.HasPrefix(id, "grok-") {
		return cursorModelsGroup
	}
	return otherModelsGroup
}

const claudeModelDocURL = "https://code.claude.com/docs/en/model-config.md"

var (
	claudeFamily = regexp.MustCompile(`^(Opus|Sonnet|Haiku) \d+(?:\.\d+)?$`)
	claudeFable  = regexp.MustCompile(`to (Fable \d+(?:\.\d+)?) everywhere else`)
	claudeLink   = regexp.MustCompile(`\[([^\]]+)\]\([^)]*\)`)
)

// ClaudeModels reads the current alias versions from the Claude Code model config.
// A network or parse failure returns nil so the caller keeps ModelCatalog.
func ClaudeModels(ctx context.Context) []Model {
	if _, ok := ctx.Deadline(); !ok {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, 4*time.Second)
		defer cancel()
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, claudeModelDocURL, nil)
	if err != nil {
		return nil
	}
	req.Header.Set("Accept", "text/plain, text/markdown")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil
	}
	body, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return nil
	}
	return ParseClaudeModelDoc(string(body))
}

// ParseClaudeModelDoc reads the Anthropic API alias row and the Fable resolution
// from the Claude Code model config markdown.
func ParseClaudeModelDoc(text string) []Model {
	var opus, sonnet, haiku string
	found := false
	for _, line := range strings.Split(text, "\n") {
		if !strings.Contains(line, "Anthropic API") || !strings.Contains(line, "|") {
			continue
		}
		cells := tableCells(line)
		if len(cells) < 4 || !strings.Contains(cells[0], "Anthropic API") {
			continue
		}
		if !claudeFamily.MatchString(cells[1]) || !claudeFamily.MatchString(cells[2]) || !claudeFamily.MatchString(cells[3]) {
			continue
		}
		opus, sonnet, haiku = cells[1], cells[2], cells[3]
		found = true
		break
	}
	if !found {
		return nil
	}
	fable := "Fable 5.2"
	if match := claudeFable.FindStringSubmatch(text); len(match) == 2 {
		fable = match[1]
	}
	return []Model{
		{ID: "fable", Label: fable},
		{ID: "opus", Label: opus},
		{ID: "sonnet", Label: sonnet, Default: true},
		{ID: "haiku", Label: haiku},
	}
}

func tableCells(line string) []string {
	parts := strings.Split(line, "|")
	cells := make([]string, 0, len(parts))
	for _, part := range parts {
		cell := cleanCell(part)
		if cell == "" {
			continue
		}
		cells = append(cells, cell)
	}
	return cells
}

func cleanCell(value string) string {
	value = strings.TrimSpace(value)
	value = strings.ReplaceAll(value, "**", "")
	value = strings.ReplaceAll(value, "`", "")
	value = claudeLink.ReplaceAllString(value, "$1")
	return strings.TrimSpace(value)
}
