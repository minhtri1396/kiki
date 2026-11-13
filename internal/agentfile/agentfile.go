// Package agentfile stores each agent as a markdown file in the agent folder.
package agentfile

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"kiki/internal/agentdef"
	"kiki/internal/console"

	"gopkg.in/yaml.v3"
)

// ErrNotFound means the agent id is not in the agent folder.
var ErrNotFound = errors.New("unknown agent")

// ErrExists means another agent file already uses that id.
var ErrExists = errors.New("an agent with that id already exists")

// File is one agent markdown file. Category is the folder that contains it.
type File struct {
	ID          string
	Name        string
	Description string
	Category    string
	Enabled     bool
	AITool      string
	Model       string
	Repos       []Repository
	WorksWith   []Collaborator
	Prompt      string
	Path        string
}

// Repository is one repository this agent uses, and what it does there.
type Repository struct {
	Kind        string
	Description string
}

// Collaborator is one agent this agent works with, and how they coordinate.
type Collaborator struct {
	ID            string
	Collaboration string
}

// repoList accepts a kind, a list of kinds, or a list of kind and description notes.
type repoList []Repository

func (r *repoList) UnmarshalYAML(value *yaml.Node) error {
	if value.Kind == yaml.ScalarNode {
		kind := strings.TrimSpace(value.Value)
		if kind == "" {
			*r = nil
			return nil
		}
		*r = repoList{{Kind: kind}}
		return nil
	}
	if value.Kind != yaml.SequenceNode {
		return fmt.Errorf("repos must be a repository kind or a list")
	}
	out := make(repoList, 0, len(value.Content))
	for _, item := range value.Content {
		switch item.Kind {
		case yaml.ScalarNode:
			kind := strings.TrimSpace(item.Value)
			if kind == "" {
				continue
			}
			out = append(out, Repository{Kind: kind})
		case yaml.MappingNode:
			var row struct {
				ID          string `yaml:"id"`
				Kind        string `yaml:"kind"`
				Description string `yaml:"description"`
			}
			if err := item.Decode(&row); err != nil {
				return err
			}
			kind := strings.TrimSpace(row.ID)
			if kind == "" {
				kind = strings.TrimSpace(row.Kind)
			}
			if kind == "" {
				continue
			}
			out = append(out, Repository{Kind: kind, Description: strings.TrimSpace(row.Description)})
		default:
			return fmt.Errorf("repos must be a repository kind or a list")
		}
	}
	*r = out
	return nil
}

func (r repoList) MarshalYAML() (any, error) {
	if len(r) == 0 {
		return []string{}, nil
	}
	noted := false
	for _, item := range r {
		if strings.TrimSpace(item.Description) != "" {
			noted = true
			break
		}
	}
	if !noted {
		kinds := make([]string, len(r))
		for i, item := range r {
			kinds[i] = item.Kind
		}
		return kinds, nil
	}
	type row struct {
		ID          string `yaml:"id"`
		Description string `yaml:"description,omitempty"`
	}
	rows := make([]row, len(r))
	for i, item := range r {
		rows[i] = row{ID: item.Kind, Description: strings.TrimSpace(item.Description)}
	}
	return rows, nil
}

// peers accepts an id, a list of ids, or a list of id and description notes.
// A note may still be stored under the old key collaboration.
type peers []Collaborator

func (p *peers) UnmarshalYAML(value *yaml.Node) error {
	if value.Kind == yaml.ScalarNode {
		text := strings.TrimSpace(value.Value)
		if text == "" {
			*p = nil
			return nil
		}
		*p = peers{{ID: text}}
		return nil
	}
	if value.Kind != yaml.SequenceNode {
		return fmt.Errorf("teamwork must be an agent id or a list")
	}
	out := make(peers, 0, len(value.Content))
	for _, item := range value.Content {
		switch item.Kind {
		case yaml.ScalarNode:
			id := strings.TrimSpace(item.Value)
			if id == "" {
				continue
			}
			out = append(out, Collaborator{ID: id})
		case yaml.MappingNode:
			var row struct {
				ID            string `yaml:"id"`
				Description   string `yaml:"description"`
				Collaboration string `yaml:"collaboration"`
			}
			if err := item.Decode(&row); err != nil {
				return err
			}
			id := strings.TrimSpace(row.ID)
			if id == "" {
				continue
			}
			note := strings.TrimSpace(row.Description)
			if note == "" {
				note = strings.TrimSpace(row.Collaboration)
			}
			out = append(out, Collaborator{ID: id, Collaboration: note})
		default:
			return fmt.Errorf("teamwork must be an agent id or a list")
		}
	}
	*p = out
	return nil
}

