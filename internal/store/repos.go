package store

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"kiki/internal/console"
	"kiki/internal/toolchain"

	"gopkg.in/yaml.v3"
)

type repoFile struct {
	ID       string   `yaml:"id"`
	Folder   string   `yaml:"folder,omitempty"`
	Services []string `yaml:"services"`
	Index    string   `yaml:"index,omitempty"`
	Sparse   string   `yaml:"sparse,omitempty"`
}

func (f *repoFile) UnmarshalYAML(value *yaml.Node) error {
	var row struct {
		ID       string   `yaml:"id"`
		Kind     string   `yaml:"kind"`
		Folder   string   `yaml:"folder"`
		Services []string `yaml:"services"`
		Index    string   `yaml:"index"`
		Sparse   string   `yaml:"sparse"`
	}
	if err := value.Decode(&row); err != nil {
		return err
	}
	id := strings.TrimSpace(row.ID)
	if id == "" {
		id = strings.TrimSpace(row.Kind)
	}
	*f = repoFile{
		ID:       id,
		Folder:   row.Folder,
		Services: row.Services,
		Index:    row.Index,
		Sparse:   row.Sparse,
	}
	return nil
}

// SaveRepoIndex records obsidian or codegraph on one repository file.
// An empty index removes it. The other repositories in snap keep the index already on them.
func (s *Store) SaveRepoIndex(snap console.Snapshot, kind, index string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if !toolchain.RepoID(kind) {
		return fmt.Errorf("unknown repository")
	}
	if index != "" && index != "obsidian" && index != "codegraph" {
		return fmt.Errorf("index must be obsidian or codegraph")
	}
	found := false
	for i := range snap.Repos {
		if snap.Repos[i].Kind != kind {
			continue
		}
		snap.Repos[i].Index = index
		found = true
	}
	if !found {
		return fmt.Errorf("unknown repository")
	}
	return writeRepoFiles(s.path, reposForFiles(snap))
}

// SaveRepoSparse records the sparse-checkout folder on one repository file.
// An empty path clears it. The other repositories keep the path already on them.
func (s *Store) SaveRepoSparse(snap console.Snapshot, kind, sparse string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if !toolchain.RepoID(kind) {
		return fmt.Errorf("unknown repository")
	}
	cleaned, err := console.SparsePath(sparse)
	if err != nil {
		return err
	}
	found := false
	for i := range snap.Repos {
		if snap.Repos[i].Kind != kind {
			continue
		}
		snap.Repos[i].Sparse = cleaned
		found = true
	}
	if !found {
		return fmt.Errorf("unknown repository")
	}
	return writeRepoFiles(s.path, reposForFiles(snap))
}

// RemoveIndexFolder deletes the index directory inside a repository.
// codegraph removes .codegraph. obsidian removes .obsidian. A missing directory is left as success.
func RemoveIndexFolder(repoFolder, index string) error {
	name, err := indexDirName(index)
	if err != nil {
		return err
	}
	repoFolder = strings.TrimSpace(repoFolder)
	if repoFolder == "" || !filepath.IsAbs(repoFolder) {
		return fmt.Errorf("folder is not set")
	}
	repoFolder = filepath.Clean(repoFolder)
	target := filepath.Join(repoFolder, name)
	rel, err := filepath.Rel(repoFolder, target)
	if err != nil || rel != name {
		return fmt.Errorf("index folder is invalid")
	}
	info, err := os.Lstat(target)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if info.Mode()&os.ModeSymlink != 0 {
		return fmt.Errorf("%s is a link", name)
	}
	if !info.IsDir() {
		return fmt.Errorf("%s is not a folder", name)
	}
	return os.RemoveAll(target)
}

func indexDirName(index string) (string, error) {
	switch index {
	case "codegraph":
		return ".codegraph", nil
	case "obsidian":
		return ".obsidian", nil
	default:
		return "", fmt.Errorf("this repository has no index")
	}
}

func reposDir(setupPath string) string {
	return filepath.Join(filepath.Dir(setupPath), "repos")
}

