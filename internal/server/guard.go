package server

import (
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strings"

	"kiki/internal/agentfile"
	"kiki/internal/console"
	"kiki/internal/guard"
	"kiki/internal/tools"
)

func (s *Server) handleGuardrail(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Agent string          `json:"agent"`
		Cwd   string          `json:"cwd"`
		Event string          `json:"event"`
		Input json.RawMessage `json:"input"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	result := s.checkGuard(body.Agent, body.Cwd, body.Event, body.Input)
	writeJSON(w, http.StatusOK, map[string]any{
		"allow":   result.Allow,
		"message": result.Message,
	})
}

func (s *Server) ensureAgentHooks(tool string) error {
	tool = strings.TrimSpace(tool)
	if tool == "" {
		return nil
	}
	home, err := s.hookHome()
	if err != nil {
		return err
	}
	cfg, err := s.loadTools()
	if err != nil {
		return err
	}
	if cfg.GuardrailsOn() {
		exe, err := guardExecutable()
		if err != nil {
			return err
		}
		if err := guard.InstallTool(home, exe, tool); err != nil {
			return err
		}
		if err := guard.HooksReady(home, tool); err != nil {
			return err
		}
	}
	if !cfg.RTK {
		return nil
	}
	return s.ensureRTKHook(home, tool)
}

func (s *Server) ensureRTKHook(home, tool string) error {
	if err := tools.HookReady(home, tool); err == nil {
		return nil
	}
	if s.RTKHooks != nil {
		if err := s.RTKHooks(home, true); err != nil {
			return err
		}
	} else if err := tools.EnsureHook(home, tool); err != nil {
		return err
	}
	return tools.HookReady(home, tool)
}

func (s *Server) ensureAgentTools(prep launchPrep) error {
	ids := make([]string, 0, len(prep.agent.Tools))
	for _, item := range prep.agent.Tools {
		ids = append(ids, item.ID)
	}
	enabled, err := s.enabledTools()
	if err != nil {
		return err
	}
	return toolsReady(ids, enabled)
}

// EnableGuardrails publishes this app and installs or removes its AI tool hooks.
// Guardrails stay installed when the setting is missing. Turning them off removes the hooks.
func (s *Server) EnableGuardrails(addr string) error {
	home, err := s.hookHome()
	if err != nil {
		return err
	}
	exe, err := guardExecutable()
	if err != nil {
		return err
	}
	url := guard.ListenURL(addr)
	s.guardMu.Lock()
	s.guardHome = home
	s.guardURL = url
	bindings := append([]guard.Binding(nil), s.guardBindings...)
	s.guardMu.Unlock()
	if err := s.publishGuard(home, url, bindings); err != nil {
		return err
	}
	cfg, err := s.loadTools()
	if err != nil {
		return err
	}
	return s.applyGuardrails(home, exe, cfg.GuardrailsOn())
}

func (s *Server) syncGuardrails(on bool) error {
	home, err := s.hookHome()
	if err != nil {
		return err
	}
	exe := ""
	if on && s.GuardHooks == nil {
		exe, err = guardExecutable()
		if err != nil {
			return err
		}
	}
	return s.applyGuardrails(home, exe, on)
}

func (s *Server) applyGuardrails(home, exe string, on bool) error {
	if s.GuardHooks != nil {
		return s.GuardHooks(home, on)
	}
	if !on {
		return guard.Remove(home)
	}
	return guard.Install(home, exe)
}

func (s *Server) hookHome() (string, error) {
	if home := strings.TrimSpace(s.MCPHome); home != "" {
		return home, nil
	}
	return os.UserHomeDir()
}

func guardExecutable() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", err
	}
	if resolved, err := filepath.EvalSymlinks(exe); err == nil {
		exe = resolved
	}
	return exe, nil
}

func (s *Server) bindGuard(agentID, worktree string, readonly bool, services []string) {
	agentID = strings.TrimSpace(agentID)
	worktree = strings.TrimSpace(worktree)
	if agentID == "" || worktree == "" || !filepath.IsAbs(worktree) {
		return
	}
	s.guardMu.Lock()
	s.guardBindings = append(s.guardBindings, guard.Binding{
		AgentID:  agentID,
		Worktree: filepath.Clean(worktree),
		ReadOnly: readonly,
		Services: append([]string(nil), services...),
	})
	if len(s.guardBindings) > 40 {
		s.guardBindings = append([]guard.Binding(nil), s.guardBindings[len(s.guardBindings)-40:]...)
	}
	home := s.guardHome
	url := s.guardURL
	bindings := append([]guard.Binding(nil), s.guardBindings...)
	s.guardMu.Unlock()
	if home == "" || url == "" {
		return
	}
	_ = s.publishGuard(home, url, bindings)
}

func (s *Server) publishGuard(home, url string, bindings []guard.Binding) error {
	snap, err := s.load()
	if err != nil {
		return err
	}
	paths := make([]string, 0, len(snap.Repos)+len(bindings))
	for _, repo := range snap.Repos {
		folder := strings.TrimSpace(repo.Folder)
		if filepath.IsAbs(folder) {
			paths = append(paths, filepath.Clean(folder))
		}
	}
	for _, binding := range bindings {
		if filepath.IsAbs(binding.Worktree) {
			paths = append(paths, filepath.Clean(binding.Worktree))
		}
	}
	return guard.Publish(home, guard.Endpoint{
		URL:       url,
		Workspace: snap.WorkspaceID,
		Paths:     paths,
	})
}

func (s *Server) checkGuard(agent, cwd, event string, input []byte) guard.Result {
	q, err := s.guardQuery()
	if err != nil {
		return guard.Result{Message: "Guardrail: KIKI could not read the agent setup, so this action is blocked."}
	}
	q.Agent = agent
	q.Cwd = cwd
	resolved := guard.Resolve(q)
	switch resolved.Status {
	case guard.StatusOutside:
		return guard.Result{Allow: true}
	case guard.StatusDeny:
		return guard.Result{Message: resolved.Message}
	}
	home, _ := os.UserHomeDir()
	if s.MCPHome != "" {
		home = s.MCPHome
	}
	return resolved.Policy.Decide(cwd, home, guard.Classify(event, input))
}

func (s *Server) guardQuery() (guard.Query, error) {
	snap, err := s.load()
	if err != nil {
		return guard.Query{}, err
	}
	dir, err := s.agentDir()
	if err != nil {
		return guard.Query{}, err
	}
	files, err := agentfile.List(dir)
	if err != nil {
		return guard.Query{}, err
	}
	s.guardMu.Lock()
	bindings := append([]guard.Binding(nil), s.guardBindings...)
	s.guardMu.Unlock()
	cfg, err := s.loadTools()
	if err != nil {
		return guard.Query{}, err
	}
	enabled := map[string]bool{}
	for _, item := range toolsFrom(cfg) {
		enabled[item.ID] = true
	}
	policies := make([]guard.Policy, 0, len(files))
	for _, file := range files {
		if !file.Enabled {
			continue
		}
		policies = append(policies, policyFrom(file, snap, enabled, cfg.RTK))
	}
	workspace := make([]string, 0, len(snap.Repos))
	for _, repo := range snap.Repos {
		folder := strings.TrimSpace(repo.Folder)
		if filepath.IsAbs(folder) {
			workspace = append(workspace, filepath.Clean(folder))
		}
	}
	return guard.Query{Bindings: bindings, Policies: policies, Workspace: workspace}, nil
}

func policyFrom(file agentfile.File, snap console.Snapshot, enabled map[string]bool, rtk bool) guard.Policy {
	policy := guard.Policy{
		AgentID:   file.ID,
		Workspace: snap.WorkspaceID,
		MCPs:      useIDs(file.MCPs),
		Tools:     enabledUseIDs(file.Tools, enabled),
		RTK:       rtk,
	}
	for _, peer := range file.WorksWith {
		if id := strings.TrimSpace(peer.ID); id != "" {
			policy.Agents = append(policy.Agents, id)
		}
	}
	for _, repo := range file.Repos {
		setup := snap.Repo(repo.Kind)
		folder := strings.TrimSpace(setup.Folder)
		if !filepath.IsAbs(folder) {
			continue
		}
		folder = filepath.Clean(folder)
		if setup.Sparse != "" {
			folder = filepath.Join(folder, filepath.FromSlash(setup.Sparse))
		}
		policy.Roots = append(policy.Roots, folder)
		if repo.Readonly {
			policy.ReadOnly = append(policy.ReadOnly, folder)
		}
		if len(setup.Services) > 0 {
			if policy.Services == nil {
				policy.Services = map[string][]string{}
			}
			policy.Services[folder] = append([]string(nil), setup.Services...)
		}
	}
	return policy
}

func enabledUseIDs(list []agentfile.Use, enabled map[string]bool) []string {
	out := make([]string, 0, len(list))
	for _, item := range list {
		id := strings.TrimSpace(item.ID)
		if id == "" || !enabled[id] {
			continue
		}
		out = append(out, id)
	}
	return out
}

func useIDs(list []agentfile.Use) []string {
	out := make([]string, 0, len(list))
	for _, item := range list {
		if id := strings.TrimSpace(item.ID); id != "" {
			out = append(out, id)
		}
	}
	return out
}