func (p peers) MarshalYAML() (any, error) {
	if len(p) == 0 {
		return []string{}, nil
	}
	noted := false
	for _, item := range p {
		if strings.TrimSpace(item.Collaboration) != "" {
			noted = true
			break
		}
	}
	if !noted {
		ids := make([]string, len(p))
		for i, item := range p {
			ids[i] = item.ID
		}
		return ids, nil
	}
	type row struct {
		ID          string `yaml:"id"`
		Description string `yaml:"description,omitempty"`
	}
	rows := make([]row, len(p))
	for i, item := range p {
		rows[i] = row{ID: item.ID, Description: strings.TrimSpace(item.Collaboration)}
	}
	return rows, nil
}

type header struct {
	ID              string   `yaml:"id"`
	Name            string   `yaml:"name"`
	Description     string   `yaml:"description"`
	Enabled         *bool    `yaml:"enabled,omitempty"`
	AITool          string   `yaml:"ai_tool"`
	Model           string   `yaml:"model"`
	Repos           repoList `yaml:"repos"`
	WorksWith       peers    `yaml:"teamwork,omitempty"`
	LegacyWorksWith peers    `yaml:"works_with,omitempty"`
}

var slugPattern = regexp.MustCompile(`^[a-z][a-z0-9_]*$`)

// Resolve returns the agent folder. An empty path uses .kiki/agents next to the app.
func Resolve(root, raw string) (string, error) {
	root = strings.TrimSpace(root)
	if root == "" || !filepath.IsAbs(root) {
		return "", fmt.Errorf("app root is not set")
	}
	raw = strings.TrimSpace(raw)
	if raw == "" {
		raw = filepath.Join(root, ".kiki", "agents")
	}
	if strings.ContainsAny(raw, "\r\n") {
		return "", fmt.Errorf("agent path is invalid")
	}
	if !filepath.IsAbs(raw) {
		return "", fmt.Errorf("agent path must be an absolute path")
	}
	clean := filepath.Clean(raw)
	info, err := os.Stat(clean)
	if err == nil && !info.IsDir() {
		return "", fmt.Errorf("agent path is not a folder")
	}
	if err != nil && !os.IsNotExist(err) {
		return "", err
	}
	return clean, nil
}