func reposForFiles(snap console.Snapshot) []console.Repo {
	byKind := map[string]console.Repo{}
	for _, repo := range snap.Repos {
		byKind[repo.Kind] = repo
	}
	out := make([]console.Repo, 0, len(toolchain.Kinds))
	for _, kind := range toolchain.Kinds {
		repo := byKind[kind]
		repo.Kind = kind
		if repo.Kind == "document" {
			repo.Services = []string{}
		}
		if repo.Services == nil {
			repo.Services = []string{}
		}
		if repo.Index != "obsidian" && repo.Index != "codegraph" {
			repo.Index = ""
		}
		out = append(out, repo)
	}
	seen := map[string]bool{}
	extras := make([]console.Repo, 0)
	for _, repo := range snap.Repos {
		if toolchain.Known(repo.Kind) || !toolchain.RepoID(repo.Kind) || seen[repo.Kind] {
			continue
		}
		seen[repo.Kind] = true
		if repo.Services == nil {
			repo.Services = []string{}
		}
		if repo.Index != "obsidian" && repo.Index != "codegraph" {
			repo.Index = ""
		}
		extras = append(extras, repo)
	}
	sort.Slice(extras, func(i, j int) bool { return extras[i].Kind < extras[j].Kind })
	return append(out, extras...)
}

func mergeRepoFiles(setupPath string, snap console.Snapshot) (console.Snapshot, error) {
	files, err := readRepoFiles(setupPath)
	if err != nil {
		return snap, err
	}
	for i, repo := range snap.Repos {
		file, ok := files[repo.Kind]
		if !ok {
			continue
		}
		snap.Repos[i].Index = file.Index
		if sparse, err := console.SparsePath(file.Sparse); err == nil {
			snap.Repos[i].Sparse = sparse
		}
		if repo.Kind != "document" && file.Services != nil {
			snap.Repos[i].Services = file.Services
		}
	}
	return snap, nil
}

func readRepoFiles(setupPath string) (map[string]repoFile, error) {
	dir := reposDir(setupPath)
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return map[string]repoFile{}, nil
		}
		return nil, err
	}
	out := map[string]repoFile{}
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".yaml") {
			continue
		}
		raw, err := os.ReadFile(filepath.Join(dir, entry.Name()))
		if err != nil {
			return nil, err
		}
		var doc repoFile
		if err := yaml.Unmarshal(raw, &doc); err != nil {
			return nil, fmt.Errorf("read %s: %w", entry.Name(), err)
		}
		if !toolchain.RepoID(doc.ID) {
			doc.ID = strings.TrimSuffix(entry.Name(), ".yaml")
		}
		if !toolchain.RepoID(doc.ID) {
			continue
		}
		if doc.Index != "obsidian" && doc.Index != "codegraph" {
			doc.Index = ""
		}
		out[doc.ID] = doc
	}
	return out, nil
}

func writeRepoFiles(setupPath string, repos []console.Repo) error {
	dir := reposDir(setupPath)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	for _, repo := range repos {
		if !toolchain.RepoID(repo.Kind) {
			continue
		}
		text, err := repoFileText(repo)
		if err != nil {
			return err
		}
		path := filepath.Join(dir, repo.Kind+".yaml")
		existing, err := os.ReadFile(path)
		if err == nil && bytes.Equal(existing, text) {
			continue
		}
		if err != nil && !os.IsNotExist(err) {
			return err
		}
		tmp := path + ".tmp"
		if err := os.WriteFile(tmp, text, 0o644); err != nil {
			return err
		}
		if err := os.Rename(tmp, path); err != nil {
			return err
		}
	}
	return nil
}

func repoFileText(repo console.Repo) ([]byte, error) {
	services := repo.Services
	if services == nil || repo.Kind == "document" {
		services = []string{}
	}
	index := repo.Index
	if index != "obsidian" && index != "codegraph" {
		index = ""
	}
	sparse, err := console.SparsePath(repo.Sparse)
	if err != nil {
		sparse = ""
	}
	raw, err := yaml.Marshal(repoFile{
		ID:       repo.Kind,
		Folder:   strings.TrimSpace(repo.Folder),
		Services: services,
		Index:    index,
		Sparse:   sparse,
	})
	if err != nil {
		return nil, err
	}
	header := "# Repository " + repo.Kind + ". KIKI stores this in .kiki/repos. The index is what the launch prompt tells the model to query.\n"
	return []byte(header + string(raw)), nil
}
