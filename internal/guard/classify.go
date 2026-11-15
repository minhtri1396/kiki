package guard

import (
	"encoding/json"
	"regexp"
	"strings"
)

var mcpMention = regexp.MustCompile(`(?i)mcp[-_][a-z0-9][a-z0-9_-]*`)

// Classify reads a hook payload and returns the MCP, tool, repository, and agent
// interactions it is about to perform. A prompt or model message with no such
// interaction returns no targets.
func Classify(event string, raw []byte) []Target {
	m := object(raw)
	event = strings.ToLower(strings.TrimSpace(event))
	if event == "" {
		event = strings.ToLower(field(m, "hook_event_name", "hookEventName", "event", "hook_event"))
	}
	switch event {
	case "beforemcpexecution", "aftermcpexecution":
		return []Target{{Kind: "mcp", Name: mcpLabel(mcpServer(m), field(m, "tool_name", "toolName"))}}
	case "beforeshellexecution", "aftershellexecution":
		return shellTargets(field(m, "command", "cmd"), pathsOf(m), false)
	case "beforereadfile", "beforetabfileread":
		return []Target{{Kind: "repo", Paths: pathsOf(m)}}
	case "beforefileedit", "afterfileedit", "beforetabfileedit", "aftertabfileedit":
		return []Target{{Kind: "repo", Paths: pathsOf(m), Write: true}}
	case "subagentstart", "subagentstop":
		return []Target{{Kind: "agent", Name: agentName(m)}}
	case "pretooluse", "posttooluse", "posttoolusefailure", "beforetool", "aftertool", "permissionrequest":
		return toolTargets(m)
	case "beforesubmitprompt", "userpromptsubmit", "userpromptexpansion", "beforeagent", "beforemodel", "aftermodel", "afteragentresponse", "afteragentthought", "stop", "sessionstart", "sessionend":
		return nil
	default:
		if event == "" && !hasAction(m) {
			return nil
		}
		if cmd := field(m, "command", "cmd"); cmd != "" && field(m, "tool_name", "toolName") == "" && mcpServer(m) == "" {
			return shellTargets(cmd, pathsOf(m), false)
		}
		if server := mcpServer(m); server != "" {
			return []Target{{Kind: "mcp", Name: server}}
		}
		if field(m, "tool_name", "toolName") != "" {
			return toolTargets(m)
		}
		if paths := pathsOf(m); len(paths) > 0 {
			return []Target{{Kind: "repo", Paths: paths}}
		}
		return nil
	}
}

func toolTargets(m map[string]any) []Target {
	name := field(m, "tool_name", "toolName")
	switch actionKind(name) {
	case "mcp":
		return []Target{{Kind: "mcp", Name: mcpLabel(firstText(mcpServer(m), name), name)}}
	case "agent":
		return []Target{{Kind: "agent", Name: agentName(m)}}
	case "repo":
		return shellTargets(field(m, "command", "cmd"), pathsOf(m), repoToolWrites(name))
	default:
		return []Target{{Kind: "tool", Name: name}}
	}
}

func shellTargets(command string, extra []string, write bool) []Target {
	paths := append([]string{}, extra...)
	paths = append(paths, commandPaths(command)...)
	out := []Target{{Kind: "repo", Paths: paths, Write: write || commandWrites(command)}}
	for _, name := range mcpMentions(command) {
		out = append(out, Target{Kind: "mcp", Name: name})
	}
	if name, ok := launchedAgent(command); ok {
		out = append(out, Target{Kind: "agent", Name: name})
	}
	for _, name := range commandTools(command) {
		out = append(out, Target{Kind: "tool", Name: name})
	}
	return out
}

func commandTools(command string) []string {
	seen := map[string]bool{}
	var out []string
	add := func(name string) {
		if name == "" || seen[name] {
			return
		}
		seen[name] = true
		out = append(out, name)
	}
	for _, segment := range splitCommand(command) {
		base, rest := segmentCommand(tokenize(segment))
		switch base {
		case "gh":
			add("GitHub CLI")
		case "rtk":
			add("RTK")
			if next, _ := segmentCommand(rest); next == "gh" {
				add("GitHub CLI")
			}
		}
	}
	return out
}

