package server

import (
	"strings"
	"testing"

	"kiki/internal/agentdef"
	"kiki/internal/store"
)

func TestEpicNoteMatchesTheAgentBoard(t *testing.T) {
	task := store.Task{EpicKey: "KIKI-8", EpicBoardID: "12", EpicBoardName: "KIKI"}
	note := "Task: Ship billing\n\nAdd the invoice page."
	jira := agentdef.Agent{MCPs: []agentdef.Use{{ID: "atlassian_jira", BoardID: "12", BoardName: "KIKI"}}}
	got := epicNote(note, task, jira)
	for _, want := range []string{
		"Task: Ship billing",
		"Add the invoice page.",
		"Jira epic: KIKI-8",
		"Board: KIKI (12)",
		"This task belongs to this Jira epic. Do the work under this epic.",
		"If you create Jira tickets that are not sub-tasks, create them under this epic.",
		"If you create sub-tasks, create those sub-task tickets as children of the ticket this agent needs.",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("missing %q in\n%s", want, got)
		}
	}
	other := agentdef.Agent{MCPs: []agentdef.Use{{ID: "atlassian_jira", BoardID: "14", BoardName: "Platform"}}}
	if epicNote(note, task, other) != note {
		t.Fatal("epic was sent to a different board")
	}
	off := agentdef.Agent{MCPs: []agentdef.Use{{ID: "github"}}}
	if epicNote(note, task, off) != note {
		t.Fatal("epic was sent to an agent without Jira")
	}
	if epicNote(note, store.Task{}, jira) != note {
		t.Fatal("missing epic was sent")
	}
}