func moveFile(src, dst string) error {
	if filepath.Clean(src) == filepath.Clean(dst) {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	if err := os.Rename(src, dst); err == nil {
		return nil
	}
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	info, err := in.Stat()
	if err != nil {
		return err
	}
	tmp := dst + ".tmp"
	out, err := os.OpenFile(tmp, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, info.Mode().Perm())
	if err != nil {
		return err
	}
	_, copyErr := io.Copy(out, in)
	closeErr := out.Close()
	if copyErr != nil {
		_ = os.Remove(tmp)
		return copyErr
	}
	if closeErr != nil {
		_ = os.Remove(tmp)
		return closeErr
	}
	if err := os.Rename(tmp, dst); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return os.Remove(src)
}

// Ensure creates the agent folder when it is missing. The first time that
// folder has no agent markdown, it writes one file per embedded agent inside
// a category folder. A later run leaves those files as they are, and moves a
// top-level <id>.md into its category folder.
func Ensure(dir, defaultTool string) error {
	dir = strings.TrimSpace(dir)
	if dir == "" || !filepath.IsAbs(dir) {
		return fmt.Errorf("agent path is not set")
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	present, err := hasAgentMarkdown(dir)
	if err != nil {
		return err
	}
	if !present {
		tool, model := defaultToolModel(defaultTool)
		for _, agent := range agentdef.Catalog {
			path := filepath.Join(dir, categoryName(agent.Group), agent.ID+".md")
			if err := writeNew(path, agent, tool, model); err != nil {
				return err
			}
		}
		return nil
	}
	for _, agent := range agentdef.Catalog {
		dest := filepath.Join(dir, categoryName(agent.Group), agent.ID+".md")
		if _, err := os.Stat(dest); err == nil {
			continue
		} else if !os.IsNotExist(err) {
			return err
		}
		legacy := filepath.Join(dir, agent.ID+".md")
		if _, err := os.Stat(legacy); err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return err
		}
		if err := moveFile(legacy, dest); err != nil {
			return err
		}
	}
	return nil
}

// List reads the agent files in category folders.
func List(dir string) ([]File, error) {
	dir = filepath.Clean(strings.TrimSpace(dir))
	paths, err := categoryFiles(dir)
	if err != nil {
		return nil, err
	}
	files := make([]File, 0, len(paths))
	for _, path := range paths {
		file, err := readFile(dir, path)
		if err != nil {
			if errors.Is(err, errSkip) {
				continue
			}
			return nil, err
		}
		files = append(files, file)
	}
	order := map[string]int{}
	for i, agent := range agentdef.Catalog {
		order[agent.ID] = i
	}
	sort.Slice(files, func(i, j int) bool {
		left, okLeft := order[files[i].ID]
		right, okRight := order[files[j].ID]
		if okLeft && okRight {
			return left < right
		}
		if okLeft != okRight {
			return okLeft
		}
		if files[i].Category != files[j].Category {
			return files[i].Category < files[j].Category
		}
		return files[i].ID < files[j].ID
	})
	return files, nil
}

// Find returns the agent file with this id.
func Find(dir, id string) (File, bool, error) {
	files, err := List(dir)
	if err != nil {
		return File{}, false, err
	}
	for _, file := range files {
		if file.ID == id {
			return file, true, nil
		}
	}
	return File{}, false, nil
}

// Save writes one agent file. currentID is the id already stored on disk.
// A new id or category moves the file. The prompt body is stored as given.
func Save(dir, currentID string, next File) error {
	dir = filepath.Clean(strings.TrimSpace(dir))
	if dir == "" || !filepath.IsAbs(dir) {
		return fmt.Errorf("agent path is not set")
	}
	next, err := normalizeFile(next)
	if err != nil {
		return err
	}
	files, err := List(dir)
	if err != nil {
		return err
	}
	var current File
	found := false
	for _, file := range files {
		if file.ID == currentID {
			current = file
			found = true
			continue
		}
		if file.ID == next.ID {
			return ErrExists
		}
	}
	if !found {
		return ErrNotFound
	}
	dest := filepath.Join(dir, next.Category, next.ID+".md")
	if info, err := os.Stat(dest); err == nil && !info.IsDir() && filepath.Clean(dest) != filepath.Clean(current.Path) {
		return ErrExists
	} else if err != nil && !os.IsNotExist(err) {
		return err
	}
	if err := writeFile(dest, headerOf(next), next.Prompt); err != nil {
		return err
	}
	if filepath.Clean(dest) != filepath.Clean(current.Path) {
		if err := os.Remove(current.Path); err != nil && !os.IsNotExist(err) {
			return err
		}
		removeEmpty(filepath.Dir(current.Path), dir)
	}
	return nil
}

// ReplaceRepos rewrites repository ids in the agent files under dir.
// Agents that do not use a replaced id are left unchanged.
func ReplaceRepos(dir string, replacements map[string]string) error {
	if len(replacements) == 0 {
		return nil
	}
	files, err := List(dir)
	if err != nil {
		return err
	}
	for _, file := range files {
		changed := false
		for i, repo := range file.Repos {
			next := strings.TrimSpace(replacements[repo.Kind])
			if next == "" || next == repo.Kind {
				continue
			}
			file.Repos[i].Kind = next
			changed = true
		}
		if !changed {
			continue
		}
		if err := Save(dir, file.ID, file); err != nil {
			return err
		}
	}
	return nil
}

func defaultToolModel(defaultTool string) (string, string) {
	tool := strings.TrimSpace(defaultTool)
	if tool == "" || !containsTool(tool) {
		tool = "cursor"
	}
	model := ""
	resolved, err := console.ResolveModel(console.ModelCatalog(), tool, "")
	if err == nil {
		model = resolved
	}
	return tool, model
}

func containsTool(tool string) bool {
	for _, name := range console.OrcaAgents {
		if name == tool {
			return true
		}
	}
	return false
}

func writeNew(path string, agent agentdef.Agent, tool, model string) error {
	body, err := agent.Body()
	if err != nil {
		return err
	}
	on := true
	var with peers
	for _, peer := range agent.Peers {
		id := strings.TrimSpace(peer.ID)
		if id == "" {
			continue
		}
		with = append(with, Collaborator{
			ID:            id,
			Collaboration: strings.TrimSpace(peer.Collaboration),
		})
	}
	repos := make(repoList, 0, len(agent.Repos))
	for _, repo := range agent.Repos {
		kind := strings.TrimSpace(repo.Kind)
		if kind == "" {
			continue
		}
		repos = append(repos, Repository{
			Kind:        kind,
			Description: strings.TrimSpace(repo.Description),
		})
	}
	meta := header{
		ID:          agent.ID,
		Name:        agent.Title,
		Description: agent.Summary,
		Enabled:     &on,
		AITool:      tool,
		Model:       model,
		Repos:       repos,
		WorksWith:   with,
	}
	return writeFile(path, meta, body)
}

func categoryName(group string) string {
	return strings.ToLower(strings.TrimSpace(group))
}

func headerOf(file File) header {
	on := file.Enabled
	return header{
		ID:          file.ID,
		Name:        file.Name,
		Description: file.Description,
		Enabled:     &on,
		AITool:      file.AITool,
		Model:       file.Model,
		Repos:       append(repoList(nil), file.Repos...),
		WorksWith:   append(peers(nil), file.WorksWith...),
	}
}

func normalizeFile(file File) (File, error) {
	file.ID = strings.TrimSpace(file.ID)
	file.Name = strings.TrimSpace(file.Name)
	file.Description = strings.TrimSpace(file.Description)
	file.Category = strings.ToLower(strings.TrimSpace(file.Category))
	file.AITool = strings.TrimSpace(file.AITool)
	file.Model = strings.TrimSpace(file.Model)
	file.Prompt = strings.ReplaceAll(file.Prompt, "\r\n", "\n")
	if !slugPattern.MatchString(file.ID) {
		return File{}, fmt.Errorf("enter an agent id that starts with a letter and uses only letters, digits, and underscores")
	}
	if !slugPattern.MatchString(file.Category) {
		return File{}, fmt.Errorf("enter a category folder name that starts with a letter and uses only letters, digits, and underscores")
	}
	if file.Name == "" || strings.ContainsAny(file.Name, "\r\n") {
		return File{}, fmt.Errorf("enter a name")
	}
	with := make([]Collaborator, 0, len(file.WorksWith))
	seenPeer := map[string]bool{}
	for _, peer := range file.WorksWith {
		id := strings.TrimSpace(peer.ID)
		if id == "" || id == file.ID || seenPeer[id] {
			continue
		}
		if !slugPattern.MatchString(id) {
			return File{}, fmt.Errorf("works with must be an agent id")
		}
		seenPeer[id] = true
		with = append(with, Collaborator{ID: id, Collaboration: strings.TrimSpace(peer.Collaboration)})
	}
	file.WorksWith = with
	if !containsTool(file.AITool) {
		return File{}, fmt.Errorf("choose an AI tool")
	}
	if file.Model == "" || strings.ContainsAny(file.Model, "\r\n") {
		return File{}, fmt.Errorf("choose a model")
	}
	repos := make([]Repository, 0, len(file.Repos))
	seen := map[string]bool{}
	for _, repo := range file.Repos {
		kind := strings.TrimSpace(repo.Kind)
		if kind == "" || seen[kind] {
			continue
		}
		if !slugPattern.MatchString(kind) {
			return File{}, fmt.Errorf("repos must use folder-safe names")
		}
		seen[kind] = true
		repos = append(repos, Repository{Kind: kind, Description: strings.TrimSpace(repo.Description)})
	}
	if len(repos) == 0 {
		return File{}, fmt.Errorf("choose at least one repo")
	}
	file.Repos = repos
	return file, nil
}

var errSkip = errors.New("skip agent file")

func readFile(root, path string) (File, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return File{}, err
	}
	meta, body, err := parse(string(raw))
	if err != nil {
		return File{}, errSkip
	}
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return File{}, err
	}
	category := filepath.Dir(rel)
	if category == "." || strings.Contains(category, string(filepath.Separator)) {
		return File{}, errSkip
	}
	id := strings.TrimSpace(meta.ID)
	if id == "" {
		id = strings.TrimSuffix(filepath.Base(path), filepath.Ext(path))
	}
	enabled := true
	if meta.Enabled != nil {
		enabled = *meta.Enabled
	}
	return File{
		ID:          id,
		Name:        strings.TrimSpace(meta.Name),
		Description: strings.TrimSpace(meta.Description),
		Category:    category,
		Enabled:     enabled,
		AITool:      strings.TrimSpace(meta.AITool),
		Model:       strings.TrimSpace(meta.Model),
		Repos:       append([]Repository(nil), meta.Repos...),
		WorksWith:   append([]Collaborator(nil), meta.WorksWith...),
		Prompt:      body,
		Path:        path,
	}, nil
}

