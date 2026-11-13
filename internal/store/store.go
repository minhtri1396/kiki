// Package store keeps the console setup in a YAML file and run history in SQLite.
package store

import (
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"kiki/internal/console"
	"kiki/internal/toolchain"

	"gopkg.in/yaml.v3"
)

// Store is the console setup file and the run log.
type Store struct {
	path    string
	logPath string
	db      *sql.DB
	mu      sync.Mutex
}

// Launch is one recorded agent start.
type Launch struct {
	ID        int64  `json:"id" yaml:"id"`
	AgentID   string `json:"agentId" yaml:"agent_id"`
	Runtime   string `json:"runtime" yaml:"runtime"`
	Note      string `json:"note" yaml:"note"`
	Status    string `json:"status" yaml:"status"`
	Detail    string `json:"detail" yaml:"detail"`
	CreatedAt string `json:"createdAt" yaml:"created_at"`
}

// TeamFolder is a local folder the user syncs between computers.
// Path is the team folder. User is this person's folder inside it.
// Member is the member folder last copied into the local agents folder.
type TeamFolder struct {
	Path   string `yaml:"path,omitempty"`
	User   string `yaml:"user,omitempty"`
	Member string `yaml:"member,omitempty"`
}

type fileDoc struct {
	WorkspaceID   string     `yaml:"workspace_id"`
	OrcaAgent     string     `yaml:"orca_agent"`
	AgentPath     string     `yaml:"agent_path,omitempty"`
	GenerateTool  string     `yaml:"generate_tool,omitempty"`
	GenerateModel string     `yaml:"generate_model,omitempty"`
	Repos         []repoDoc  `yaml:"repos"`
	TeamFolder    TeamFolder `yaml:"team_folder,omitempty"`
}

type repoDoc struct {
	Kind     string   `yaml:"kind"`
	Folder   string   `yaml:"folder,omitempty"`
	Services []string `yaml:"services,omitempty"`
}

// Open opens or creates the setup file.
func Open(path string) (*Store, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	s := &Store{
		path:    path,
		logPath: filepath.Join(filepath.Dir(path), "kiki.db"),
	}
	if _, err := os.Stat(path); err != nil {
		if !os.IsNotExist(err) {
			return nil, err
		}
		if err := s.write(defaultDoc()); err != nil {
			return nil, err
		}
	}
	if _, err := s.read(); err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", sqliteDSN(s.logPath))
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(1)
	s.db = db
	if err := s.db.Ping(); err != nil {
		_ = db.Close()
		return nil, err
	}
	if err := s.initLogs(); err != nil {
		_ = db.Close()
		return nil, err
	}
	if err := s.migrateLaunches(); err != nil {
		_ = db.Close()
		return nil, err
	}
	return s, nil
}

// Close releases the run log.
func (s *Store) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.db == nil {
		return nil
	}
	err := s.db.Close()
	s.db = nil
	return err
}

// DataDir is the .kiki folder next to the app. KIKI stores the files it creates there.
func DataDir(dir string) string {
	return filepath.Join(dir, ".kiki")
}

// Path is kiki.yaml inside the .kiki folder.
func Path(dir string) string {
	return filepath.Join(DataDir(dir), "kiki.yaml")
}

// File returns the setup file path.
func (s *Store) File() string {
	return s.path
}

// LogFile returns the SQLite file that stores recent runs, next to the setup file.
func (s *Store) LogFile() string {
	return s.logPath
}

// EnsureLayout creates backend, website, mobile, and docs under root when they
// are missing, and stores those paths. Service names already in the file stay.
func (s *Store) EnsureLayout(root string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	doc, err := s.read()
	if err != nil {
		return err
	}
	snap, err := console.ApplyRoot(root, doc.snapshot())
	if err != nil {
		return err
	}
	snap, err = mergeRepoFiles(s.path, snap)
	if err != nil {
		return err
	}
	if err := writeRepoFiles(s.path, reposForFiles(snap)); err != nil {
		return err
	}
	next := reposFrom(snap)
	if reposEqual(doc.Repos, next) {
		return nil
	}
	doc.Repos = next
	return s.write(doc)
}

// Load reads the saved setup. Root is filled by the caller.
func (s *Store) Load() (console.Snapshot, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	doc, err := s.read()
	if err != nil {
		return console.Snapshot{}, err
	}
	return mergeRepoFiles(s.path, doc.snapshot())
}

// TeamFolder returns the saved local team folder.
func (s *Store) TeamFolder() (TeamFolder, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	doc, err := s.read()
	if err != nil {
		return TeamFolder{}, err
	}
	return doc.TeamFolder, nil
}

// SaveTeamFolder stores the team folder without changing the rest of the setup.
func (s *Store) SaveTeamFolder(cfg TeamFolder) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	doc, err := s.read()
	if err != nil {
		return err
	}
	doc.TeamFolder = trimTeamFolder(cfg)
	return s.write(doc)
}

