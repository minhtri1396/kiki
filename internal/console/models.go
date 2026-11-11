package console

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
	"time"
)

// Model is one provider model for an AI tool.
type Model struct {
	ID      string
	Label   string
	Default bool
}

// ModelCatalog is the model list Orca uses for each AI tool.
// Cursor is a short fallback. The running app replaces it with `cursor-agent models` when that command succeeds.
func ModelCatalog() map[string][]Model {
	return map[string][]Model{
		"cursor": {
			{ID: "auto", Label: "Auto", Default: true},
			{ID: "gpt-5.3-codex", Label: "GPT-5.3 Codex"},
			{ID: "claude-opus-4-8", Label: "Claude Opus 4.8"},
		},
		// Labels are the versions Claude Code maps those aliases to.
		// The id stays the alias, which is what `orca agent launch --model` accepts.
		"claude": {
			{ID: "fable", Label: "Fable"},
			{ID: "opus", Label: "Opus 4.6"},
			{ID: "sonnet", Label: "Sonnet 4.6", Default: true},
			{ID: "haiku", Label: "Haiku 4.5"},
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

// CursorModels asks the local Cursor CLI which models it can launch.
func CursorModels(ctx context.Context) []Model {
	if _, ok := ctx.Deadline(); !ok {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, 4*time.Second)
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
		models = append(models, Model{ID: id, Label: label})
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
