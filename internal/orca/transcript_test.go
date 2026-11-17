package orca

import (
	"strings"
	"testing"
)

func TestTranscriptKeepsThinkingAndAnswer(t *testing.T) {
	screen := strings.Join([]string{
		"Cursor Agent",
		"v2026.11.13",
		"Tip: Use /plan to plan ahead.",
		"[Pasted text #1 +647 lines]",
		"Thinking",
		"The invoice page needs a handler.",
		"",
		"I added the invoice route and its test.",
		"Auto",
		"~/Documents/Goldeneye/ellum/docs · main",
		"Run Everything",
	}, "\n")
	text, ask := Transcript(screen)
	if ask.Prompt != "" {
		t.Fatalf("ask = %+v", ask)
	}
	want := "Thinking\nThe invoice page needs a handler.\n\nI added the invoice route and its test."
	if text != want {
		t.Fatalf("text = %q", text)
	}
}

func TestTranscriptQuestionWithChoices(t *testing.T) {
	screen := strings.Join([]string{
		"Thinking",
		"Two shapes fit this task.",
		"Which one should I write?",
		"1. Short brief",
		"2. Full specification",
	}, "\n")
	text, ask := Transcript(screen)
	if ask.Prompt != "Which one should I write?" || len(ask.Options) != 2 || ask.Options[0] != "Short brief" {
		t.Fatalf("ask = %+v", ask)
	}
	if text != "Thinking\nTwo shapes fit this task." {
		t.Fatalf("text = %q", text)
	}
}
