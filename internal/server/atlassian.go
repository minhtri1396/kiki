package server

import (
	"fmt"
	"os"
	"strings"

	"kiki/internal/agentdef"
	"kiki/internal/mcp"
)

type atlassianBody struct {
	Enabled    bool        `json:"enabled"`
	Username   string      `json:"username"`
	APIToken   string      `json:"apiToken"`
	Jira       serviceBody `json:"jira"`
	Confluence serviceBody `json:"confluence"`
}

type serviceBody struct {
	Enabled     bool   `json:"enabled"`
	URL         string `json:"url"`
	Own         bool   `json:"ownCredentials"`
	Username    string `json:"username"`
	APIToken    string `json:"apiToken"`
	BoardID     string `json:"boardId"`
	BoardName   string `json:"boardName"`
	SpaceID     string `json:"spaceId"`
	SpaceKey    string `json:"spaceKey"`
	SpaceName   string `json:"spaceName"`
	ParentID    string `json:"parentId"`
	ParentTitle string `json:"parentTitle"`
}

func (b atlassianBody) form() mcp.Atlassian {
	return mcp.Atlassian{
		Enabled:    b.Enabled,
		Username:   b.Username,
		APIToken:   b.APIToken,
		Jira:       b.Jira.form(),
		Confluence: b.Confluence.form(),
	}
}

func (b serviceBody) form() mcp.Service {
	return mcp.Service{
		Enabled:     b.Enabled,
		URL:         b.URL,
		Own:         b.Own,
		Username:    b.Username,
		APIToken:    b.APIToken,
		BoardID:     b.BoardID,
		BoardName:   b.BoardName,
		SpaceID:     b.SpaceID,
		SpaceKey:    b.SpaceKey,
		SpaceName:   b.SpaceName,
		ParentID:    b.ParentID,
		ParentTitle: b.ParentTitle,
	}
}

func atlassianState(cfg mcp.Atlassian) map[string]any {
	return map[string]any{
		"enabled":     cfg.Enabled,
		"username":    cfg.Username,
		"apiToken":    cfg.APIToken,
		"apiTokenSet": cfg.APIToken != "",
		"jira":        serviceState(cfg.Jira),
		"confluence":  serviceState(cfg.Confluence),
	}
}

func serviceState(service mcp.Service) map[string]any {
	return map[string]any{
		"enabled":        service.Enabled,
		"url":            service.URL,
		"ownCredentials": service.Own,
		"username":       service.Username,
		"apiToken":       service.APIToken,
		"apiTokenSet":    service.APIToken != "",
		"boardId":        service.BoardID,
		"boardName":      service.BoardName,
		"spaceId":        service.SpaceID,
		"spaceKey":       service.SpaceKey,
		"spaceName":      service.SpaceName,
		"parentId":       service.ParentID,
		"parentTitle":    service.ParentTitle,
	}
}

// ensureAgentMCPs installs each MCP turned on for this agent into the AI Tool
// selected for the run. An MCP the agent does not use is left out.
func (s *Server) ensureAgentMCPs(prep launchPrep) error {
	if !agentUsesAtlassian(prep.agent.MCPs) {
		return nil
	}
	return s.ensureAtlassianMCP(prep)
}

func atlassianReadyFor(cfg mcp.Atlassian, list []agentdef.Use) error {
	if !cfg.Ready() {
		return fmt.Errorf("set up Atlassian MCP before starting this agent")
	}
	for _, item := range list {
		switch strings.TrimSpace(item.ID) {
		case "atlassian_jira":
			if !cfg.Jira.Enabled {
				return fmt.Errorf("set up Atlassian - Jira before starting this agent")
			}
		case "atlassian_confluence":
			if !cfg.Confluence.Enabled {
				return fmt.Errorf("set up Atlassian - Confluence before starting this agent")
			}
		}
	}
	return nil
}

func agentUsesAtlassian(list []agentdef.Use) bool {
	for _, item := range list {
		switch strings.TrimSpace(item.ID) {
		case "atlassian", "atlassian_jira", "atlassian_confluence":
			return true
		}
	}
	return false
}

// ensureAtlassianMCP installs Atlassian MCP under mcp-atlassian-<workspace>
// when the agent's AI Tool does not already have that key.
func (s *Server) ensureAtlassianMCP(prep launchPrep) error {
	if s.Store == nil {
		return nil
	}
	cfg, err := s.Store.Atlassian()
	if err != nil {
		return err
	}
	if err := atlassianReadyFor(cfg, prep.agent.MCPs); err != nil {
		return err
	}
	home := s.MCPHome
	if home == "" {
		home, err = os.UserHomeDir()
		if err != nil {
			return err
		}
	}
	worktree := prep.worktree
	if prep.taskID != "" {
		// A feature run starts in a new checkout. Only the AI Tool's own
		// settings are sure to be there.
		worktree = ""
	}
	return mcp.Ensure(prep.tool, worktree, home, prep.snap.WorkspaceID, cfg)
}