func splitCommand(command string) []string {
	var out []string
	var b strings.Builder
	var quote rune
	flush := func() {
		text := strings.TrimSpace(b.String())
		b.Reset()
		if text != "" {
			out = append(out, text)
		}
	}
	for i := 0; i < len(command); i++ {
		r := rune(command[i])
		if quote != 0 {
			b.WriteByte(command[i])
			if r == quote {
				quote = 0
			}
			continue
		}
		if r == '\'' || r == '"' {
			quote = r
			b.WriteByte(command[i])
			continue
		}
		if r == '\n' || r == ';' {
			flush()
			continue
		}
		if r == '|' || (r == '&' && i+1 < len(command) && command[i+1] == '&') {
			flush()
			if r == '&' || (i+1 < len(command) && command[i+1] == '|') {
				i++
			}
			continue
		}
		b.WriteByte(command[i])
	}
	flush()
	return out
}

func segmentCommand(tokens []string) (string, []string) {
	for i := 0; i < len(tokens); i++ {
		tok := tokens[i]
		if isAssignment(tok) || isWrapper(baseName(tok)) {
			continue
		}
		if strings.HasPrefix(tok, "-") {
			continue
		}
		return baseName(tok), tokens[i+1:]
	}
	return "", nil
}

func isAssignment(tok string) bool {
	eq := strings.IndexByte(tok, '=')
	if eq <= 0 || strings.HasPrefix(tok, "-") {
		return false
	}
	return !strings.ContainsAny(tok[:eq], `/\`)
}

func isWrapper(name string) bool {
	switch name {
	case "sudo", "command", "env", "time", "nice", "nohup", "exec":
		return true
	default:
		return false
	}
}

func baseName(tok string) string {
	tok = strings.Trim(strings.TrimSpace(tok), `"'`)
	if tok == "" {
		return ""
	}
	if i := strings.LastIndexAny(tok, `/\`); i >= 0 {
		tok = tok[i+1:]
	}
	return strings.ToLower(tok)
}

func actionKind(name string) string {
	n := strings.ToLower(strings.TrimSpace(name))
	switch {
	case n == "":
		return "tool"
	case isMCP(n):
		return "mcp"
	case isAgentTool(n):
		return "agent"
	case isRepoTool(n):
		return "repo"
	default:
		return "tool"
	}
}

func isMCP(name string) bool {
	return strings.Contains(name, "mcp__") || strings.Contains(name, "mcp_") || strings.Contains(name, "mcp-") || strings.HasPrefix(name, "mcp:")
}

func isAgentTool(name string) bool {
	switch name {
	case "task", "agent", "spawn_agent", "subagent":
		return true
	default:
		return false
	}
}

func isRepoTool(name string) bool {
	switch name {
	case "shell", "bash", "read", "write", "edit", "delete", "grep", "glob", "multiedit",
		"apply_patch", "exec_command", "run_shell_command", "read_file", "write_file", "replace",
		"list_directory", "list_dir", "grep_search", "editnotebook", "notebookedit", "strreplace",
		"search_replace", "delete_file", "read_many_files", "ls":
		return true
	default:
		return false
	}
}

func hasAction(m map[string]any) bool {
	return field(m, "tool_name", "toolName", "command", "cmd") != "" || mcpServer(m) != "" || agentName(m) != "" || len(pathsOf(m)) > 0
}

func mcpServer(m map[string]any) string {
	return field(m, "server", "server_name", "serverName", "mcp_server", "mcpServer", "mcp_server_name")
}

func agentName(m map[string]any) string {
	return field(m, "subagent_type", "subagentType", "agent_type", "agentType", "agent_id", "agentId")
}

func object(raw []byte) map[string]any {
	var m map[string]any
	if err := json.Unmarshal(raw, &m); err != nil || m == nil {
		return map[string]any{}
	}
	return m
}

func field(m map[string]any, keys ...string) string {
	if s := lookup(m, keys...); s != "" {
		return s
	}
	for _, nest := range []string{"tool_input", "toolInput", "arguments", "input"} {
		child, ok := m[nest].(map[string]any)
		if !ok {
			continue
		}
		if s := lookup(child, keys...); s != "" {
			return s
		}
	}
	return ""
}

func lookup(m map[string]any, keys ...string) string {
	for _, key := range keys {
		text, ok := m[key].(string)
		if ok && strings.TrimSpace(text) != "" {
			return strings.TrimSpace(text)
		}
	}
	return ""
}

func pathsOf(m map[string]any) []string {
	keys := []string{"file_path", "filePath", "path", "target_file", "target_notebook", "notebook_path", "paths", "files"}
	var out []string
	collect := func(src map[string]any) {
		for _, key := range keys {
			out = append(out, stringsFrom(src[key])...)
		}
	}
	collect(m)
	for _, nest := range []string{"tool_input", "toolInput", "arguments", "input"} {
		if child, ok := m[nest].(map[string]any); ok {
			collect(child)
		}
	}
	return out
}

func stringsFrom(value any) []string {
	switch t := value.(type) {
	case string:
		if strings.TrimSpace(t) != "" {
			return []string{strings.TrimSpace(t)}
		}
	case []any:
		var out []string
		for _, item := range t {
			if text, ok := item.(string); ok && strings.TrimSpace(text) != "" {
				out = append(out, strings.TrimSpace(text))
			}
		}
		return out
	}
	return nil
}

func commandPaths(command string) []string {
	tokens := tokenize(command)
	var paths []string
	for i, tok := range tokens {
		if tok == "cd" && i+1 < len(tokens) {
			paths = append(paths, tokens[i+1])
			continue
		}
		if strings.HasPrefix(tok, ">") || strings.HasPrefix(tok, "<") {
			target := strings.TrimLeft(tok, "<>")
			if target != "" {
				paths = append(paths, target)
			}
			continue
		}
		if isPathToken(tok) {
			paths = append(paths, tok)
		}
	}
	return paths
}

func tokenize(command string) []string {
	var out []string
	var b strings.Builder
	var quote rune
	flush := func() {
		if b.Len() == 0 {
			return
		}
		out = append(out, b.String())
		b.Reset()
	}
	for _, r := range command {
		switch {
		case quote != 0:
			if r == quote {
				quote = 0
				continue
			}
			b.WriteRune(r)
		case r == '\'' || r == '"':
			quote = r
		case r == ' ' || r == '\t' || r == '\n' || r == '\r':
			flush()
		default:
			b.WriteRune(r)
		}
	}
	flush()
	return out
}

func isPathToken(tok string) bool {
	if tok == "" || strings.Contains(tok, "://") {
		return false
	}
	return strings.HasPrefix(tok, "/") || strings.HasPrefix(tok, "./") || strings.HasPrefix(tok, "../") || strings.HasPrefix(tok, "~/") || tok == "~"
}

func mcpMentions(command string) []string {
	found := mcpMention.FindAllString(command, -1)
	if len(found) == 0 {
		return nil
	}
	seen := map[string]bool{}
	var out []string
	for _, name := range found {
		name = strings.ToLower(name)
		if seen[name] {
			continue
		}
		seen[name] = true
		out = append(out, name)
	}
	return out
}

func launchedAgent(command string) (string, bool) {
	low := strings.ToLower(command)
	if !strings.Contains(low, "orca") || !strings.Contains(low, "launch") {
		return "", false
	}
	tokens := tokenize(command)
	for i, tok := range tokens {
		if tok == "--agent" && i+1 < len(tokens) {
			return tokens[i+1], true
		}
	}
	return "", true
}

func firstText(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return strings.TrimSpace(value)
		}
	}
	return ""
}

func mcpLabel(server, tool string) string {
	server = strings.TrimSpace(server)
	tool = strings.TrimSpace(tool)
	if server == "" || server == tool || strings.Contains(tool, server) {
		return tool
	}
	if tool == "" {
		return server
	}
	return server + " " + tool
}

func repoToolWrites(name string) bool {
	switch canonicalTool(name) {
	case "shell", "bash", "execcommand", "runshellcommand",
		"read", "grep", "glob", "listdirectory", "listdir", "grepsearch", "readfile", "readmanyfiles", "ls":
		return false
	default:
		return true
	}
}

func canonicalTool(name string) string {
	name = strings.ToLower(strings.TrimSpace(name))
	name = strings.ReplaceAll(name, "_", "")
	name = strings.ReplaceAll(name, "-", "")
	return name
}

func commandWrites(command string) bool {
	if strings.TrimSpace(command) == "" {
		return false
	}
	if hasFileRedirect(command) {
		return true
	}
	for _, segment := range splitCommand(command) {
		if segmentWrites(tokenize(segment)) {
			return true
		}
	}
	return false
}

func hasFileRedirect(command string) bool {
	for i := 0; i < len(command); i++ {
		if command[i] != '>' {
			continue
		}
		if i+1 < len(command) && command[i+1] == '&' {
			continue
		}
		return true
	}
	return false
}

func segmentWrites(tokens []string) bool {
	base, rest := segmentCommand(tokens)
	switch base {
	case "", "gh":
		return false
	case "rtk":
		return segmentWrites(rest)
	case "git":
		return gitWrites(rest)
	case "sed":
		return sedWrites(rest)
	case "find":
		return findWrites(rest)
	case "codegraph":
		return codegraphWrites(rest)
	default:
		return !readOnlyCommand(base)
	}
}

func readOnlyCommand(name string) bool {
	switch name {
	case "ls", "cat", "head", "tail", "less", "more", "wc", "file", "stat", "pwd",
		"whoami", "id", "date", "uname", "which", "basename", "dirname", "realpath",
		"readlink", "diff", "cmp", "comm", "sort", "uniq", "cut", "tr", "nl", "od",
		"hexdump", "strings", "du", "tree", "bat", "rg", "grep", "egrep", "fgrep",
		"ag", "awk", "echo", "printf", "true", "false", "test", "[", ":", "type",
		"column", "fold", "tac", "rev", "shasum", "sha256sum", "md5", "md5sum",
		"cksum", "expr", "seq", "printenv":
		return true
	default:
		return false
	}
}

func gitWrites(args []string) bool {
	sub, rest := gitSubcommand(args)
	switch sub {
	case "", "status", "log", "diff", "show", "blame", "rev-parse", "ls-files", "ls-tree",
		"rev-list", "describe", "name-rev", "shortlog", "cat-file", "grep", "check-ignore",
		"merge-base", "help", "version", "var":
		return false
	case "branch":
		return gitBranchWrites(rest)
	case "remote":
		return gitRemoteWrites(rest)
	case "config":
		return gitConfigWrites(rest)
	case "stash":
		return gitStashWrites(rest)
	case "tag":
		return gitTagWrites(rest)
	default:
		return true
	}
}

func gitSubcommand(args []string) (string, []string) {
	for i := 0; i < len(args); i++ {
		tok := args[i]
		if tok == "--" {
			if i+1 < len(args) {
				return args[i+1], args[i+2:]
			}
			return "", nil
		}
		if strings.HasPrefix(tok, "-") {
			if gitOptionTakesArg(tok) && !strings.Contains(tok, "=") {
				i++
			}
			continue
		}
		return tok, args[i+1:]
	}
	return "", nil
}

func gitOptionTakesArg(tok string) bool {
	switch tok {
	case "-C", "-c", "--git-dir", "--work-tree", "--namespace", "--super-prefix", "--config-env":
		return true
	default:
		return false
	}
}

func gitBranchWrites(args []string) bool {
	for _, arg := range args {
		if arg == "--" {
			continue
		}
		if strings.HasPrefix(arg, "-") {
			switch arg {
			case "-d", "-D", "-m", "-M", "-c", "-C", "-u", "--delete", "--move", "--copy", "--force", "--edit-description", "--unset-upstream":
				return true
			}
			if strings.HasPrefix(arg, "--set-upstream-to") || strings.HasPrefix(arg, "--delete") {
				return true
			}
			continue
		}
		return true
	}
	return false
}

func gitRemoteWrites(args []string) bool {
	sub := ""
	for _, arg := range args {
		if strings.HasPrefix(arg, "-") {
			continue
		}
		sub = arg
		break
	}
	switch sub {
	case "", "show", "get-url":
		return false
	default:
		return true
	}
}

func gitConfigWrites(args []string) bool {
	positional := 0
	for _, arg := range args {
		switch arg {
		case "--get", "--list", "-l", "--get-regexp", "--get-all", "--get-urlmatch":
			return false
		}
		if !strings.HasPrefix(arg, "-") {
			positional++
		}
	}
	return positional > 0
}

func gitStashWrites(args []string) bool {
	for _, arg := range args {
		if strings.HasPrefix(arg, "-") {
			continue
		}
		return arg != "list" && arg != "show"
	}
	return true
}

func gitTagWrites(args []string) bool {
	for _, arg := range args {
		if arg == "-l" || arg == "--list" || strings.HasPrefix(arg, "--list=") {
			return false
		}
	}
	for _, arg := range args {
		if !strings.HasPrefix(arg, "-") {
			return true
		}
	}
	return false
}

func sedWrites(args []string) bool {
	for _, arg := range args {
		if arg == "--in-place" || strings.HasPrefix(arg, "--in-place=") {
			return true
		}
		if strings.HasPrefix(arg, "-") && !strings.HasPrefix(arg, "--") && strings.Contains(strings.TrimLeft(arg, "-"), "i") {
			return true
		}
	}
	return false
}

func findWrites(args []string) bool {
	for _, arg := range args {
		switch arg {
		case "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fls", "-fprint", "-fprintf", "-fprint0":
			return true
		}
	}
	return false
}

func codegraphWrites(args []string) bool {
	for _, arg := range args {
		if strings.HasPrefix(arg, "-") {
			continue
		}
		switch arg {
		case "explore", "query", "status", "callers", "callees", "impact", "affected", "node", "files", "search", "help", "version":
			return false
		default:
			return true
		}
	}
	return false
}
