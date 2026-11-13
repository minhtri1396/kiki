// Package agentdef is the catalog of KIKI agents embedded in the binary.
// The documents live in internal/agents and are compiled in by go build.
package agentdef

import (
	"fmt"
	"strings"

	"kiki/internal/agents"
)

// Repo is one repository this agent uses.
// Description is the note written into the agent file when KIKI creates it.
type Repo struct {
	Kind        string
	Description string
}

// Peer is one agent this agent coordinates with.
// Collaboration is the note written into the agent file when KIKI creates it.
type Peer struct {
	ID            string
	Collaboration string
}

// Agent is one embedded instruction document the console can start.
type Agent struct {
	ID      string
	Title   string
	Group   string
	File    string
	Repos   []Repo
	Peers   []Peer
	Summary string
}

// Catalog is the launch order shown in the console.
var Catalog = []Agent{
	{
		ID:      "requirement_document",
		Title:   "Document Writer",
		Group:   "Requirement",
		File:    "requirement/document.md",
		Repos:   []Repo{{Kind: "document", Description: documentWriterRepo}},
		Peers:   []Peer{{ID: "requirement_designer", Collaboration: documentToDesigner}},
		Summary: "Writes business_requirement.md and product_requirement.md.",
	},
	{
		ID:      "requirement_designer",
		Title:   "UI Designer",
		Group:   "Requirement",
		File:    "requirement/designer.md",
		Repos:   []Repo{{Kind: "document", Description: designerRepo}},
		Peers:   []Peer{{ID: "requirement_document", Collaboration: designerToDocument}},
		Summary: "Draws design.html from the product requirement.",
	},
	{
		ID:    "requirement_planner",
		Title: "Planner",
		Group: "Requirement",
		File:  "requirement/planner.md",
		Repos: []Repo{
			{Kind: "document", Description: plannerDocumentRepo},
			{Kind: "backend", Description: plannerCodeRepo("backend")},
			{Kind: "website", Description: plannerCodeRepo("website")},
			{Kind: "mobile", Description: plannerCodeRepo("mobile")},
		},
		Peers: []Peer{
			{ID: "requirement_document", Collaboration: plannerToDocument},
			{ID: "requirement_designer", Collaboration: plannerToDesigner},
			{ID: "backend_runner", Collaboration: plannerToRunner("backend_runner", "backend")},
			{ID: "website_runner", Collaboration: plannerToRunner("website_runner", "website")},
			{ID: "mobile_runner", Collaboration: plannerToRunner("mobile_runner", "mobile")},
		},
		Summary: "Creates Jira tickets from a requirement folder.",
	},
	{
		ID:      "backend_runner",
		Title:   "Senior Backend Developer",
		Group:   "Backend",
		File:    "development/backend/runner.md",
		Repos:   []Repo{{Kind: "backend", Description: runnerRepo("backend", "website, mobile, or document")}},
		Peers:   []Peer{{ID: "backend_reviewer", Collaboration: runnerToReviewer("backend_runner", "backend_reviewer")}},
		Summary: "Implements a backend ticket and opens a pull request.",
	},
	{
		ID:      "backend_reviewer",
		Title:   "Backend Reviewer",
		Group:   "Backend",
		File:    "development/backend/reviewer.md",
		Repos:   []Repo{{Kind: "backend", Description: reviewerRepo("backend", "website, mobile, or document")}},
		Peers:   []Peer{{ID: "backend_runner", Collaboration: reviewerToRunner("backend_reviewer", "backend_runner")}},
		Summary: "Reviews pull requests from backend_runner.",
	},
	{
		ID:      "website_runner",
		Title:   "Senior Website Developer",
		Group:   "Website",
		File:    "development/website/runner.md",
		Repos:   []Repo{{Kind: "website", Description: runnerRepo("website", "backend, mobile, or document")}},
		Peers:   []Peer{{ID: "website_reviewer", Collaboration: runnerToReviewer("website_runner", "website_reviewer")}},
		Summary: "Implements a website ticket and opens a pull request.",
	},
	{
		ID:      "website_reviewer",
		Title:   "Website Reviewer",
		Group:   "Website",
		File:    "development/website/reviewer.md",
		Repos:   []Repo{{Kind: "website", Description: reviewerRepo("website", "backend, mobile, or document")}},
		Peers:   []Peer{{ID: "website_runner", Collaboration: reviewerToRunner("website_reviewer", "website_runner")}},
		Summary: "Reviews pull requests from website_runner.",
	},
	{
		ID:      "mobile_runner",
		Title:   "Senior Mobile Developer",
		Group:   "Mobile",
		File:    "development/mobile/runner.md",
		Repos:   []Repo{{Kind: "mobile", Description: runnerRepo("mobile", "backend, website, or document")}},
		Peers:   []Peer{{ID: "mobile_reviewer", Collaboration: runnerToReviewer("mobile_runner", "mobile_reviewer")}},
		Summary: "Implements a mobile ticket and opens a pull request.",
	},
	{
		ID:      "mobile_reviewer",
		Title:   "Mobile Reviewer",
		Group:   "Mobile",
		File:    "development/mobile/reviewer.md",
		Repos:   []Repo{{Kind: "mobile", Description: reviewerRepo("mobile", "backend, website, or document")}},
		Peers:   []Peer{{ID: "mobile_runner", Collaboration: reviewerToRunner("mobile_reviewer", "mobile_runner")}},
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
	if !strings.Contains(text, "# Agent:") {
		return "", fmt.Errorf("embedded agent %s has no document heading", a.ID)
	}
	if strings.Contains(text, "## Works with agents") || strings.Contains(text, "**Agent ID:**") {
		return "", fmt.Errorf("embedded agent %s repeats fields the launch prompt already includes", a.ID)
	}
	if strings.Contains(text, "obsidian vault=") || strings.Contains(text, "codegraph explore") || strings.Contains(text, "codegraph_explore") {
		return "", fmt.Errorf("embedded agent %s contains index instructions the launch prompt adds", a.ID)
	}
	return text, nil
}
