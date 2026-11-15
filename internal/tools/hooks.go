package tools

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// runHook runs rtk. Tests replace it.
var runHook = func(home string, args ...string) ([]byte, error) {
	cmd := exec.Command("rtk", args...)
	cmd.Env = hookEnv(home)
	return cmd.CombinedOutput()
}

// SyncHooks registers or removes the RTK command hook for every AI tool.
// The hook rewrites a shell command before it runs. The launch prompt does not mention RTK.
// Turning RTK off removes that hook from every AI tool this app supports.
func SyncHooks(home string, on bool) error {
	home = strings.TrimSpace(home)
	if home == "" {
		return fmt.Errorf("home directory is not set")
	}
	if on {
		if !commandExists("rtk") {
			return fmt.Errorf("rtk is not installed")
		}
		return runSteps(home, installHookSteps())
	}
	if commandExists("rtk") {
		_ = runSteps(home, removeHookSteps())
	}
	return RemoveHooks(home)
}

// EnsureHook registers the RTK command hook for one AI tool.
func EnsureHook(home, tool string) error {
	home = strings.TrimSpace(home)
	tool = strings.TrimSpace(tool)
	if home == "" {
		return fmt.Errorf("home directory is not set")
	}
	steps, err := installStepsFor(tool)
	if err != nil {
		return err
	}
	if !commandExists("rtk") {
		return fmt.Errorf("rtk is not installed")
	}
	return runSteps(home, steps)
}

// RemoveHooks deletes RTK command hooks from every AI tool this app supports.
// Other hooks in those files stay in place.
func RemoveHooks(home string) error {
	home = strings.TrimSpace(home)
	if home == "" {
		return fmt.Errorf("home directory is not set")
	}
	var errs []string
	for _, tool := range []string{"cursor", "claude", "codex", "gemini"} {
		if err := removeToolHook(home, tool); err != nil {
			errs = append(errs, err.Error())
		}
	}
	if len(errs) == 0 {
		return nil
	}
	return fmt.Errorf("RTK hook: %s", strings.Join(errs, "; "))
}

func runSteps(home string, steps [][]string) error {
	var errs []string
	for _, args := range steps {
		out, err := runHook(home, args...)
		if err != nil {
			errs = append(errs, strings.Join(args, " ")+": "+hookOutput(out, err))
		}
	}
	if len(errs) == 0 {
		return nil
	}
	return fmt.Errorf("RTK hook: %s", strings.Join(errs, "; "))
}

func installStepsFor(tool string) ([][]string, error) {
	switch tool {
	case "claude":
		return [][]string{{"init", "--global", "--hook-only", "--auto-patch"}}, nil
	case "cursor":
		return [][]string{{"init", "--global", "--agent", "cursor", "--hook-only", "--auto-patch"}}, nil
	case "gemini":
		return [][]string{{"init", "--global", "--gemini", "--hook-only", "--auto-patch"}}, nil
	case "codex":
		return [][]string{{"init", "--global", "--codex"}}, nil
	default:
		return nil, fmt.Errorf("AI Tool is not supported")
	}
}

func installHookSteps() [][]string {
	return [][]string{
		{"init", "--global", "--hook-only", "--auto-patch"},
		{"init", "--global", "--agent", "cursor", "--hook-only", "--auto-patch"},
		{"init", "--global", "--gemini", "--hook-only", "--auto-patch"},
		{"init", "--global", "--codex"},
	}
}

func removeHookSteps() [][]string {
	return [][]string{
		{"init", "--global", "--uninstall", "--auto-patch"},
		{"init", "--global", "--gemini", "--uninstall", "--auto-patch"},
		{"init", "--global", "--codex", "--uninstall"},
	}
}

func hookEnv(home string) []string {
	base := os.Environ()
	out := make([]string, 0, len(base)+1)
	for _, item := range base {
		if strings.HasPrefix(item, "HOME=") {
			continue
		}
		out = append(out, item)
	}
	return append(out, "HOME="+home)
}

// HookReady reports that this AI tool has the RTK command hook.
// The hook rewrites a shell command before it runs.
func HookReady(home, tool string) error {
	home = strings.TrimSpace(home)
	tool = strings.TrimSpace(tool)
	if home == "" {
		return fmt.Errorf("home directory is not set")
	}
	spec, err := hookSpec(tool)
	if err != nil {
		return err
	}
	raw, err := os.ReadFile(filepath.Join(home, spec.rel))
	if err != nil {
		return fmt.Errorf("%s RTK hook is not installed", hookToolTitle(tool))
	}
	var root map[string]any
	if err := json.Unmarshal(raw, &root); err != nil {
		return fmt.Errorf("%s RTK hook is not installed", hookToolTitle(tool))
	}
	hooks, _ := root["hooks"].(map[string]any)
	if !eventHasRTK(hooks[spec.event], spec) {
		return fmt.Errorf("%s RTK hook %s is missing", hookToolTitle(tool), spec.event)
	}
	if spec.script == "" {
		return nil
	}
	if _, err := os.Stat(filepath.Join(home, spec.script)); err != nil {
		return fmt.Errorf("%s RTK hook script is missing", hookToolTitle(tool))
	}
	return nil
}

