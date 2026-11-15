package guard

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

const marker = " guardrail "

var workspacePattern = regexp.MustCompile(`^[a-z][a-z0-9_-]{0,31}$`)

// Endpoint is one running KIKI the hook can ask.
type Endpoint struct {
	URL       string   `json:"url"`
	Workspace string   `json:"workspace"`
	Paths     []string `json:"paths"`
}

// ListenURL is the local address hooks call.
func ListenURL(addr string) string {
	addr = strings.TrimSpace(addr)
	if strings.HasPrefix(addr, ":") {
		addr = "127.0.0.1" + addr
	}
	return "http://" + strings.TrimRight(addr, "/")
}

// HookCommand is the shell command that runs this app's guardrail, without the tool name.
func HookCommand(exe string) string {
	exe = strings.TrimSpace(exe)
	if exe == "" {
		exe = "kiki"
	}
	return strconv.Quote(exe) + " guardrail"
}

// Publish writes the address and repository folders for one workspace.
func Publish(home string, ep Endpoint) error {
	ep.Workspace = strings.TrimSpace(ep.Workspace)
	if ep.Workspace == "" {
		ep.Workspace = "kiki"
	}
	if !workspacePattern.MatchString(ep.Workspace) {
		return fmt.Errorf("workspace is not valid")
	}
	ep.URL = strings.TrimRight(strings.TrimSpace(ep.URL), "/")
	if ep.URL == "" {
		return fmt.Errorf("guardrail address is not set")
	}
	ep.Paths = cleanList(ep.Paths)
	raw, err := json.MarshalIndent(ep, "", "  ")
	if err != nil {
		return err
	}
	return writeHook(filepath.Join(home, ".kiki", "guardrails", ep.Workspace+".json"), raw)
}

// Load reads the workspaces this machine has published.
func Load(home string) ([]Endpoint, error) {
	dir := filepath.Join(home, ".kiki", "guardrails")
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []Endpoint
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		raw, err := os.ReadFile(filepath.Join(dir, entry.Name()))
		if err != nil {
			return nil, err
		}
		var ep Endpoint
		if err := json.Unmarshal(raw, &ep); err != nil {
			continue
		}
		if strings.TrimSpace(ep.URL) == "" {
			continue
		}
		out = append(out, ep)
	}
	return out, nil
}

// Remove deletes the guardrail hooks from Cursor, Claude, Codex, and Gemini.
// Hooks already configured for something else are left in place.
func Remove(home string) error {
	var errs []string
	for _, tool := range []string{"cursor", "claude", "codex", "gemini"} {
		if err := RemoveTool(home, tool); err != nil {
			errs = append(errs, toolTitle(tool)+": "+err.Error())
		}
	}
	if len(errs) > 0 {
		return errors.New(strings.Join(errs, "; "))
	}
	return nil
}

// RemoveTool deletes the guardrail hooks for one AI tool.
func RemoveTool(home, tool string) error {
	_, rel, grouped, err := hookCoverage(tool)
	if err != nil {
		return err
	}
	path := filepath.Join(home, rel)
	raw, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if len(bytes.TrimSpace(raw)) == 0 {
		return nil
	}
	var root map[string]any
	if err := json.Unmarshal(raw, &root); err != nil {
		return err
	}
	hooks, _ := root["hooks"].(map[string]any)
	if root["hooks"] != nil && hooks == nil {
		return fmt.Errorf("hooks are not an object")
	}
	if hooks == nil {
		return nil
	}
	changed := false
	for event, value := range hooks {
		var next []any
		if grouped {
			next, err = dropGrouped(value)
		} else {
			next, err = dropFlat(value)
		}
		if err != nil {
			return fmt.Errorf("%s: %w", event, err)
		}
		before, _ := json.Marshal(value)
		after, _ := json.Marshal(next)
		if !bytes.Equal(before, after) {
			changed = true
		}
		if len(next) == 0 {
			delete(hooks, event)
		} else {
			hooks[event] = next
		}
	}
	if !changed {
		return nil
	}
	raw, err = json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	return writeHook(path, raw)
}

