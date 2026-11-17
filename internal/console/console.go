// Package console turns the saved setup into the prompt sent when an agent starts.
package console

import (
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"kiki/internal/agentdef"
	"kiki/internal/toolchain"
)

// OrcaAgents are the runtimes `orca agent launch --agent` accepts in this console.
var OrcaAgents = []string{"cursor", "claude", "codex", "gemini"}

// Orchestrator is one app that starts agents and groups their work.
type Orchestrator struct {
	ID    string `json:"id"`
	Label string `json:"label"`
}

// Orchestrators are the Agent Orchestration tools a user can choose.
// Only Orca is supported.
var Orchestrators = []Orchestrator{
	{ID: "orca", Label: "Orca"},
}

// NormalizeOrchestrator returns the saved tool, or Orca when none is set.
func NormalizeOrchestrator(id string) string {
	id = strings.TrimSpace(id)
	if id == "" {
		return "orca"
	}
	return id
}

// KnownOrchestrator reports whether id is a supported Agent Orchestration tool.
// An empty id is Orca.
func KnownOrchestrator(id string) bool {
	id = NormalizeOrchestrator(id)
	for _, item := range Orchestrators {
		if item.ID == id {
			return true
		}
	}
	return false
}

// OrchestratorLabel is the name shown for an Agent Orchestration tool.
func OrchestratorLabel(id string) string {
	id = NormalizeOrchestrator(id)
	for _, item := range Orchestrators {
		if item.ID == id {
			return item.Label
		}
	}
	return id
}

