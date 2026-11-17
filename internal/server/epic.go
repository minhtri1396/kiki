package server

import (
	"fmt"
	"strings"

	"kiki/internal/agentdef"
	"kiki/internal/store"
)

// epicNote appends the Jira epic when this agent has Jira and the epic is on that agent's board.
func epicNote(note string, task store.Task, agent agentdef.Agent) string {
	hasJira, boardID := agentJiraBoard(agent)
	epicBoard := strings.TrimSpace(task.EpicBoardID)
	key := strings.TrimSpace(task.EpicKey)
	if !hasJira || key == "" || epicBoard == "" || epicBoard != strings.TrimSpace(boardID) {
		return note
	}
		block := fmt.Sprintf("Jira epic: %s\nBoard: %s\nThis task belongs to this Jira epic. Do the work under this epic.\nIf you create Jira tickets that are not sub-tasks, create them under this epic.\nIf you create sub-tasks, create those sub-task tickets as children of the ticket this agent needs.", key, boardLabel(task.EpicBoardName, task.EpicBoardID))
	note = strings.TrimRight(note, "\n")
	if note == "" {
		return block
	}
	return note + "\n\n" + block
}

func agentJiraBoard(agent agentdef.Agent) (bool, string) {
	for _, item := range agent.MCPs {
		if strings.TrimSpace(item.ID) != "atlassian_jira" {
			continue
		}
		return true, strings.TrimSpace(item.BoardID)
	}
	return false, ""
}

func boardLabel(name, id string) string {
	name = strings.TrimSpace(name)
	id = strings.TrimSpace(id)
	if name == "" {
		return id
	}
	if id == "" || id == name {
		return name
	}
	return name + " (" + id + ")"
}
