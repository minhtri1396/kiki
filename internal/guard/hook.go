package guard

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Run is the `kiki guardrail <tool>` command. AI tools start it before input and output.
func Run(tool string) int {
	raw, _ := io.ReadAll(io.LimitReader(os.Stdin, 1<<20))
	home, err := os.UserHomeDir()
	if err != nil {
		fmt.Fprint(os.Stdout, Reply(tool, "", true, ""))
		return 0
	}
	fmt.Fprint(os.Stdout, Gate(home, tool, raw, os.Getenv("KIKI_AGENT"), postGuard))
	return 0
}

// Gate asks the running app about this hook. A folder that is not a KIKI repository is allowed.
// When the app is not running, the action is allowed. The app decides only while it is up.
func Gate(home, tool string, raw []byte, agent string, post func(url string, body []byte) (bool, string, error)) string {
	event, cwd, paths := inspect(raw)
	eps, err := Load(home)
	if err != nil || len(eps) == 0 {
		return Reply(tool, event, true, "")
	}
	ep, owned := findOwner(eps, cwd, paths)
	if !owned {
		return Reply(tool, event, true, "")
	}
	if cwd == "" {
		cwd = filepath.Dir(firstOwned(ep, paths))
	}
	payload, err := json.Marshal(map[string]any{
		"tool":  tool,
		"agent": agent,
		"cwd":   cwd,
		"event": event,
		"input": json.RawMessage(Reduce(raw)),
	})
	if err != nil {
		return Reply(tool, event, true, "")
	}
	allow, message, err := post(ep.URL, payload)
	if err != nil {
		return Reply(tool, event, true, "")
	}
	return Reply(tool, event, allow, message)
}

// Reduce keeps the fields the guardrail decides on and drops file contents and prompts.
func Reduce(raw []byte) []byte {
	m := object(raw)
	kept := map[string]any{}
	copyFields(kept, m)
	for _, nest := range []string{"tool_input", "toolInput", "arguments", "input"} {
		child, ok := m[nest].(map[string]any)
		if !ok {
			continue
		}
		inner := map[string]any{}
		copyFields(inner, child)
		if len(inner) > 0 {
			kept[nest] = inner
		}
	}
	if roots, ok := m["workspace_roots"].([]any); ok {
		kept["workspace_roots"] = roots
	}
	out, err := json.Marshal(kept)
	if err != nil {
		return []byte("{}")
	}
	return out
}

func copyFields(dst, src map[string]any) {
	keys := []string{
		"hook_event_name", "hookEventName", "event", "hook_event",
		"cwd", "workspaceRoot", "working_directory",
		"tool_name", "toolName",
		"command", "cmd",
		"server", "server_name", "serverName", "mcp_server", "mcpServer", "mcp_server_name",
		"subagent_type", "subagentType", "agent_type", "agentType", "agent_id", "agentId",
		"file_path", "filePath", "path", "target_file", "target_notebook", "notebook_path",
		"paths", "files",
	}
	for _, key := range keys {
		value, ok := src[key]
		if !ok {
			continue
		}
		if text, ok := value.(string); ok && len(text) > 8000 {
			value = text[:8000]
		}
		dst[key] = value
	}
}

func inspect(raw []byte) (event, cwd string, paths []string) {
	m := object(raw)
	event = field(m, "hook_event_name", "hookEventName", "event", "hook_event")
	cwd = field(m, "cwd", "workspaceRoot", "working_directory")
	if !filepath.IsAbs(cwd) {
		cwd = ""
	}
	if cwd == "" {
		if roots, ok := m["workspace_roots"].([]any); ok {
			for _, item := range roots {
				text, ok := item.(string)
				if ok && filepath.IsAbs(strings.TrimSpace(text)) {
					cwd = strings.TrimSpace(text)
					break
				}
			}
		}
	}
	paths = absolutePaths(m, 0)
	return event, cwd, paths
}

func absolutePaths(value any, depth int) []string {
	if depth > 6 {
		return nil
	}
	var out []string
	switch t := value.(type) {
	case map[string]any:
		for _, item := range t {
			out = append(out, absolutePaths(item, depth+1)...)
		}
	case []any:
		for _, item := range t {
			out = append(out, absolutePaths(item, depth+1)...)
		}
	case string:
		text := strings.TrimSpace(t)
		if filepath.IsAbs(text) && !strings.Contains(text, "://") && len(text) < 4096 {
			out = append(out, text)
		}
	}
	return out
}

func findOwner(eps []Endpoint, cwd string, paths []string) (Endpoint, bool) {
	for _, ep := range eps {
		if cwd != "" && endpointOwns(ep, cwd) {
			return ep, true
		}
	}
	for _, ep := range eps {
		if firstOwned(ep, paths) != "" {
			return ep, true
		}
	}
	return Endpoint{}, false
}

func endpointOwns(ep Endpoint, path string) bool {
	for _, root := range ep.Paths {
		if Inside(root, path) {
			return true
		}
	}
	return false
}

func firstOwned(ep Endpoint, paths []string) string {
	for _, path := range paths {
		if endpointOwns(ep, path) {
			return path
		}
	}
	return ""
}

func postGuard(base string, body []byte) (bool, string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimRight(base, "/")+"/api/guardrail", bytes.NewReader(body))
	if err != nil {
		return false, "", err
	}
	req.Header.Set("Content-Type", "application/json")
	res, err := (&http.Client{Timeout: 8 * time.Second}).Do(req)
	if err != nil {
		return false, "", err
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		return false, "", fmt.Errorf("status %d", res.StatusCode)
	}
	var payload struct {
		Allow   bool   `json:"allow"`
		Message string `json:"message"`
	}
	if err := json.NewDecoder(res.Body).Decode(&payload); err != nil {
		return false, "", err
	}
	return payload.Allow, payload.Message, nil
}