// Install writes command hooks for Cursor, Claude, Codex, and Gemini.
// Hooks already configured for something else are left in place.
func Install(home, exe string) error {
	var errs []string
	for _, tool := range []string{"cursor", "claude", "codex", "gemini"} {
		if err := InstallTool(home, exe, tool); err != nil {
			errs = append(errs, toolTitle(tool)+": "+err.Error())
		}
	}
	if len(errs) > 0 {
		return errors.New(strings.Join(errs, "; "))
	}
	return nil
}

// InstallTool writes the guardrail hooks for one AI tool.
// The hooks run before input is sent to the model and after the model output.
func InstallTool(home, exe, tool string) error {
	command := HookCommand(exe) + " " + tool
	switch tool {
	case "cursor":
		return installCursor(filepath.Join(home, ".cursor", "hooks.json"), command)
	case "claude":
		return installGrouped(filepath.Join(home, ".claude", "settings.json"), claudeEvents, command, 20, "")
	case "codex":
		return installGrouped(filepath.Join(home, ".codex", "hooks.json"), codexEvents, command, 20, "")
	case "gemini":
		return installGrouped(filepath.Join(home, ".gemini", "settings.json"), geminiEvents, command, 20000, "kiki-guardrail")
	default:
		return fmt.Errorf("AI Tool is not supported")
	}
}

// HooksReady reports that this AI tool has every guardrail hook KIKI installs.
// Those hooks run before input is sent to the model and after the model output.
func HooksReady(home, tool string) error {
	events, rel, grouped, err := hookCoverage(tool)
	if err != nil {
		return err
	}
	raw, err := os.ReadFile(filepath.Join(home, rel))
	if err != nil {
		return fmt.Errorf("%s guardrail hooks are not installed", toolTitle(tool))
	}
	var root map[string]any
	if err := json.Unmarshal(raw, &root); err != nil {
		return fmt.Errorf("%s guardrail hooks are not installed", toolTitle(tool))
	}
	hooks, _ := root["hooks"].(map[string]any)
	suffix := marker + tool
	for _, event := range events {
		if !eventHasCommand(hooks[event], suffix, grouped) {
			return fmt.Errorf("%s guardrail hook %s is missing", toolTitle(tool), event)
		}
	}
	return nil
}

func hookCoverage(tool string) (events []string, rel string, grouped bool, err error) {
	switch tool {
	case "cursor":
		return cursorEvents, filepath.Join(".cursor", "hooks.json"), false, nil
	case "claude":
		return claudeEvents, filepath.Join(".claude", "settings.json"), true, nil
	case "codex":
		return codexEvents, filepath.Join(".codex", "hooks.json"), true, nil
	case "gemini":
		return geminiEvents, filepath.Join(".gemini", "settings.json"), true, nil
	default:
		return nil, "", false, fmt.Errorf("AI Tool is not supported")
	}
}

func eventHasCommand(value any, suffix string, grouped bool) bool {
	list, ok := value.([]any)
	if !ok {
		return false
	}
	if !grouped {
		for _, item := range list {
			obj, _ := item.(map[string]any)
			cmd, _ := obj["command"].(string)
			if strings.Contains(cmd, suffix) {
				return true
			}
		}
		return false
	}
	for _, item := range list {
		obj, _ := item.(map[string]any)
		inner, _ := obj["hooks"].([]any)
		for _, hook := range inner {
			row, _ := hook.(map[string]any)
			cmd, _ := row["command"].(string)
			if strings.Contains(cmd, suffix) {
				return true
			}
		}
	}
	return false
}

func toolTitle(tool string) string {
	switch tool {
	case "cursor":
		return "Cursor"
	case "claude":
		return "Claude"
	case "codex":
		return "Codex"
	case "gemini":
		return "Gemini"
	default:
		return tool
	}
}

var cursorEvents = []string{
	"beforeSubmitPrompt",
	"preToolUse",
	"postToolUse",
	"beforeShellExecution",
	"afterShellExecution",
	"beforeMCPExecution",
	"afterMCPExecution",
	"beforeReadFile",
	"afterFileEdit",
	"subagentStart",
	"afterAgentResponse",
}

