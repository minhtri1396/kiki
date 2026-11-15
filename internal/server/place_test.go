package server

import (
	"testing"

	"kiki/internal/agentdef"
	"kiki/internal/mcp"
)

func TestApplyAtlassianPlacesUsesSettingsUntilTheAgentChooses(t *testing.T) {
	cfg := mcp.Atlassian{
		Jira: mcp.Service{BoardID: "12", BoardName: "KIKI"},
		Confluence: mcp.Service{
			SpaceID: "111", SpaceKey: "ENG", SpaceName: "Engineering",
			ParentID: "99", ParentTitle: "Requirements",
		},
	}
	uses := []agentdef.Use{{
		ID: "atlassian_jira",
	}, {
		ID:        "atlassian_confluence",
		SpaceID:   "222",
		SpaceKey:  "OPS",
		SpaceName: "Operations",
	}}
	got := applyAtlassianPlaces(uses, cfg)
	if got[0].BoardID != "12" || got[0].BoardName != "KIKI" {
		t.Fatalf("jira = %+v", got[0])
	}
	if got[1].SpaceKey != "OPS" || got[1].ParentTitle != "" {
		t.Fatalf("confluence = %+v", got[1])
	}
	chosen := applyAtlassianPlaces([]agentdef.Use{{ID: "atlassian_jira", BoardID: "14", BoardName: "Platform"}}, cfg)
	if chosen[0].BoardName != "Platform" {
		t.Fatalf("override = %+v", chosen[0])
	}
}