func categoryFiles(dir string) ([]string, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var paths []string
	for _, entry := range entries {
		name := entry.Name()
		if !entry.IsDir() || strings.HasPrefix(name, ".") {
			continue
		}
		children, err := os.ReadDir(filepath.Join(dir, name))
		if err != nil {
			return nil, err
		}
		for _, child := range children {
			childName := child.Name()
			if child.IsDir() || strings.HasPrefix(childName, ".") || !strings.EqualFold(filepath.Ext(childName), ".md") {
				continue
			}
			paths = append(paths, filepath.Join(dir, name, childName))
		}
	}
	sort.Strings(paths)
	return paths, nil
}

func hasAgentMarkdown(dir string) (bool, error) {
	rels, err := markdownRels(dir)
	if err != nil {
		return false, err
	}
	return len(rels) > 0, nil
}

func markdownRels(dir string) ([]string, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var rels []string
	for _, entry := range entries {
		name := entry.Name()
		if strings.HasPrefix(name, ".") {
			continue
		}
		if !entry.IsDir() {
			if strings.EqualFold(filepath.Ext(name), ".md") {
				rels = append(rels, name)
			}
			continue
		}
		children, err := os.ReadDir(filepath.Join(dir, name))
		if err != nil {
			return nil, err
		}
		for _, child := range children {
			childName := child.Name()
			if child.IsDir() || strings.HasPrefix(childName, ".") || !strings.EqualFold(filepath.Ext(childName), ".md") {
				continue
			}
			rels = append(rels, filepath.Join(name, childName))
		}
	}
	sort.Strings(rels)
	return rels, nil
}

