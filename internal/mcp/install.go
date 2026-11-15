package mcp

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
)

var (
	mcpNamePattern = regexp.MustCompile(`^[a-z][a-z0-9_]*$`)
	workspaceKey   = regexp.MustCompile(`^[a-z][a-z0-9_-]{0,31}$`)
)

// ServerKey is the MCP server name written into an AI tool.
// The name is mcp-<mcp name>-<workspace id>, for example mcp-atlassian-kiki.
func ServerKey(name, workspace string) (string, error) {
	name = strings.TrimSpace(name)
	workspace = strings.TrimSpace(workspace)
	if !mcpNamePattern.MatchString(name) {
		return "", fmt.Errorf("MCP name is not valid")
	}
	if !workspaceKey.MatchString(workspace) {
		return "", fmt.Errorf("workspace must be lowercase letters, digits, hyphens, or underscores")
	}
	return "mcp-" + name + "-" + workspace, nil
}

// lookPath finds uvx. Tests replace it.
var lookPath = findUVX

// Ensure writes the Atlassian MCP server into the AI tool's user settings
// when that tool does not already have the key mcp-atlassian-<workspace>.
// A setup that is off does nothing. The same key in the launch folder, or in
// Claude's settings for that folder, counts as set up.
func Ensure(tool, worktree, home, workspace string, cfg Atlassian) error {
	cfg = cfg.Normalized()
	if !cfg.Ready() {
		return nil
	}
	if err := cfg.Validate(); err != nil {
		return err
	}
	tool = strings.TrimSpace(tool)
	home = strings.TrimSpace(home)
	if home == "" {
		return fmt.Errorf("home directory is not set")
	}
	key, err := ServerKey("atlassian", workspace)
	if err != nil {
		return err
	}
	if projectHasKey(tool, worktree, key) {
		return nil
	}
	switch tool {
	case "cursor":
		return installJSON(filepath.Join(home, ".cursor", "mcp.json"), tool, key, cfg)
	case "gemini":
		return installJSON(filepath.Join(home, ".gemini", "settings.json"), tool, key, cfg)
	case "claude":
		return installClaude(worktree, home, key, cfg)
	case "codex":
		return installCodex(filepath.Join(home, ".codex", "config.toml"), key, cfg)
	default:
		return fmt.Errorf("AI Tool is not supported")
	}
}

// projectHasKey reports that this launch folder already has the workspace MCP key.
// That server is available to the agent, so the user settings are left unchanged.
func projectHasKey(tool, worktree, key string) bool {
	worktree = strings.TrimSpace(worktree)
	if worktree == "" || key == "" {
		return false
	}
	var path string
	switch tool {
	case "cursor":
		path = filepath.Join(worktree, ".cursor", "mcp.json")
	case "gemini":
		path = filepath.Join(worktree, ".gemini", "settings.json")
	case "claude":
		path = filepath.Join(worktree, ".mcp.json")
	case "codex":
		path = filepath.Join(worktree, ".codex", "config.toml")
	default:
		return false
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return false
	}
	if tool == "codex" {
		return tomlHasKey(string(data), key)
	}
	obj, err := parseJSONObject(data)
	if err != nil {
		return false
	}
	has, err := serversHaveKey(obj, key)
	return err == nil && has
}

func installJSON(path, tool, key string, cfg Atlassian) error {
	obj, created, err := readObject(path)
	if err != nil {
		return fmt.Errorf("%s MCP settings: %w", toolLabel(tool), err)
	}
	if !created {
		has, err := serversHaveKey(obj, key)
		if err != nil {
			return fmt.Errorf("%s MCP settings: %w", toolLabel(tool), err)
		}
		if has {
			return nil
		}
	}
	if err := requireUVX(); err != nil {
		return err
	}
	if err := addAtlassianServer(obj, tool, key, cfg); err != nil {
		return fmt.Errorf("%s MCP settings: %w", toolLabel(tool), err)
	}
	if err := writePrivate(path, obj.pretty()); err != nil {
		return fmt.Errorf("%s MCP settings: %w", toolLabel(tool), err)
	}
	return nil
}

func installClaude(worktree, home, key string, cfg Atlassian) error {
	path := filepath.Join(home, ".claude.json")
	obj, created, err := readObject(path)
	if err != nil {
		return fmt.Errorf("Claude MCP settings: %w", err)
	}
	if !created {
		has, err := claudeHas(obj, worktree, key)
		if err != nil {
			return fmt.Errorf("Claude MCP settings: %w", err)
		}
		if has {
			return nil
		}
	}
	if err := requireUVX(); err != nil {
		return err
	}
	if err := addAtlassianServer(obj, "claude", key, cfg); err != nil {
		return fmt.Errorf("Claude MCP settings: %w", err)
	}
	if err := writePrivate(path, obj.pretty()); err != nil {
		return fmt.Errorf("Claude MCP settings: %w", err)
	}
	return nil
}

func installCodex(path, key string, cfg Atlassian) error {
	data, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("Codex MCP settings: %w", err)
	}
	if tomlHasKey(string(data), key) {
		return nil
	}
	if err := requireUVX(); err != nil {
		return err
	}
	block, err := codexBlock(key, cfg.Env())
	if err != nil {
		return err
	}
	next := data
	if len(bytes.TrimSpace(data)) == 0 {
		next = []byte(strings.TrimPrefix(block, "\n"))
	} else {
		if !bytes.HasSuffix(data, []byte("\n")) {
			next = append(next, '\n')
		}
		next = append(next, []byte(block)...)
	}
	if err := writePrivate(path, next); err != nil {
		return fmt.Errorf("Codex MCP settings: %w", err)
	}
	return nil
}

