// Package agentdef is the catalog of KIKI agents embedded in the binary.
// The documents live in app/agents and are compiled in by go build.
package agentdef

import (
	"fmt"
	"strings"

	"kiki/agents"
)

// Agent is one embedded instruction document the console can start.
type Agent struct {
	ID      string
	Title   string
	Group   string
	File    string
	Repos   []string
	Peer    string
	Summary string
}

// Catalog is the launch order shown in the console.
var Catalog = []Agent{
	{
		ID:      "requirement_document",
		Title:   "Document Writer",
		Group:   "Requirement",
		File:    "requirement/document.md",
		Repos:   []string{"document"},
		Peer:    "requirement_designer",
		Summary: "Writes business_requirement.md and product_requirement.md.",
	},
	{
		ID:      "requirement_designer",
		Title:   "UI Designer",
		Group:   "Requirement",
		File:    "requirement/designer.md",
		Repos:   []string{"document"},
		Summary: "Draws design.html from the product requirement.",
	},
	{
		ID:      "requirement_planner",
		Title:   "Planner",
		Group:   "Requirement",
		File:    "requirement/planner.md",
		Repos:   []string{"document", "backend", "website", "mobile"},
		Summary: "Creates Jira tickets from a requirement folder.",
	},
	{
		ID:      "backend_runner",
		Title:   "Senior Backend Developer",
		Group:   "Backend",
		File:    "development/backend/runner.md",
		Repos:   []string{"backend"},
		Peer:    "backend_reviewer",
		Summary: "Implements a backend ticket and opens a pull request.",
	},
	{
		ID:      "backend_reviewer",
		Title:   "Backend Reviewer",
		Group:   "Backend",
		File:    "development/backend/reviewer.md",
		Repos:   []string{"backend"},
		Peer:    "backend_runner",
		Summary: "Reviews pull requests from backend_runner.",
	},
	{
		ID:      "website_runner",
		Title:   "Senior Website Developer",
		Group:   "Website",
		File:    "development/website/runner.md",
		Repos:   []string{"website"},
		Peer:    "website_reviewer",
		Summary: "Implements a website ticket and opens a pull request.",
	},
	{
		ID:      "website_reviewer",
		Title:   "Website Reviewer",
		Group:   "Website",
		File:    "development/website/reviewer.md",
		Repos:   []string{"website"},
		Peer:    "website_runner",
		Summary: "Reviews pull requests from website_runner.",
	},
	{
		ID:      "mobile_runner",
		Title:   "Senior Mobile Developer",
		Group:   "Mobile",
		File:    "development/mobile/runner.md",
		Repos:   []string{"mobile"},
		Peer:    "mobile_reviewer",
		Summary: "Implements a mobile ticket and opens a pull request.",
	},
	{
		ID:      "mobile_reviewer",
		Title:   "Mobile Reviewer",
		Group:   "Mobile",
		File:    "development/mobile/reviewer.md",
		Repos:   []string{"mobile"},
		Peer:    "mobile_runner",
		Summary: "Reviews pull requests from mobile_runner.",
	},
}

// ByID returns the catalog entry for an agent id.
func ByID(id string) (Agent, bool) {
	for _, agent := range Catalog {
		if agent.ID == id {
			return agent, true
		}
	}
	return Agent{}, false
}

// Body returns the embedded markdown for this agent.
func (a Agent) Body() (string, error) {
	raw, err := agentdocs.Files.ReadFile(a.File)
	if err != nil {
		return "", fmt.Errorf("read embedded agent %s: %w", a.ID, err)
	}
	text := string(raw)
	if !strings.Contains(text, "**Agent ID:** `"+a.ID+"`") {
		return "", fmt.Errorf("embedded agent %s does not contain its agent id", a.ID)
	}
	return text, nil
}
