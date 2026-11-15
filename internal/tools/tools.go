// Package tools is the CLI tools Settings can turn on.
// A global tool applies to every agent. The others are chosen on each agent.
package tools

import (
	"os/exec"
	"strings"
)

// Spec is one tool the Settings page can offer.
// Global means the switch applies to every agent, with no switch on the agent.
type Spec struct {
	ID      string
	Name    string
	Command string
	Detail  string
	Page    string
	Global  bool
}

// Settings is which tools are turned on for this computer.
// A missing Guardrails value means guardrails are on.
type Settings struct {
	GitHubCLI  bool  `yaml:"github_cli,omitempty" json:"github_cli"`
	RTK        bool  `yaml:"rtk,omitempty" json:"rtk"`
	Guardrails *bool `yaml:"guardrails,omitempty" json:"guardrails,omitempty"`
}

// commandExists reports whether a CLI is on PATH. Tests replace it.
var commandExists = func(name string) bool {
	_, err := exec.LookPath(name)
	return err == nil
}

// Catalog is GitHub CLI, then RTK.
// GitHub CLI is chosen on each agent. RTK applies to every agent when it is on.
func Catalog() []Spec {
	return []Spec{
		{
			ID:      "github_cli",
			Name:    "GitHub CLI",
			Command: "gh",
			Detail:  "The gh command opens pull requests, issues, and checks. Use gh for that work.",
			Page:    "https://cli.github.com/",
		},
		{
			ID:      "rtk",
			Name:    "RTK",
			Command: "rtk",
			Detail:  "The AI tool hook rewrites shell commands through rtk before they run, so the output stays short.",
			Page:    "https://github.com/rtk-ai/rtk",
			Global:  true,
		},
	}
}

// Global reports that this id applies to every agent when Settings turns it on.
func Global(id string) bool {
	id = strings.TrimSpace(id)
	for _, item := range Catalog() {
		if item.ID == id {
			return item.Global
		}
	}
	return false
}

// Empty reports that nothing in this file needs to be stored.
// Guardrails stay on when the value is missing, so that choice is not stored.
func (s Settings) Empty() bool {
	return !s.GitHubCLI && !s.RTK && s.GuardrailsOn()
}

// GuardrailsOn reports that KIKI guardrail hooks should be installed.
func (s Settings) GuardrailsOn() bool {
	return s.Guardrails == nil || *s.Guardrails
}

// On reports that this catalog id is turned on.
func (s Settings) On(id string) bool {
	switch strings.TrimSpace(id) {
	case "github_cli":
		return s.GitHubCLI
	case "rtk":
		return s.RTK
	default:
		return false
	}
}

// Installed reports that this command is on PATH.
func Installed(command string) bool {
	command = strings.TrimSpace(command)
	return command != "" && commandExists(command)
}