func removeEmpty(dir, root string) {
	dir = filepath.Clean(dir)
	root = filepath.Clean(root)
	if dir == root {
		return
	}
	rel, err := filepath.Rel(root, dir)
	if err != nil || rel == "." || strings.HasPrefix(rel, "..") {
		return
	}
	entries, err := os.ReadDir(dir)
	if err != nil || len(entries) > 0 {
		return
	}
	_ = os.Remove(dir)
}

func writeFile(path string, meta header, body string) error {
	raw, err := format(meta, body)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, raw, 0o644); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return nil
}

func format(meta header, body string) ([]byte, error) {
	raw, err := yaml.Marshal(meta)
	if err != nil {
		return nil, err
	}
	body = strings.TrimRight(body, "\n") + "\n"
	text := "---\n" + string(raw) + "---\n\n" + body
	return []byte(text), nil
}

func parse(text string) (header, string, error) {
	front, body, ok := split(text)
	if !ok {
		return header{}, "", fmt.Errorf("agent file has no front matter")
	}
	var meta header
	if err := yaml.Unmarshal([]byte(front), &meta); err != nil {
		return header{}, "", err
	}
	if len(meta.WorksWith) == 0 {
		meta.WorksWith = meta.LegacyWorksWith
	}
	meta.LegacyWorksWith = nil
	return meta, body, nil
}

func split(text string) (string, string, bool) {
	const fence = "---"
	if !strings.HasPrefix(text, fence+"\n") {
		return "", "", false
	}
	rest := text[len(fence)+1:]
	end := strings.Index(rest, "\n"+fence+"\n")
	if end < 0 {
		return "", "", false
	}
	body := rest[end+len("\n"+fence+"\n"):]
	if strings.HasPrefix(body, "\n") {
		body = body[1:]
	}
	return rest[:end], body, true
}
