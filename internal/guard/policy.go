package guard

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Policy is what the user allowed for one agent.
// ReadOnly lists roots the agent may read and must not change.
type Policy struct {
	AgentID   string
	Workspace string
	MCPs      []string
	Tools     []string
	Agents    []string
	Roots     []string
	ReadOnly  []string
	RTK       bool
}

// Target is one concrete interaction found in a hook event.
// Write is true when a repository interaction changes files.
type Target struct {
	Kind  string
	Name  string
	Paths []string
	Write bool
}

// Result is the guardrail decision. Allow is false when the action is blocked.
type Result struct {
	Allow   bool
	Message string
}

// Binding is an agent KIKI started in a worktree.
// ReadOnly means that worktree belongs to a repository the agent must not change.
type Binding struct {
	AgentID  string
	Worktree string
	ReadOnly bool
}

// Query is the information available when a hook asks for a decision.
type Query struct {
	Cwd       string
	Agent     string
	Bindings  []Binding
	Policies  []Policy
	Workspace []string
}

// Status is how an agent was chosen for a hook.
type Status int

const (
	// StatusOutside means this folder is not one of the app's repositories.
	StatusOutside Status = iota
	// StatusUse means Policy is the agent whose setup decides.
	StatusUse
	// StatusDeny means the action is blocked before a policy is applied.
	StatusDeny
)

// Resolution is the agent selected for a hook, or the reason none can be used.
type Resolution struct {
	Status  Status
	Policy  Policy
	Message string
}

// Resolve picks the agent whose setup applies to this folder.
// A named agent wins, then the agent KIKI started in this worktree,
// then the only enabled agent that includes the folder.
func Resolve(q Query) Resolution {
	cwd := absPath(q.Cwd)
	if id := strings.TrimSpace(q.Agent); id != "" {
		policy, ok := policyByID(q.Policies, id)
		if !ok {
			return Resolution{Status: StatusDeny, Message: "Guardrail: agent " + id + " is not available."}
		}
		if insideAny(policy.Roots, cwd) || bindingFor(q.Bindings, id, cwd) {
			if extra, readonly, ok := bindingWorktree(q.Bindings, id, cwd); ok {
				policy = policy.withRoot(extra, readonly)
			}
			return Resolution{Status: StatusUse, Policy: policy}
		}
		return Resolution{Status: StatusDeny, Message: "Guardrail: agent " + id + " is not allowed to use this folder."}
	}
	if binding, ok := latestBinding(q.Bindings, cwd); ok {
		policy, found := policyByID(q.Policies, binding.AgentID)
		if !found {
			return Resolution{Status: StatusDeny, Message: "Guardrail: agent " + binding.AgentID + " is not available."}
		}
		return Resolution{Status: StatusUse, Policy: policy.withRoot(binding.Worktree, binding.ReadOnly)}
	}
	var matched []Policy
	for _, policy := range q.Policies {
		if insideAny(policy.Roots, cwd) {
			matched = append(matched, policy)
		}
	}
	switch len(matched) {
	case 1:
		return Resolution{Status: StatusUse, Policy: matched[0]}
	case 0:
		if insideAny(q.Workspace, cwd) {
			return Resolution{Status: StatusDeny, Message: "Guardrail: no agent is set up to use this repository."}
		}
		return Resolution{Status: StatusOutside}
	default:
		ids := make([]string, 0, len(matched))
		for _, policy := range matched {
			ids = append(ids, policy.AgentID)
		}
		sort.Strings(ids)
		return Resolution{
			Status:  StatusDeny,
			Message: "Guardrail: KIKI cannot tell which agent is running in this repository (" + strings.Join(ids, ", ") + "). Start that agent from KIKI, or set KIKI_AGENT.",
		}
	}
}

// Decide allows the action only when every target is inside this policy.
// An empty target list is an input or output with no MCP, tool, repository, or agent interaction.
func (p Policy) Decide(cwd, home string, targets []Target) Result {
	if len(targets) == 0 {
		return Result{Allow: true}
	}
	cwd = absPath(cwd)
	for _, target := range targets {
		if result := p.veto(cwd, home, target); !result.Allow {
			return result
		}
	}
	return Result{Allow: true}
}

