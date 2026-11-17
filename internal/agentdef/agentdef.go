// Package agentdef is the catalog of KIKI agents embedded in the binary.
// The documents live in internal/agents and are compiled in by go build.
package agentdef

import (
	"fmt"
	"strings"

	"kiki/internal/agents"
)

// DefaultGuide is the guide file an agent reads in a repository when none is set.
const DefaultGuide = "AGENTS.md"

// GuideFile is the guide file name to use. An empty name is DefaultGuide.
func GuideFile(name string) string {
	name = strings.TrimSpace(name)
	if name == "" {
		return DefaultGuide
	}
	return name
}

// ValidGuide reports whether name is a markdown file name with no folder in it.
func ValidGuide(name string) bool {
	name = GuideFile(name)
	if strings.ContainsAny(name, "/\\\r\n\x00") {
		return false
	}
	return len(name) > len(".md") && strings.EqualFold(name[len(name)-len(".md"):], ".md")
}

// Repo is one repository this agent uses.
// Guide is the markdown file the agent reads and follows when it works in that
// repository. Empty means DefaultGuide.
// Description is the note written into the agent file when KIKI creates it.
// Readonly means this agent may read the repository and must not change it.
type Repo struct {
	Kind        string
	Guide       string
	Description string
	Readonly    bool
}

// Peer is one agent this agent coordinates with.
// Collaboration is the note written into the agent file when KIKI creates it.
type Peer struct {
	ID            string
	Collaboration string
}

// Use is one MCP server or tool this agent uses.
// Name is the label shown in the launch prompt. Description is what the agent does with it.
// Board and space fields are the Jira board or Confluence space and parent page for that server.
type Use struct {
	ID          string
	Name        string
	Description string
	BoardID     string
	BoardName   string
	SpaceID     string
	SpaceKey    string
	SpaceName   string
	ParentID    string
	ParentTitle string
}

// Agent is one embedded instruction document the console can start.
type Agent struct {
	ID      string
	Title   string
	Group   string
	File    string
	Repos   []Repo
	Peers   []Peer
	MCPs    []Use
	Tools   []Use
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
		MCPs:    atlassianMCPs(plannerJira, plannerConfluence),
		Summary: "Creates Jira tickets from a requirement folder.",
	},
	{
		ID:      "backend_runner",
		Title:   "Senior Backend Developer",
		Group:   "Backend",
		File:    "development/backend/runner.md",
		Repos:   []Repo{{Kind: "backend", Description: runnerRepo("backend", "website, mobile, or document")}},
		Peers:   []Peer{{ID: "backend_reviewer", Collaboration: runnerToReviewer("backend_runner", "backend_reviewer")}},
		MCPs:    atlassianMCPs(runnerJira, runnerConfluence),
		Summary: "Implements a backend ticket and opens a pull request.",
	},
	{
		ID:      "backend_reviewer",
		Title:   "Backend Reviewer",
		Group:   "Backend",
		File:    "development/backend/reviewer.md",
		Repos:   []Repo{{Kind: "backend", Description: reviewerRepo("backend", "website, mobile, or document")}},
		Peers:   []Peer{{ID: "backend_runner", Collaboration: reviewerToRunner("backend_reviewer", "backend_runner")}},
		MCPs:    atlassianMCPs(reviewerJira, reviewerConfluence),
		Summary: "Reviews pull requests from backend_runner.",
	},
	{
		ID:      "website_runner",
		Title:   "Senior Website Developer",
		Group:   "Website",
		File:    "development/website/runner.md",
		Repos:   []Repo{{Kind: "website", Description: runnerRepo("website", "backend, mobile, or document")}},
		Peers:   []Peer{{ID: "website_reviewer", Collaboration: runnerToReviewer("website_runner", "website_reviewer")}},
		MCPs:    atlassianMCPs(runnerJira, runnerConfluence),
		Summary: "Implements a website ticket and opens a pull request.",
	},
	{
		ID:      "website_reviewer",
		Title:   "Website Reviewer",
		Group:   "Website",
		File:    "development/website/reviewer.md",
		Repos:   []Repo{{Kind: "website", Description: reviewerRepo("website", "backend, mobile, or document")}},
		Peers:   []Peer{{ID: "website_runner", Collaboration: reviewerToRunner("website_reviewer", "website_runner")}},
		MCPs:    atlassianMCPs(reviewerJira, reviewerConfluence),
		Summary: "Reviews pull requests from website_runner.",
	},
	{
		ID:      "mobile_runner",
		Title:   "Senior Mobile Developer",
		Group:   "Mobile",
		File:    "development/mobile/runner.md",
		Repos:   []Repo{{Kind: "mobile", Description: runnerRepo("mobile", "backend, website, or document")}},
		Peers:   []Peer{{ID: "mobile_reviewer", Collaboration: runnerToReviewer("mobile_runner", "mobile_reviewer")}},
		MCPs:    atlassianMCPs(runnerJira, runnerConfluence),
		Summary: "Implements a mobile ticket and opens a pull request.",
	},
	{
		ID:      "mobile_reviewer",
		Title:   "Mobile Reviewer",
		Group:   "Mobile",
		File:    "development/mobile/reviewer.md",
		Repos:   []Repo{{Kind: "mobile", Description: reviewerRepo("mobile", "backend, website, or document")}},
		Peers:   []Peer{{ID: "mobile_runner", Collaboration: reviewerToRunner("mobile_reviewer", "mobile_runner")}},
		MCPs:    atlassianMCPs(reviewerJira, reviewerConfluence),
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