func addAtlassianServer(obj *jsonObject, tool, key string, cfg Atlassian) error {
	servers := newJSONObject()
	if raw, ok := obj.vals["mcpServers"]; ok && !isNull(raw) {
		parsed, err := parseJSONObject(raw)
		if err != nil {
			return fmt.Errorf("server list is not a JSON object")
		}
		servers = parsed
	}
	doc, err := serverDocument(tool, cfg.Env())
	if err != nil {
		return err
	}
	servers.set(key, doc)
	obj.set("mcpServers", servers.compact())
	return nil
}

func serversHaveKey(obj *jsonObject, key string) (bool, error) {
	raw, ok := obj.vals["mcpServers"]
	if !ok || isNull(raw) {
		return false, nil
	}
	servers, err := parseJSONObject(raw)
	if err != nil {
		return false, fmt.Errorf("server list is not a JSON object")
	}
	_, found := servers.vals[key]
	return found, nil
}

func claudeHas(obj *jsonObject, worktree, key string) (bool, error) {
	has, err := serversHaveKey(obj, key)
	if err != nil || has {
		return has, err
	}
	raw, ok := obj.vals["projects"]
	if !ok || isNull(raw) || strings.TrimSpace(worktree) == "" {
		return false, nil
	}
	projects, err := parseJSONObject(raw)
	if err != nil {
		return false, nil
	}
	want := filepath.Clean(worktree)
	for _, projectKey := range projects.keys {
		if filepath.Clean(projectKey) != want {
			continue
		}
		project, err := parseJSONObject(projects.vals[projectKey])
		if err != nil {
			return false, nil
		}
		return serversHaveKey(project, key)
	}
	return false, nil
}

func serverDocument(tool string, env map[string]string) ([]byte, error) {
	body := map[string]any{
		"command": "uvx",
		"args":    []string{"mcp-atlassian"},
		"env":     env,
	}
	if tool == "claude" {
		body["type"] = "stdio"
	}
	return json.Marshal(body)
}

func codexBlock(key string, env map[string]string) (string, error) {
	var b strings.Builder
	fmt.Fprintf(&b, "\n[mcp_servers.%s]\ncommand = \"uvx\"\nargs = [\"mcp-atlassian\"]\n", key)
	if len(env) == 0 {
		return b.String(), nil
	}
	fmt.Fprintf(&b, "\n[mcp_servers.%s.env]\n", key)
	keys := make([]string, 0, len(env))
	for key := range env {
		keys = append(keys, key)
	}
	slices.Sort(keys)
	for _, key := range keys {
		quoted, err := tomlBasic(env[key])
		if err != nil {
			return "", err
		}
		fmt.Fprintf(&b, "%s = %s\n", key, quoted)
	}
	return b.String(), nil
}

func tomlBasic(value string) (string, error) {
	for _, r := range value {
		if r < 0x20 {
			return "", fmt.Errorf("an Atlassian value contains a character that cannot be stored")
		}
	}
	value = strings.ReplaceAll(value, `\`, `\\`)
	value = strings.ReplaceAll(value, `"`, `\"`)
	return `"` + value + `"`, nil
}

func tomlHasKey(text, key string) bool {
	want := "mcp_servers." + strings.ToLower(key)
	for _, line := range strings.Split(text, "\n") {
		trim := strings.TrimSpace(line)
		if trim == "" || strings.HasPrefix(trim, "#") {
			continue
		}
		if !strings.HasPrefix(trim, "[") || !strings.HasSuffix(trim, "]") {
			continue
		}
		section := strings.ToLower(strings.Trim(strings.TrimSpace(trim[1:len(trim)-1]), `"'`))
		if section == want || section == want+".env" {
			return true
		}
	}
	return false
}

func toolLabel(tool string) string {
	switch tool {
	case "cursor":
		return "Cursor"
	case "claude":
		return "Claude"
	case "codex":
		return "Codex"
	case "gemini":
		return "Gemini"
	default:
		return "AI Tool"
	}
}

func requireUVX() error {
	if _, err := lookPath("uvx"); err != nil {
		return fmt.Errorf("uvx is not installed. Atlassian MCP runs with uvx mcp-atlassian")
	}
	return nil
}

func findUVX(name string) (string, error) {
	if path, err := exec.LookPath(name); err == nil {
		return path, nil
	}
	home, _ := os.UserHomeDir()
	for _, dir := range []string{"/opt/homebrew/bin", "/usr/local/bin", filepath.Join(home, ".local", "bin")} {
		candidate := filepath.Join(dir, name)
		info, err := os.Stat(candidate)
		if err == nil && !info.IsDir() && info.Mode()&0o111 != 0 {
			return candidate, nil
		}
	}
	return "", fmt.Errorf("uvx is not installed")
}

func writePrivate(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0o600); err != nil {
		return err
	}
	if err := os.Chmod(tmp, 0o600); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return os.Chmod(path, 0o600)
}