func (p Policy) veto(cwd, home string, target Target) Result {
	switch target.Kind {
	case "mcp":
		if p.allowsMCP(target.Name) {
			return Result{Allow: true}
		}
		label := strings.TrimSpace(target.Name)
		if label == "" {
			label = "an unnamed MCP server"
		}
		return Result{Message: "Guardrail: this agent cannot use the MCP server " + label + "."}
	case "tool":
		if strings.EqualFold(strings.TrimSpace(target.Name), "rtk") {
			if p.RTK {
				return Result{Allow: true}
			}
			return Result{Message: "Guardrail: RTK is off."}
		}
		if p.allowsTool(target.Name) {
			return Result{Allow: true}
		}
		label := strings.TrimSpace(target.Name)
		if label == "" {
			label = "this tool"
		}
		return Result{Message: "Guardrail: this agent cannot use the tool " + label + "."}
	case "agent":
		if p.allowsAgent(target.Name) {
			return Result{Allow: true}
		}
		label := strings.TrimSpace(target.Name)
		if label == "" {
			label = "another agent"
		}
		return Result{Message: "Guardrail: this agent cannot work with " + label + "."}
		case "repo":
			if result := p.vetoPaths(cwd, home, target.Paths); !result.Allow {
				return result
			}
			if target.Write {
				return p.vetoReadOnly(cwd, home, target.Paths)
			}
			return Result{Allow: true}
	default:
		return Result{Message: "Guardrail: this action is not one this agent can take."}
	}
}

func (p Policy) vetoPaths(cwd, home string, paths []string) Result {
	if len(paths) == 0 {
		if p.inside(cwd) {
			return Result{Allow: true}
		}
		return Result{Message: "Guardrail: this agent cannot work in " + cwd + ". It is outside the repositories set for this agent."}
	}
	for _, path := range paths {
		if strings.Contains(path, "://") {
			continue
		}
		full := expandPath(path, cwd, home)
		if full == "" || full == "." {
			continue
		}
		if !p.inside(full) {
			return Result{Message: "Guardrail: this agent cannot use " + full + ". It is outside the repositories set for this agent."}
		}
	}
	return Result{Allow: true}
}

func (p Policy) vetoReadOnly(cwd, home string, paths []string) Result {
	if len(p.ReadOnly) == 0 {
		return Result{Allow: true}
	}
	if insideAny(p.ReadOnly, cwd) {
		return Result{Message: "Guardrail: this agent can only read " + cwd + ". That repository is readonly."}
	}
	for _, path := range paths {
		if strings.Contains(path, "://") {
			continue
		}
		full := expandPath(path, cwd, home)
		if full == "" || full == "." {
			continue
		}
		if insideAny(p.ReadOnly, full) {
			return Result{Message: "Guardrail: this agent can only read " + full + ". That repository is readonly."}
		}
	}
	return Result{Allow: true}
}

func (p Policy) inside(path string) bool {
	return insideAny(p.Roots, path)
}

func (p Policy) allowsMCP(name string) bool {
	name = strings.ToLower(strings.TrimSpace(name))
	if name == "" {
		return false
	}
	product := ""
	if strings.Contains(name, "confluence") {
		product = "confluence"
	} else if strings.Contains(name, "jira") {
		product = "jira"
	}
	if strings.Contains(name, "atlassian") || product != "" {
		if product == "" {
			return p.has(p.MCPs, "atlassian") || p.has(p.MCPs, "atlassian_jira") || p.has(p.MCPs, "atlassian_confluence")
		}
		return p.has(p.MCPs, "atlassian") || p.has(p.MCPs, "atlassian_"+product) || p.has(p.MCPs, product)
	}
	for _, id := range p.MCPs {
		id = strings.ToLower(strings.TrimSpace(id))
		if id == "" || id == "atlassian" || strings.HasPrefix(id, "atlassian_") {
			continue
		}
		if name == id || hasToken(name, id) {
			return true
		}
		if p.Workspace != "" {
			key := "mcp-" + id + "-" + strings.ToLower(p.Workspace)
			if name == key || strings.Contains(name, key) {
				return true
			}
		}
	}
	return false
}

func (p Policy) allowsTool(name string) bool {
	switch strings.ToLower(strings.TrimSpace(name)) {
	case "gh", "github_cli", "github cli":
		name = "github_cli"
	}
	return p.has(p.Tools, name)
}