func trimTeamFolder(cfg TeamFolder) TeamFolder {
	cfg.Path = strings.TrimSpace(cfg.Path)
	cfg.User = strings.TrimSpace(cfg.User)
	cfg.Member = strings.TrimSpace(cfg.Member)
	return cfg
}

// Save replaces the setup. Run history stays in the SQLite file.
func (s *Store) Save(snap console.Snapshot) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	doc, err := s.read()
	if err != nil {
		return err
	}
	doc.WorkspaceID = snap.WorkspaceID
	doc.OrcaAgent = snap.OrcaAgent
	doc.AgentPath = snap.AgentPath
	doc.GenerateTool = strings.TrimSpace(snap.GenerateTool)
	doc.GenerateModel = strings.TrimSpace(snap.GenerateModel)
	doc.Repos = reposFrom(snap)
	if err := s.write(doc); err != nil {
		return err
	}
	return writeRepoFiles(s.path, reposForFiles(snap))
}

func (s *Store) read() (fileDoc, error) {
	raw, err := os.ReadFile(s.path)
	if err != nil {
		return fileDoc{}, err
	}
	var doc fileDoc
	if err := yaml.Unmarshal(raw, &doc); err != nil {
		return fileDoc{}, fmt.Errorf("read setup file: %w", err)
	}
	if doc.WorkspaceID == "" {
		doc.WorkspaceID = "kiki"
	}
	if doc.OrcaAgent == "" {
		doc.OrcaAgent = "cursor"
	}
	doc.Repos = normalizeRepos(doc.Repos)
	return doc, nil
}

func (s *Store) write(doc fileDoc) error {
	doc.Repos = normalizeRepos(doc.Repos)
	raw, err := yaml.Marshal(doc)
	if err != nil {
		return err
	}
	text := "# KIKI setup. This file is saved in the .kiki folder next to the app. Agents read it.\n" + string(raw)
	tmp := s.path + ".tmp"
	if err := os.WriteFile(tmp, []byte(text), 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, s.path)
}

func defaultDoc() fileDoc {
	return fileDoc{
		WorkspaceID: "kiki",
		OrcaAgent:   "cursor",
		Repos:       normalizeRepos(nil),
	}
}

func normalizeRepos(repos []repoDoc) []repoDoc {
	byKind := map[string]repoDoc{}
	for _, repo := range repos {
		byKind[repo.Kind] = repo
	}
	out := make([]repoDoc, 0, len(toolchain.Kinds))
	for _, kind := range toolchain.Kinds {
		repo := byKind[kind]
		repo.Kind = kind
		if repo.Services == nil {
			repo.Services = []string{}
		}
		out = append(out, repo)
	}
	seen := map[string]bool{}
	extras := make([]repoDoc, 0)
	for _, repo := range repos {
		kind := strings.TrimSpace(repo.Kind)
		if toolchain.Known(kind) || !toolchain.RepoID(kind) || seen[kind] {
			continue
		}
		seen[kind] = true
		repo.Kind = kind
		if repo.Services == nil {
			repo.Services = []string{}
		}
		extras = append(extras, repo)
	}
	sort.Slice(extras, func(i, j int) bool { return extras[i].Kind < extras[j].Kind })
	return append(out, extras...)
}

func reposEqual(a, b []repoDoc) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i].Kind != b[i].Kind || a[i].Folder != b[i].Folder || !sameStrings(a[i].Services, b[i].Services) {
			return false
		}
	}
	return true
}

func sameStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func reposFrom(snap console.Snapshot) []repoDoc {
	out := make([]repoDoc, 0, len(snap.Repos))
	for _, repo := range snap.Repos {
		services := repo.Services
		if services == nil {
			services = []string{}
		}
		out = append(out, repoDoc{Kind: repo.Kind, Folder: repo.Folder, Services: services})
	}
	return normalizeRepos(out)
}

func now() string {
	return time.Now().UTC().Format(time.RFC3339)
}

func (d fileDoc) snapshot() console.Snapshot {
	snap := console.Snapshot{
		WorkspaceID:   d.WorkspaceID,
		OrcaAgent:     d.OrcaAgent,
		AgentPath:     d.AgentPath,
		GenerateTool:  strings.TrimSpace(d.GenerateTool),
		GenerateModel: strings.TrimSpace(d.GenerateModel),
		Repos:         make([]console.Repo, 0, len(d.Repos)),
	}
	for _, repo := range d.Repos {
		services := repo.Services
		if services == nil {
			services = []string{}
		}
		snap.Repos = append(snap.Repos, console.Repo{
			Kind:     repo.Kind,
			Folder:   repo.Folder,
			Services: services,
		})
	}
	return snap
}

// DetailLimit keeps launch details small enough for the UI.
func DetailLimit(text string) string {
	const max = 8000
	if len(text) <= max {
		return text
	}
	return text[:max] + fmt.Sprintf("\n… (%d bytes truncated)", len(text)-max)
}