var (
	workspaceID = regexp.MustCompile(`^[a-z][a-z0-9_-]{0,31}$`)
	serviceName = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`)
)

// Repo is one app repository. Index is obsidian, codegraph, or empty when none is set.
// Sparse is a folder inside the git repository. Empty means the whole repository is checked out.
type Repo struct {
	Kind     string
	Folder   string
	Services []string
	Index    string
	Sparse   string
}

// Snapshot is the setup passed to one agent launch.
type Snapshot struct {
	Root          string
	SetupFile     string
	Worktree      string
	WorkspaceID   string
	Orchestrator  string
	AgentPath     string
	GenerateTool  string
	GenerateModel string
	Repos         []Repo
}

// Repo returns the setup for a folder kind.
func (s Snapshot) Repo(kind string) Repo {
	for _, repo := range s.Repos {
		if repo.Kind == kind {
			return repo
		}
	}
	return Repo{Kind: kind}
}

// Validate checks a snapshot before it is stored.
func Validate(snap Snapshot) error {
	if !workspaceID.MatchString(snap.WorkspaceID) {
		return fmt.Errorf("workspace must be lowercase letters, digits, hyphens, or underscores")
	}
	if !KnownOrchestrator(snap.Orchestrator) {
		return fmt.Errorf("Agent Orchestration tool is not supported")
	}
	if snap.GenerateTool != "" && !contains(OrcaAgents, snap.GenerateTool) {
		return fmt.Errorf("Generate AI Tool is not supported")
	}
	if snap.GenerateModel != "" && !modelID.MatchString(snap.GenerateModel) {
		return fmt.Errorf("Generate model is not supported")
	}
	if len(snap.Repos) < len(toolchain.Kinds) {
		return fmt.Errorf("need backend, website, mobile, and document")
	}
	seenKind := map[string]bool{}
	for i, kind := range toolchain.Kinds {
		repo := snap.Repos[i]
		if repo.Kind != kind {
			return fmt.Errorf("repo order must be backend, website, mobile, document")
		}
		if err := validateRepo(repo, kind == "document"); err != nil {
			return err
		}
		seenKind[kind] = true
	}
	for _, repo := range snap.Repos[len(toolchain.Kinds):] {
		if !toolchain.RepoID(repo.Kind) || seenKind[repo.Kind] {
			return fmt.Errorf("repository %s is not valid", repo.Kind)
		}
		seenKind[repo.Kind] = true
		if err := validateRepo(repo, false); err != nil {
			return err
		}
	}
	return nil
}

func validateRepo(repo Repo, document bool) error {
	if err := validateFolder(repo.Folder); err != nil {
		return fmt.Errorf("%s: %w", repo.Kind, err)
	}
	if document && len(repo.Services) > 0 {
		return fmt.Errorf("document does not use service names")
	}
	seen := map[string]bool{}
	for _, name := range repo.Services {
		if !serviceName.MatchString(name) {
			return fmt.Errorf("service name %q for %s may contain only letters, digits, dots, hyphens, or underscores", name, repo.Kind)
		}
		if seen[name] {
			return fmt.Errorf("service %q for %s is duplicated", name, repo.Kind)
		}
		seen[name] = true
	}
	return nil
}

// SparsePath checks a folder path for a sparse checkout.
// An empty path means the whole repository. The result uses forward slashes.
func SparsePath(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	raw = strings.Trim(raw, "/")
	if raw == "" {
		return "", nil
	}
	if strings.ContainsAny(raw, "\\\r\n") || strings.Contains(raw, ":") {
		return "", fmt.Errorf("folder path must be inside the repository, such as product/requirements")
	}
	cleaned := path.Clean(raw)
	if cleaned == "." || cleaned == ".." || strings.HasPrefix(cleaned, "../") {
		return "", fmt.Errorf("folder path must be inside the repository, such as product/requirements")
	}
	for _, part := range strings.Split(cleaned, "/") {
		if part == "" || part == "." || part == ".." || part == ".git" {
			return "", fmt.Errorf("folder path must be inside the repository, such as product/requirements")
		}
	}
	return cleaned, nil
}

func validateFolder(raw string) error {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil
	}
	if strings.ContainsAny(raw, "\r\n") {
		return fmt.Errorf("folder path is invalid")
	}
	if !filepath.IsAbs(raw) {
		return fmt.Errorf("folder must be an absolute path")
	}
	info, err := os.Stat(raw)
	if err != nil || !info.IsDir() {
		return fmt.Errorf("folder %s does not exist", raw)
	}
	return nil
}

// ApplyRoot gives every repository a folder and creates any folder that is missing.
// A saved absolute path stays. An empty path uses the directory under the app root.
// Saved service names are kept.
func ApplyRoot(root string, snap Snapshot) (Snapshot, error) {
	root = strings.TrimSpace(root)
	if root == "" || !filepath.IsAbs(root) {
		return Snapshot{}, fmt.Errorf("app root is not set")
	}
	root = filepath.Clean(root)
	byKind := map[string]Repo{}
	for _, repo := range snap.Repos {
		byKind[repo.Kind] = repo
	}
	snap.Root = root
	snap.Repos = make([]Repo, 0, len(toolchain.Kinds))
	for _, kind := range toolchain.Kinds {
		repo := byKind[kind]
		repo.Kind = kind
		repo.Folder = repoFolder(root, kind, repo.Folder)
		if kind == "document" {
			repo.Services = nil
		}
		if err := os.MkdirAll(repo.Folder, 0o755); err != nil {
			return Snapshot{}, err
		}
		snap.Repos = append(snap.Repos, repo)
	}
	extras := make([]string, 0)
	for kind := range byKind {
		if toolchain.Known(kind) || !toolchain.RepoID(kind) {
			continue
		}
		extras = append(extras, kind)
	}
	sort.Strings(extras)
	for _, kind := range extras {
		repo := byKind[kind]
		repo.Kind = kind
		repo.Folder = repoFolder(root, kind, repo.Folder)
		if err := os.MkdirAll(repo.Folder, 0o755); err != nil {
			return Snapshot{}, err
		}
		snap.Repos = append(snap.Repos, repo)
	}
	return snap, nil
}

func repoFolder(root, kind, saved string) string {
	saved = strings.TrimSpace(saved)
	if saved != "" && filepath.IsAbs(saved) {
		return filepath.Clean(saved)
	}
	if toolchain.Known(kind) {
		return toolchain.Folder(root, kind)
	}
	return filepath.Join(root, kind)
}

// Fill cleans the saved folder paths.
func Fill(snap Snapshot) Snapshot {
	for i := range snap.Repos {
		folder := strings.TrimSpace(snap.Repos[i].Folder)
		if folder == "" {
			snap.Repos[i].Folder = ""
			continue
		}
		snap.Repos[i].Folder = filepath.Clean(folder)
	}
	return snap
}

// ScanServices lists Nx project names from project.json files in a repository folder.
// An empty folder returns no names. node_modules and other tool folders are skipped.
func ScanServices(folder string) ([]string, error) {
	folder = strings.TrimSpace(folder)
	if folder == "" {
		return []string{}, nil
	}
	if strings.ContainsAny(folder, "\r\n") || !filepath.IsAbs(folder) {
		return nil, fmt.Errorf("folder must be an absolute path")
	}
	info, err := os.Stat(folder)
	if err != nil || !info.IsDir() {
		return nil, fmt.Errorf("folder does not exist")
	}
	found := map[string]struct{}{}
	err = filepath.WalkDir(folder, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return nil
		}
		if entry.IsDir() {
			if path != folder && skipServiceDir(entry.Name()) {
				return filepath.SkipDir
			}
			return nil
		}
		if entry.Name() != "project.json" {
			return nil
		}
		name, ok := readProjectName(path)
		if ok && serviceName.MatchString(name) {
			found[name] = struct{}{}
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	names := make([]string, 0, len(found))
	for name := range found {
		names = append(names, name)
	}
	sort.Strings(names)
	return names, nil
}

func skipServiceDir(name string) bool {
	switch name {
	case "node_modules", "dist", "coverage", "vendor", "tmp":
		return true
	default:
		return strings.HasPrefix(name, ".")
	}
}

func readProjectName(path string) (string, bool) {
	file, err := os.Open(path)
	if err != nil {
		return "", false
	}
	defer file.Close()
	var doc struct {
		Name string `json:"name"`
	}
	decoder := json.NewDecoder(io.LimitReader(file, 256*1024))
	if err := decoder.Decode(&doc); err != nil {
		return "", false
	}
	return strings.TrimSpace(doc.Name), true
}

// Peer is one agent this launch can send messages to.
type Peer struct {
	ID       string
	Handoffs []Handoff
}

// Handoff is one message for a peer. When is the condition for sending it and
// Send is the content. Reply is what the peer reports back to the sender when it
// has finished. An empty Reply means no report. MaxLoops, when above zero, is the
// most round trips the two agents may make.
type Handoff struct {
	When     string
	Send     string
	Reply    string
	MaxLoops int
}

// writeLaunchLead names the Agent Orchestration tool that started this agent.
// Orca is told it was started with `orca agent launch`.
func writeLaunchLead(b *strings.Builder, snap Snapshot) {
	switch NormalizeOrchestrator(snap.Orchestrator) {
	case "orca":
		b.WriteString("KIKI started you with `orca agent launch`. Follow the agent document at the end of this prompt exactly. That document is your operating instructions.\n\n")
	default:
		fmt.Fprintf(b, "KIKI started you with %s. Follow the agent document at the end of this prompt exactly. That document is your operating instructions.\n\n", OrchestratorLabel(snap.Orchestrator))
	}
}

// Requester is an agent that lists another agent under Agents Coordination and
// so turned it on for a task.
type Requester struct {
	ID    string
	Title string
}

// Prompt is the text passed to the selected Agent Orchestration tool.
func Prompt(agent agentdef.Agent, body string, snap Snapshot, note string, peers []Peer) string {
	return PromptWaiting(agent, body, snap, note, peers, nil)
}

// PromptWaiting is Prompt for an agent that a task turned on because other agents
// list it under Agents Coordination. That agent waits for a request from them
// before it does any work.
func PromptWaiting(agent agentdef.Agent, body string, snap Snapshot, note string, peers []Peer, requesters []Requester) string {
	snap = Fill(snap)
	var b strings.Builder
	fmt.Fprintf(&b, "You are %s (%s).\n\n", agent.ID, agent.Title)
	writeLaunchLead(&b, snap)
	fmt.Fprintf(&b, "Repository root: %s\n", snap.Root)
	fmt.Fprintf(&b, "%s worktree: %s\n", OrchestratorLabel(snap.Orchestrator), snap.Worktree)
	fmt.Fprintf(&b, "Setup file: %s\n", snap.SetupFile)
	fmt.Fprintf(&b, "Workspace id: %s\n", snap.WorkspaceID)
	b.WriteString("\nThe setup file is .kiki/kiki.yaml next to the app. The app folder is the repository root. Its absolute path is the Setup file line above. Read that file in Step 0. It is the only setup file. The workspace id is workspace_id in that file. Do not invent values that are missing, and do not assume the workspace id is kiki. Folder paths and service names below are the saved setup.\n\n")
	fmt.Fprintf(&b, "You were started in the %s worktree. That folder is the repository for this launch. Run git and project commands in the repository folder from the setup file. Do not create, edit, commit, or run project commands in a repository that is not listed. A repository with no folder is not set up.\n\n", OrchestratorLabel(snap.Orchestrator))
	b.WriteString("Service names are the Nx project names in a code repository. Use them when you choose a project and its guide file. The document repository has a folder and no service names.\n\n")
	b.WriteString("Repositories for this agent:\n")
	for _, item := range agent.Repos {
		kind := strings.TrimSpace(item.Kind)
		if kind == "" {
			continue
		}
		repo := snap.Repo(kind)
		guide := agentdef.GuideFile(item.Guide)
		fmt.Fprintf(&b, "- %s\n", kind)
		if repo.Folder == "" {
			b.WriteString("  folder: (not set)\n")
		} else {
			fmt.Fprintf(&b, "  folder: %s\n", repo.Folder)
		}
		if repo.Folder == "" {
			fmt.Fprintf(&b, "  guide file: %s (read it and follow it whenever you work in this repository)\n", guide)
		} else {
			fmt.Fprintf(&b, "  guide file: %s (read it and follow it whenever you work in this repository)\n", filepath.Join(repo.Folder, guide))
		}
		if repo.Sparse != "" {
			fmt.Fprintf(&b, "  checkout: %s\n", repo.Sparse)
			if item.Readonly {
				fmt.Fprintf(&b, "  This repository is a sparse checkout of %s. The working tree contains that folder and files at the repository root. Other folders are not checked out. Do not disable sparse checkout or add another path. This repository is readonly for this agent. Read files only inside %s. Do not create, edit, delete, rename, commit, or push.\n", repo.Sparse, repo.Sparse)
			} else {
				fmt.Fprintf(&b, "  This repository is a sparse checkout of %s. The working tree contains that folder and files at the repository root. Other folders are not checked out. Do not disable sparse checkout or add another path. Read and change files only inside %s. Run git commit, git push, and git pull in the repository folder. Pull updates this checkout. A commit keeps every other path on the remote as it already is.\n", repo.Sparse, repo.Sparse)
			}
		} else if item.Readonly {
			b.WriteString("  Readonly: this agent can only read this repository. Do not create, edit, delete, rename, commit, or push.\n")
		}
		if kind != "document" {
			if len(repo.Services) == 0 {
				b.WriteString("  services: (none)\n")
			} else {
				fmt.Fprintf(&b, "  services: %s\n", strings.Join(repo.Services, ", "))
			}
		}
		note := strings.TrimSpace(item.Description)
		if note == "" {
			continue
		}
		b.WriteString("  Work in this repository:\n")
		for _, line := range strings.Split(note, "\n") {
			fmt.Fprintf(&b, "  %s\n", line)
		}
	}
	b.WriteString("\n")
	writeIndexes(&b, agent.Repos, snap)
	writePeers(&b, agent.ID, peers, OrchestratorLabel(snap.Orchestrator))
	writeUses(&b, "MCP servers for this agent:", "Use this MCP server:", agent.MCPs)
	writeUses(&b, "Tools for this agent:", "", agent.Tools)
	note = strings.TrimSpace(note)
	switch {
	case writeWaiting(&b, agent.ID, requesters, OrchestratorLabel(snap.Orchestrator), note != ""):
	case note == "":
		b.WriteString("The user did not send a ticket, a requirement folder, or any other instruction. Follow the Startup behavior in the agent document and wait.\n\n")
	default:
		b.WriteString("The user sent a message with this launch. It is repeated after the agent document. Treat that message as the user message in your terminal.\n\n")
	}
	fmt.Fprintf(&b, "===== AGENT DOCUMENT (%s) =====\n", agent.File)
	b.WriteString(body)
	if !strings.HasSuffix(body, "\n") {
		b.WriteString("\n")
	}
	b.WriteString("===== END AGENT DOCUMENT =====\n")
	if note != "" {
		b.WriteString("\n===== USER MESSAGE =====\n")
		b.WriteString(note)
		b.WriteString("\n===== END USER MESSAGE =====\n")
	}
	return b.String()
}

// writeWaiting tells a supporting agent to stay idle until a request reaches it.
// It reports whether it wrote anything.
func writeWaiting(b *strings.Builder, self string, requesters []Requester, orchestrator string, hasNote bool) bool {
	var list []string
	for _, item := range requesters {
		id := strings.TrimSpace(item.ID)
		if id == "" || id == strings.TrimSpace(self) {
			continue
		}
		if title := strings.TrimSpace(item.Title); title != "" {
			list = append(list, fmt.Sprintf("%s (%s)", id, title))
		} else {
			list = append(list, id)
		}
	}
	if len(list) == 0 {
		return false
	}
	fmt.Fprintf(b, "WAIT FOR A REQUEST. KIKI turned you on for a task because you are listed under Agents Coordination by %s. You are a supporting agent. Do not start any work yet. Do not read repositories, do not create or edit files, and do not run project commands until a request reaches you. Stay idle and wait for a request from %s, sent through %s to your agent id. Do the work in the agent document only after that request arrives, and do it for what the request asks.\n", strings.Join(list, " and "), strings.Join(list, " or "), orchestrator)
	if hasNote {
		b.WriteString("The user message at the end of this prompt is the task. It is context for the request that will reach you. It is not an instruction to start now.\n")
	}
	b.WriteString("\n")
	return true
}

// writeLoop states the loop limit for one message. A loop is one round trip, where
// the agent sends the message and the other agent reports back.
func writeLoop(b *strings.Builder, peer string, pair Handoff, number int) {
	again := "the report from " + peer + " asks for more work, or shows the work is not done"
	if pair.When != "" {
		again = "the Send when condition of message " + fmt.Sprint(number) + " is met again after the report from " + peer
	}
	fmt.Fprintf(b, "    Loop: this exchange can go back and forth between you and %s. Count one loop each time you send this message and %s reports back. Allow at most %d loops. Send this message again only when %s. State the loop number in every message you send, for example \"loop 1 of %d\". After loop %d, do not send this message again: stop the exchange, and tell the user what is still open.\n", peer, peer, pair.MaxLoops, again, pair.MaxLoops, pair.MaxLoops)
}

func writePeers(b *strings.Builder, self string, peers []Peer, orchestrator string) {
	self = strings.TrimSpace(self)
	list := make([]Peer, 0, len(peers))
	for _, peer := range peers {
		id := strings.TrimSpace(peer.ID)
		if id == "" || id == self {
			continue
		}
		item := Peer{ID: id}
		for _, pair := range peer.Handoffs {
			when := strings.TrimSpace(pair.When)
			send := strings.TrimSpace(pair.Send)
			if send == "" {
				continue
			}
			handoff := Handoff{When: when, Send: send, Reply: strings.TrimSpace(pair.Reply)}
			if handoff.Reply != "" && pair.MaxLoops > 0 {
				handoff.MaxLoops = pair.MaxLoops
			}
			item.Handoffs = append(item.Handoffs, handoff)
		}
		list = append(list, item)
	}
	if self == "" && len(list) == 0 {
		return
	}
	if self != "" {
		fmt.Fprintf(b, "Your agent id is %s. Other agents reach you by this id.\n", self)
	}
	if len(list) == 0 {
		b.WriteString("\n")
		return
	}
	b.WriteString("Agents Coordination with these agents. When you communicate or hand work to one of them, use that agent id.\n")
	fmt.Fprintf(b, "Each message below has a condition and the content to send. While you work, check each condition. When a condition is met, send that content to the agent through %s, addressed by its agent id. Add the facts the other agent needs, such as paths, ids, and results. Send each message when its condition is met, not before. A message with no condition is sent at the point its content describes.\n", orchestrator)
	for _, peer := range list {
		fmt.Fprintf(b, "- agent id: %s\n", peer.ID)
		if len(peer.Handoffs) == 0 {
			b.WriteString("  No message is set for this agent.\n")
			continue
		}
		for i, pair := range peer.Handoffs {
			fmt.Fprintf(b, "  Message %d to %s:\n", i+1, peer.ID)
			if pair.When == "" {
				b.WriteString("    Send when: (no condition set)\n")
			} else {
				b.WriteString("    Send when:\n")
				for _, line := range strings.Split(pair.When, "\n") {
					fmt.Fprintf(b, "      %s\n", line)
				}
			}
			b.WriteString("    Send:\n")
			for _, line := range strings.Split(pair.Send, "\n") {
				fmt.Fprintf(b, "      %s\n", line)
			}
			switch {
			case pair.Reply != "" && self != "":
				fmt.Fprintf(b, "    Report back: yes. In this message, ask %s to report back to you, agent id %s, when it has finished the work. Use %s for that report. Expect that report.\n", peer.ID, self, orchestrator)
			case pair.Reply != "":
				fmt.Fprintf(b, "    Report back: yes. In this message, ask %s to report back to you when it has finished the work.\n", peer.ID)
			default:
				fmt.Fprintf(b, "    Report back: no. Do not ask %s to report back.\n", peer.ID)
			}
			if pair.Reply != "" {
				fmt.Fprintf(b, "    The report from %s must contain:\n", peer.ID)
				for _, line := range strings.Split(pair.Reply, "\n") {
					fmt.Fprintf(b, "      %s\n", line)
				}
				b.WriteString("    Put that in the message so the other agent knows what to send back.\n")
			}
			if pair.Reply != "" && pair.MaxLoops > 0 {
				writeLoop(b, peer.ID, pair, i+1)
			}
		}
	}
	b.WriteString("\n")
}

func writePlace(b *strings.Builder, item agentdef.Use) {
	if line := placeLine(item.BoardName, item.BoardID); line != "" {
		fmt.Fprintf(b, "  Board: %s\n", line)
	}
	if line := placeLine(item.SpaceName, item.SpaceKey); line != "" {
		fmt.Fprintf(b, "  Space: %s\n", line)
	} else if line := placeLine("", item.SpaceID); line != "" {
		fmt.Fprintf(b, "  Space: %s\n", line)
	}
	if line := placeLine(item.ParentTitle, item.ParentID); line != "" {
		fmt.Fprintf(b, "  Parent page: %s\n", line)
	}
}

func placeLine(name, id string) string {
	name = strings.TrimSpace(name)
	id = strings.TrimSpace(id)
	if name == "" {
		return id
	}
	if id == "" || id == name {
		return name
	}
	return name + " (" + id + ")"
}

func writeUses(b *strings.Builder, heading, label string, items []agentdef.Use) {
	list := make([]agentdef.Use, 0, len(items))
	for _, item := range items {
		id := strings.TrimSpace(item.ID)
		if id == "" {
			continue
		}
		list = append(list, agentdef.Use{
			ID:          id,
			Name:        strings.TrimSpace(item.Name),
			Description: strings.TrimSpace(item.Description),
			BoardID:     strings.TrimSpace(item.BoardID),
			BoardName:   strings.TrimSpace(item.BoardName),
			SpaceID:     strings.TrimSpace(item.SpaceID),
			SpaceKey:    strings.TrimSpace(item.SpaceKey),
			SpaceName:   strings.TrimSpace(item.SpaceName),
			ParentID:    strings.TrimSpace(item.ParentID),
			ParentTitle: strings.TrimSpace(item.ParentTitle),
		})
	}
	if len(list) == 0 {
		return
	}
	b.WriteString(heading)
	b.WriteByte('\n')
	for _, item := range list {
		name := item.Name
		if name == "" {
			name = item.ID
		}
		fmt.Fprintf(b, "- %s\n", name)
		writePlace(b, item)
		if item.Description == "" {
			continue
		}
		if label != "" {
			fmt.Fprintf(b, "  %s\n", label)
		}
		for _, line := range strings.Split(item.Description, "\n") {
			fmt.Fprintf(b, "  %s\n", line)
		}
	}
	b.WriteString("\n")
}

func contains(list []string, value string) bool {
	for _, item := range list {
		if item == value {
			return true
		}
	}
	return false
}