type rtkHookSpec struct {
	rel     string
	event   string
	command string
	matcher string
	grouped bool
	script  string
}

func hookSpec(tool string) (rtkHookSpec, error) {
	switch tool {
	case "cursor":
		return rtkHookSpec{
			rel:     filepath.Join(".cursor", "hooks.json"),
			event:   "preToolUse",
			command: "rtk hook cursor",
			matcher: "Shell",
		}, nil
	case "claude":
		return rtkHookSpec{
			rel:     filepath.Join(".claude", "settings.json"),
			event:   "PreToolUse",
			command: "rtk hook claude",
			matcher: "Bash",
			grouped: true,
		}, nil
	case "codex":
		return rtkHookSpec{
			rel:     filepath.Join(".codex", "hooks.json"),
			event:   "PreToolUse",
			command: "rtk hook codex",
			matcher: "Bash",
			grouped: true,
		}, nil
	case "gemini":
		return rtkHookSpec{
			rel:     filepath.Join(".gemini", "settings.json"),
			event:   "BeforeTool",
			command: "rtk-hook-gemini.sh",
			matcher: "run_shell_command",
			grouped: true,
			script:  filepath.Join(".gemini", "hooks", "rtk-hook-gemini.sh"),
		}, nil
	default:
		return rtkHookSpec{}, fmt.Errorf("AI Tool is not supported")
	}
}

func eventHasRTK(value any, spec rtkHookSpec) bool {
	list, ok := value.([]any)
	if !ok {
		return false
	}
	if !spec.grouped {
		for _, item := range list {
			obj, _ := item.(map[string]any)
			if commandMatches(obj["command"], spec.command) && matcherMatches(obj["matcher"], spec.matcher) {
				return true
			}
		}
		return false
	}
	for _, item := range list {
		obj, _ := item.(map[string]any)
		if !matcherMatches(obj["matcher"], spec.matcher) {
			continue
		}
		inner, _ := obj["hooks"].([]any)
		for _, hook := range inner {
			row, _ := hook.(map[string]any)
			if commandMatches(row["command"], spec.command) {
				return true
			}
		}
	}
	return false
}

func commandMatches(value any, want string) bool {
	cmd, _ := value.(string)
	return strings.Contains(cmd, want)
}

func matcherMatches(value any, want string) bool {
	got, _ := value.(string)
	return strings.EqualFold(strings.TrimSpace(got), want)
}

func removeToolHook(home, tool string) error {
	spec, err := hookSpec(tool)
	if err != nil {
		return err
	}
	scriptGone, err := removeRTKScript(home, spec)
	if err != nil {
		return err
	}
	path := filepath.Join(home, spec.rel)
	raw, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if len(strings.TrimSpace(string(raw))) == 0 {
		return nil
	}
	var root map[string]any
	if err := json.Unmarshal(raw, &root); err != nil {
		return fmt.Errorf("%s RTK hook: %w", hookToolTitle(tool), err)
	}
	hooks, _ := root["hooks"].(map[string]any)
	if root["hooks"] != nil && hooks == nil {
		return fmt.Errorf("%s RTK hook: hooks are not an object", hookToolTitle(tool))
	}
	changed := scriptGone
	if hooks != nil {
		for event, value := range hooks {
			next, removed, err := stripRTK(value, spec)
			if err != nil {
				return fmt.Errorf("%s %s: %w", hookToolTitle(tool), event, err)
			}
			if !removed {
				continue
			}
			changed = true
			if len(next) == 0 {
				delete(hooks, event)
			} else {
				hooks[event] = next
			}
		}
	}
	if !changed {
		return nil
	}
	raw, err = json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	return writeHookFile(path, raw)
}

func stripRTK(value any, spec rtkHookSpec) ([]any, bool, error) {
	if value == nil {
		return nil, false, nil
	}
	list, ok := value.([]any)
	if !ok {
		return nil, false, fmt.Errorf("hooks are not a list")
	}
	var out []any
	removed := false
	for _, item := range list {
		obj, ok := item.(map[string]any)
		if !ok {
			out = append(out, item)
			continue
		}
		if !spec.grouped {
			if commandMatches(obj["command"], spec.command) {
				removed = true
				continue
			}
			out = append(out, obj)
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
			if commandMatches(row["command"], spec.command) {
				removed = true
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
	return out, removed, nil
}

func removeRTKScript(home string, spec rtkHookSpec) (bool, error) {
	if spec.script == "" {
		return false, nil
	}
	gone := false
	for _, rel := range []string{spec.script, filepath.Join(".gemini", "hooks", ".rtk-hook.sha256")} {
		err := os.Remove(filepath.Join(home, rel))
		if err == nil {
			gone = true
			continue
		}
		if !os.IsNotExist(err) {
			return false, err
		}
	}
	return gone, nil
}

func writeHookFile(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	data = append([]byte(strings.TrimSpace(string(data))), '\n')
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

func hookToolTitle(tool string) string {
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

func hookOutput(out []byte, err error) string {
	text := strings.TrimSpace(string(out))
	if text == "" {
		return err.Error()
	}
	if len(text) > 400 {
		text = text[:400]
	}
	return text
}
