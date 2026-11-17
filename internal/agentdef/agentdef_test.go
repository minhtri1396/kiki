package agentdef

import (
	"strings"
	"testing"
)

func TestEmbeddedDocumentsMatchTheLaunchPrompt(t *testing.T) {
	for _, agent := range Catalog {
		body, err := agent.Body()
		if err != nil {
			t.Fatalf("%s: %v", agent.ID, err)
		}
		for _, stale := range []string{
			"config.local.yaml",
			"mcp.json",
			"jira.<ws>",
			"orca_agent",
			"Agent handoff",
			"Make yourself findable",
			"WAIT FOR A REQUEST.",
		} {
			if strings.Contains(body, stale) {
				t.Errorf("%s still contains %q", agent.ID, stale)
			}
		}
		if !strings.Contains(body, "Agents Coordination") {
			t.Errorf("%s does not mention Agents Coordination", agent.ID)
		}
	}
}