var claudeEvents = []string{"UserPromptSubmit", "PreToolUse", "PostToolUse", "SubagentStart"}

var codexEvents = []string{"UserPromptSubmit", "PreToolUse", "PostToolUse", "SubagentStart"}

var geminiEvents = []string{"BeforeAgent", "BeforeModel", "AfterModel", "BeforeTool", "AfterTool"}

func installCursor(path, command string) error {
	root := map[string]any{}
	existing, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	if len(bytes.TrimSpace(existing)) > 0 {
		if err := json.Unmarshal(existing, &root); err != nil {
			return err
		}
	}
	if _, ok := root["version"]; !ok {
		root["version"] = 1
	}
	hooks, _ := root["hooks"].(map[string]any)
	if root["hooks"] != nil && hooks == nil {
		return fmt.Errorf("hooks are not an object")
	}
	if hooks == nil {
		hooks = map[string]any{}
	}
	for _, event := range cursorEvents {
		list, err := dropFlat(hooks[event])
		if err != nil {
			return fmt.Errorf("%s: %w", event, err)
		}
		hooks[event] = append(list, map[string]any{
			"command":    command,
			"failClosed": false,
			"timeout":    20,
		})
	}
	root["hooks"] = hooks
	raw, err := json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	return writeHook(path, raw)
}

func installGrouped(path string, events []string, command string, timeout any, name string) error {
	root := map[string]any{}
	existing, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	if len(bytes.TrimSpace(existing)) > 0 {
		if err := json.Unmarshal(existing, &root); err != nil {
			return err
		}
	}
	hooks, _ := root["hooks"].(map[string]any)
	if root["hooks"] != nil && hooks == nil {
		return fmt.Errorf("hooks are not an object")
	}
	if hooks == nil {
		hooks = map[string]any{}
	}
	for _, event := range events {
		list, err := dropGrouped(hooks[event])
		if err != nil {
			return fmt.Errorf("%s: %w", event, err)
		}
		hook := map[string]any{
			"type":    "command",
			"command": command,
			"timeout": timeout,
		}
		if name != "" {
			hook["name"] = name
		}
		hooks[event] = append(list, map[string]any{"hooks": []any{hook}})
	}
	root["hooks"] = hooks
	raw, err := json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	return writeHook(path, raw)
}

func dropFlat(value any) ([]any, error) {
	if value == nil {
		return nil, nil
	}
	list, ok := value.([]any)
	if !ok {
		return nil, fmt.Errorf("hooks are not a list")
	}
	var out []any
	for _, item := range list {
		obj, ok := item.(map[string]any)
		if !ok {
			out = append(out, item)
			continue
		}
		cmd, _ := obj["command"].(string)
		if strings.Contains(cmd, marker) {
			continue
		}
		out = append(out, obj)
	}
	return out, nil
}

func dropGrouped(value any) ([]any, error) {
	if value == nil {
		return nil, nil
	}
	list, ok := value.([]any)
	if !ok {
		return nil, fmt.Errorf("hooks are not a list")
	}
	var out []any
	for _, item := range list {
		obj, ok := item.(map[string]any)
		if !ok {
			out = append(out, item)
			continue
		}
		inner, ok := obj["hooks"].([]any)
		if !ok {
			out = append(out, obj)
			continue
		}
		var kept []any
		for _, hook := range inner {
			row, ok := hook.(map[string]any)
			if !ok {
				kept = append(kept, hook)
				continue
			}
			cmd, _ := row["command"].(string)
			if strings.Contains(cmd, marker) {
				continue
			}
			kept = append(kept, row)
		}
		if len(kept) == 0 {
			continue
		}
		obj["hooks"] = kept
		out = append(out, obj)
	}
	return out, nil
}

func cleanList(paths []string) []string {
	seen := map[string]bool{}
	var out []string
	for _, path := range paths {
		path = absPath(path)
		if path == "" || seen[path] {
			continue
		}
		seen[path] = true
		out = append(out, path)
	}
	return out
}

func writeHook(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	data = append(bytes.TrimSpace(data), '\n')
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}