func (p Policy) allowsAgent(name string) bool {
	name = strings.TrimSpace(name)
	if name == "" {
		return false
	}
	if strings.EqualFold(name, p.AgentID) {
		return true
	}
	return p.has(p.Agents, name)
}

func (p Policy) has(list []string, id string) bool {
	id = strings.ToLower(strings.TrimSpace(id))
	if id == "" {
		return false
	}
	for _, item := range list {
		if strings.ToLower(strings.TrimSpace(item)) == id {
			return true
		}
	}
	return false
}

func (p Policy) withRoot(extra string, readonly bool) Policy {
	extra = absPath(extra)
	if extra == "" {
		return p
	}
	roots := make([]string, len(p.Roots), len(p.Roots)+1)
	copy(roots, p.Roots)
	p.Roots = append(roots, extra)
	if !readonly {
		return p
	}
	reads := make([]string, len(p.ReadOnly), len(p.ReadOnly)+1)
	copy(reads, p.ReadOnly)
	p.ReadOnly = append(reads, extra)
	return p
}

func policyByID(list []Policy, id string) (Policy, bool) {
	for _, policy := range list {
		if strings.EqualFold(policy.AgentID, id) {
			return policy, true
		}
	}
	return Policy{}, false
}

func latestBinding(list []Binding, cwd string) (Binding, bool) {
	for i := len(list) - 1; i >= 0; i-- {
		if Inside(list[i].Worktree, cwd) {
			return list[i], true
		}
	}
	return Binding{}, false
}

func bindingFor(list []Binding, id, cwd string) bool {
	_, _, ok := bindingWorktree(list, id, cwd)
	return ok
}

func bindingWorktree(list []Binding, id, cwd string) (string, bool, bool) {
	for i := len(list) - 1; i >= 0; i-- {
		if strings.EqualFold(list[i].AgentID, id) && Inside(list[i].Worktree, cwd) {
			return list[i].Worktree, list[i].ReadOnly, true
		}
	}
	return "", false, false
}

func insideAny(roots []string, path string) bool {
	for _, root := range roots {
		if Inside(root, path) {
			return true
		}
	}
	return false
}

// Inside reports whether path is the root or a file under it.
func Inside(root, path string) bool {
	root = absPath(root)
	path = absPath(path)
	if root == "" || path == "" {
		return false
	}
	if lexicalInside(resolveExisting(root), resolveExisting(path)) {
		return true
	}
	return lexicalInside(root, path)
}

func lexicalInside(root, path string) bool {
	root = filepath.Clean(root)
	path = filepath.Clean(path)
	if root == "." || path == "." || root == "" || path == "" {
		return false
	}
	if path == root {
		return true
	}
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

func expandPath(path, cwd, home string) string {
	path = strings.TrimSpace(path)
	if path == "" {
		return ""
	}
	if path == "~" && home != "" {
		return absPath(home)
	}
	if strings.HasPrefix(path, "~/") && home != "" {
		path = filepath.Join(home, path[2:])
	}
	if !filepath.IsAbs(path) {
		if cwd == "" {
			return ""
		}
		path = filepath.Join(cwd, path)
	}
	return absPath(path)
}

func absPath(path string) string {
	path = filepath.Clean(strings.TrimSpace(path))
	if path == "" || path == "." {
		return ""
	}
	if !filepath.IsAbs(path) {
		return ""
	}
	return path
}

func resolveExisting(path string) string {
	path = filepath.Clean(path)
	suffix := make([]string, 0)
	cur := path
	for {
		if _, err := os.Stat(cur); err == nil {
			if real, err := filepath.EvalSymlinks(cur); err == nil {
				cur = real
			}
			parts := append([]string{cur}, suffix...)
			return filepath.Clean(filepath.Join(parts...))
		}
		parent := filepath.Dir(cur)
		if parent == cur {
			return path
		}
		suffix = append([]string{filepath.Base(cur)}, suffix...)
		cur = parent
	}
}

func hasToken(name, id string) bool {
	for _, part := range strings.FieldsFunc(name, func(r rune) bool {
		return (r < 'a' || r > 'z') && (r < '0' || r > '9')
	}) {
		if part == id {
			return true
		}
	}
	return false
}
