package orca

import (
	"context"
	"os"
	"strings"
	"testing"
)

func TestParseTerminalRead(t *testing.T) {
	view, err := parseTerminalRead([]byte(`{
		"ok": true,
		"result": {"terminal": {"handle": "term_a", "status": "running", "tail": ["hello", "world"]}}
	}`))
	if err != nil {
		t.Fatal(err)
	}
	if view.Handle != "term_a" || view.Status != "running" || view.Log != "hello\nworld" {
		t.Fatalf("view = %+v", view)
	}
	if _, err := parseTerminalRead([]byte(`{"ok":false,"error":{"message":"missing"}}`)); err == nil || err.Error() != "missing" {
		t.Fatalf("err = %v", err)
	}
}

func TestParseTerminalIdle(t *testing.T) {
	idle, err := parseTerminalIdle([]byte(`{"ok":true}`))
	if err != nil || !idle {
		t.Fatalf("idle = %v err = %v", idle, err)
	}
	idle, err = parseTerminalIdle([]byte(`{"ok":false,"error":{"code":"timeout","message":"timeout"}}`))
	if err != nil || idle {
		t.Fatalf("idle = %v err = %v", idle, err)
	}
	if _, err := parseTerminalIdle([]byte(`{"ok":false,"error":{"code":"gone","message":"gone"}}`)); err == nil || err.Error() != "gone" {
		t.Fatalf("err = %v", err)
	}
}

func TestUnsentPrompt(t *testing.T) {
	if !unsentPrompt(TerminalView{Draft: "[Pasted text #1 +647 lines]"}) {
		t.Fatal("draft was treated as sent")
	}
	if !unsentPrompt(TerminalView{Log: "Cursor Agent\n[Pasted text #1 +647 lines]"}) {
		t.Fatal("pasted screen was treated as sent")
	}
	if unsentPrompt(TerminalView{Log: "working"}) {
		t.Fatal("plain screen was treated as unsent")
	}
}

func TestDraftText(t *testing.T) {
	if got := draftText([]byte(`"[Pasted text #1 +647 lines]"`)); got != "[Pasted text #1 +647 lines]" {
		t.Fatalf("draft = %q", got)
	}
	if got := draftText([]byte(`{"text":"hello"}`)); got != "hello" {
		t.Fatalf("object draft = %q", got)
	}
}

func TestSubmitPastedPromptPressesEnter(t *testing.T) {
	dir := t.TempDir()
	script := dir + "/orca"
	log := dir + "/args"
	body := `#!/bin/sh
printf '%s\n' "$*" >> "$ORCA_ARGS"
case "$1 $2" in
  "terminal read")
    printf '%s\n' '{"ok":true,"result":{"terminal":{"handle":"term_a","status":"running","draft":"[Pasted text #1 +647 lines]","tail":["Cursor Agent"]}}}'
    ;;
  "terminal wait")
    printf '%s\n' '{"ok":true}'
    ;;
  "terminal send")
    printf '%s\n' '{"ok":true}'
    ;;
  *)
    echo unexpected >&2
    exit 1
    ;;
esac
`
	if err := os.WriteFile(script, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("ORCA_ARGS", log)
	client := &Client{Bin: script}
	if err := client.submitPastedPrompt(context.Background(), "term_a"); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(log)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(raw), "terminal send --terminal term_a --enter --wait-submit 8 --json") {
		t.Fatalf("args =\n%s", raw)
	}
}

func TestTerminalHandle(t *testing.T) {
	raw := `{"ok":true,"result":{"outcome":{"handle":"term_a"}}}`
	if TerminalHandle(raw) != "term_a" {
		t.Fatalf("handle = %q", TerminalHandle(raw))
	}
}
