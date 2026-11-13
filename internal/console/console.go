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
	OrcaAgent     string
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
	if !contains(OrcaAgents, snap.OrcaAgent) {
		return fmt.Errorf("Default AI Tool is not supported")
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

// ApplyRoot pins every repository to a directory under the app root and creates
// any of those directories that are missing. Saved service names are kept.
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
		repo.Folder = toolchain.Folder(root, kind)
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
		repo.Folder = filepath.Join(root, kind)
		if err := os.MkdirAll(repo.Folder, 0o755); err != nil {
			return Snapshot{}, err
		}
		snap.Repos = append(snap.Repos, repo)
	}
	return snap, nil
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

// Peer is one agent this launch can hand work to.
type Peer struct {
	ID   string
	Note string
}

// Prompt is the text passed to `orca agent launch --prompt`.
func Prompt(agent agentdef.Agent, body string, snap Snapshot, note string, peers []Peer) string {
	snap = Fill(snap)
	var b strings.Builder
	fmt.Fprintf(&b, "You are %s (%s).\n\n", agent.ID, agent.Title)
	b.WriteString("KIKI started you with `orca agent launch`. Follow the agent document at the end of this prompt exactly. That document is your operating instructions.\n\n")
	fmt.Fprintf(&b, "Repository root: %s\n", snap.Root)
	fmt.Fprintf(&b, "Orca worktree: %s\n", snap.Worktree)
	fmt.Fprintf(&b, "Setup file: %s\n", snap.SetupFile)
	fmt.Fprintf(&b, "Workspace id: %s\n", snap.WorkspaceID)
	b.WriteString("\nThe setup file is .kiki/kiki.yaml next to the app. The app folder is the repository root. Its absolute path is the Setup file line above. Read that file in Step 0. It is the only setup file. Do not read .kiki/agents/config.local.yaml. The workspace id is workspace_id in that file. Do not invent values that are missing, and do not assume the workspace id is kiki. Folder paths and service names below are the saved setup.\n\n")
	b.WriteString("You were started in the Orca worktree. That folder is the repository for this launch. Run git, gh, and project commands in the repository folder from the setup file. Do not create, edit, commit, or run project commands in a repository that is not listed. A repository with no folder is not set up.\n\n")
	b.WriteString("Service names are the Nx project names in a code repository. Use them when you choose a project and its AGENTS.md. The document repository has a folder and no service names.\n\n")
	b.WriteString("Repositories for this agent:\n")
	for _, item := range agent.Repos {
		kind := strings.TrimSpace(item.Kind)
		if kind == "" {
			continue
		}
		repo := snap.Repo(kind)
		fmt.Fprintf(&b, "- %s\n", kind)
		if repo.Folder == "" {
			b.WriteString("  folder: (not set)\n")
		} else {
			fmt.Fprintf(&b, "  folder: %s\n", repo.Folder)
		}
		if repo.Sparse != "" {
			fmt.Fprintf(&b, "  checkout: %s\n", repo.Sparse)
			fmt.Fprintf(&b, "  This repository is a sparse checkout of %s. The working tree contains that folder and files at the repository root. Other folders are not checked out. Do not disable sparse checkout or add another path. Read and change files only inside %s. Run git commit, git push, and git pull in the repository folder. Pull updates this checkout. A commit keeps every other path on the remote as it already is.\n", repo.Sparse, repo.Sparse)
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
	writePeers(&b, agent.ID, peers)
	note = strings.TrimSpace(note)
	if note == "" {
		b.WriteString("The user did not send a ticket, a requirement folder, or any other instruction. Follow the Startup behavior in the agent document and wait.\n\n")
	} else {
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

func writePeers(b *strings.Builder, self string, peers []Peer) {
	self = strings.TrimSpace(self)
	list := make([]Peer, 0, len(peers))
	for _, peer := range peers {
		id := strings.TrimSpace(peer.ID)
		if id == "" || id == self {
			continue
		}
		list = append(list, Peer{ID: id, Note: strings.TrimSpace(peer.Note)})
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
	b.WriteString("Teamwork with these agents. When you communicate or hand work to one of them, use that agent id.\n")
	for _, peer := range list {
		fmt.Fprintf(b, "- agent id: %s\n", peer.ID)
		if peer.Note == "" {
			continue
		}
		b.WriteString("  Coordination:\n")
		for _, line := range strings.Split(peer.Note, "\n") {
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
