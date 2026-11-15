package tools

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestHookReadyAcceptsTheRTKCommandHook(t *testing.T) {
	home := t.TempDir()
	write := func(rel, body string) {
		t.Helper()
		path := filepath.Join(home, rel)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write(".cursor/hooks.json", `{"hooks":{"preToolUse":[{"command":"/opt/homebrew/bin/rtk hook cursor","matcher":"Shell"}]}}`)
	write(".claude/settings.json", `{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook claude"}]}]}}`)
	write(".codex/hooks.json", `{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook codex"}]}]}}`)
	write(".gemini/settings.json", `{"hooks":{"BeforeTool":[{"matcher":"run_shell_command","hooks":[{"type":"command","command":"`+filepath.Join(home, ".gemini", "hooks", "rtk-hook-gemini.sh")+`"}]}]}}`)
	write(".gemini/hooks/rtk-hook-gemini.sh", "#!/bin/sh\n")
	for _, tool := range []string{"cursor", "claude", "codex", "gemini"} {
		if err := HookReady(home, tool); err != nil {
			t.Fatalf("%s: %v", tool, err)
		}
	}
}

func TestHookReadyReportsAMissingRTKHook(t *testing.T) {
	home := t.TempDir()
	if err := os.MkdirAll(filepath.Join(home, ".cursor"), 0o755); err != nil {
		t.Fatal(err)
	}
	body := `{"hooks":{"preToolUse":[{"command":"rtk hook cursor","matcher":"Read"}]}}`
	if err := os.WriteFile(filepath.Join(home, ".cursor", "hooks.json"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	err := HookReady(home, "cursor")
	if err == nil || !strings.Contains(err.Error(), "Cursor RTK hook preToolUse is missing") {
		t.Fatalf("err = %v", err)
	}
	if err := HookReady(home, "claude"); err == nil || !strings.Contains(err.Error(), "Claude RTK hook is not installed") {
		t.Fatalf("claude = %v", err)
	}
	if err := os.MkdirAll(filepath.Join(home, ".gemini"), 0o755); err != nil {
		t.Fatal(err)
	}
	gemini := `{"hooks":{"BeforeTool":[{"matcher":"run_shell_command","hooks":[{"command":"rtk-hook-gemini.sh"}]}]}}`
	if err := os.WriteFile(filepath.Join(home, ".gemini", "settings.json"), []byte(gemini), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := HookReady(home, "gemini"); err == nil || !strings.Contains(err.Error(), "Gemini RTK hook script is missing") {
		t.Fatalf("gemini = %v", err)
	}
}

func TestRemoveHooksDropsRTKAndKeepsOtherHooks(t *testing.T) {
	home := t.TempDir()
	write := func(rel, body string) {
		t.Helper()
		path := filepath.Join(home, rel)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write(".cursor/hooks.json", `{"version":1,"hooks":{"preToolUse":[{"command":"rtk hook cursor","matcher":"Shell"},{"command":"/bin/kiki guardrail cursor"}]}}`)
	write(".claude/settings.json", `{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"command":"/opt/homebrew/bin/rtk hook claude"}]},{"hooks":[{"command":"/bin/kiki guardrail claude"}]}]}}`)
	write(".codex/hooks.json", `{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"command":"rtk hook codex"},{"command":"/bin/kiki guardrail codex"}]}]}}`)
	write(".gemini/settings.json", `{"hooks":{"BeforeTool":[{"matcher":"run_shell_command","hooks":[{"command":"`+filepath.Join(home, ".gemini", "hooks", "rtk-hook-gemini.sh")+`"}]},{"hooks":[{"command":"/bin/kiki guardrail gemini"}]}]}}`)
	write(".gemini/hooks/rtk-hook-gemini.sh", "#!/bin/sh\n")
	if err := RemoveHooks(home); err != nil {
		t.Fatal(err)
	}
	for _, item := range []struct{ rel, hook, keep string }{
		{".cursor/hooks.json", "rtk hook cursor", "/bin/kiki guardrail cursor"},
		{".claude/settings.json", "rtk hook claude", "/bin/kiki guardrail claude"},
		{".codex/hooks.json", "rtk hook codex", "/bin/kiki guardrail codex"},
		{".gemini/settings.json", "rtk-hook-gemini.sh", "/bin/kiki guardrail gemini"},
	} {
		body, err := os.ReadFile(filepath.Join(home, item.rel))
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(body), item.hook) || !strings.Contains(string(body), item.keep) {
			t.Fatalf("%s =\n%s", item.rel, body)
		}
	}
	if _, err := os.Stat(filepath.Join(home, ".gemini", "hooks", "rtk-hook-gemini.sh")); !os.IsNotExist(err) {
		t.Fatal("gemini script stayed")
	}
	for _, tool := range []string{"cursor", "claude", "codex", "gemini"} {
		if err := HookReady(home, tool); err == nil {
			t.Fatalf("%s hook stayed ready", tool)
		}
	}
}

func TestEnsureHookRunsTheToolInstall(t *testing.T) {
	prevExists := commandExists
	prevRun := runHook
	commandExists = func(name string) bool { return name == "rtk" }
	var got []string
	runHook = func(home string, args ...string) ([]byte, error) {
		if home == "" {
			t.Fatal("home is empty")
		}
		got = append(got, strings.Join(args, " "))
		return nil, nil
	}
	t.Cleanup(func() {
		commandExists = prevExists
		runHook = prevRun
	})
	if err := EnsureHook(t.TempDir(), "cursor"); err != nil {
		t.Fatal(err)
	}
	want := "init --global --agent cursor --hook-only --auto-patch"
	if len(got) != 1 || got[0] != want {
		t.Fatalf("calls = %v", got)
	}
	if err := EnsureHook(t.TempDir(), "other"); err == nil {
		t.Fatal("accepted an unknown AI tool")
	}
}

func TestSyncHooksSkipsWhenRTKIsMissingAndOff(t *testing.T) {
	prevExists := commandExists
	prevRun := runHook
	commandExists = func(string) bool { return false }
	runHook = func(string, ...string) ([]byte, error) {
		t.Fatal("rtk ran while it was missing and off")
		return nil, nil
	}
	t.Cleanup(func() {
		commandExists = prevExists
		runHook = prevRun
	})
	if err := SyncHooks(t.TempDir(), false); err != nil {
		t.Fatal(err)
	}
	if err := SyncHooks(t.TempDir(), true); err == nil || !strings.Contains(err.Error(), "not installed") {
		t.Fatalf("err = %v", err)
	}
}

func TestSyncHooksRunsInstallThenRemove(t *testing.T) {
	prevExists := commandExists
	prevRun := runHook
	commandExists = func(name string) bool { return name == "rtk" }
	var got []string
	runHook = func(home string, args ...string) ([]byte, error) {
		if home == "" {
			t.Fatal("home is empty")
		}
		got = append(got, strings.Join(args, " "))
		return nil, nil
	}
	t.Cleanup(func() {
		commandExists = prevExists
		runHook = prevRun
	})
	home := t.TempDir()
	if err := SyncHooks(home, true); err != nil {
		t.Fatal(err)
	}
	if err := SyncHooks(home, false); err != nil {
		t.Fatal(err)
	}
	want := append(installHookSteps(), removeHookSteps()...)
	if len(got) != len(want) {
		t.Fatalf("calls = %v", got)
	}
	for i, args := range want {
		if got[i] != strings.Join(args, " ") {
			t.Fatalf("call %d = %q, want %q", i, got[i], strings.Join(args, " "))
		}
	}
}

func TestSyncHooksInstallsBesideAnExistingGuardrail(t *testing.T) {
	if _, err := exec.LookPath("rtk"); err != nil {
		t.Skip("rtk is not installed")
	}
	home := t.TempDir()
	for _, dir := range []string{".cursor", ".claude", ".codex", ".gemini"} {
		if err := os.MkdirAll(filepath.Join(home, dir), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	write := func(rel, body string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(home, rel), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write(".cursor/hooks.json", `{"version":1,"hooks":{"preToolUse":[{"command":"/bin/kiki guardrail cursor"}]}}`+"\n")
	write(".claude/settings.json", `{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"/bin/kiki guardrail claude"}]}]}}`+"\n")
	write(".codex/hooks.json", `{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"/bin/kiki guardrail codex"}]}]}}`+"\n")
	write(".gemini/settings.json", `{"hooks":{"BeforeTool":[{"hooks":[{"type":"command","command":"/bin/kiki guardrail gemini","name":"kiki-guardrail"}]}]}}`+"\n")

	if err := SyncHooks(home, true); err != nil {
		t.Fatal(err)
	}
	for _, item := range []struct{ rel, hook, guard string }{
		{".cursor/hooks.json", "rtk hook cursor", "/bin/kiki guardrail cursor"},
		{".claude/settings.json", "rtk hook claude", "/bin/kiki guardrail claude"},
		{".codex/hooks.json", "rtk hook codex", "/bin/kiki guardrail codex"},
		{".gemini/settings.json", "rtk-hook-gemini.sh", "/bin/kiki guardrail gemini"},
	} {
		body, err := os.ReadFile(filepath.Join(home, item.rel))
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(body), item.hook) || !strings.Contains(string(body), item.guard) {
			t.Fatalf("%s =\n%s", item.rel, body)
		}
	}
	if err := SyncHooks(home, false); err != nil {
		t.Fatal(err)
	}
	for _, item := range []struct{ rel, hook, guard string }{
		{".cursor/hooks.json", "rtk hook cursor", "/bin/kiki guardrail cursor"},
		{".claude/settings.json", "rtk hook claude", "/bin/kiki guardrail claude"},
		{".codex/hooks.json", "rtk hook codex", "/bin/kiki guardrail codex"},
		{".gemini/settings.json", "rtk-hook-gemini.sh", "/bin/kiki guardrail gemini"},
	} {
		body, err := os.ReadFile(filepath.Join(home, item.rel))
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(body), item.hook) || !strings.Contains(string(body), item.guard) {
			t.Fatalf("after removal %s =\n%s", item.rel, body)
		}
	}
}
