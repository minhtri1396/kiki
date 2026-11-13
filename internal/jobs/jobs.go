// Package jobs runs Obsidian and CodeGraph setup, and checks whether they are ready.
package jobs

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"time"

	"kiki/internal/gitignore"
	"kiki/internal/toolchain"
)

// Job is one setup command.
type Job struct {
	ID         string `json:"id"`
	Kind       string `json:"kind"`
	Target     string `json:"target"`
	Status     string `json:"status"`
	Command    string `json:"command"`
	Log        string `json:"log"`
	StartedAt  string `json:"startedAt"`
	FinishedAt string `json:"finishedAt,omitempty"`
	Error      string `json:"error,omitempty"`
	cancel     context.CancelFunc
}

// Check is the result of one status command.
type Check struct {
	Ready     bool   `json:"ready"`
	DirExists bool   `json:"dirExists"`
	Summary   string `json:"summary"`
	CheckedAt string `json:"checkedAt,omitempty"`
}

// Target is one repository and the index saved for it.
// Index is obsidian, codegraph, or empty.
type Target struct {
	Kind   string
	Folder string
	Index  string
}

// Report is the last status check, keyed by repository kind.
type Report struct {
	Repos map[string]Check `json:"repos"`
}

// Manager runs setup commands.
type Manager struct {
	mu       sync.Mutex
	jobs     map[string]*Job
	order    []string
	running  map[string]string
	last     Report
	lookPath func(string) (string, error)
}

// New returns an empty manager.
func New() *Manager {
	return &Manager{
		jobs:     map[string]*Job{},
		running:  map[string]string{},
		lookPath: lookPath,
		last: Report{
			Repos: map[string]Check{},
		},
	}
}

// Last returns the most recent status check.
func (m *Manager) Last() Report {
	m.mu.Lock()
	defer m.mu.Unlock()
	return copyReport(m.last)
}

// List returns jobs, newest first.
func (m *Manager) List() []Job {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := make([]Job, 0, len(m.order))
	for i := len(m.order) - 1; i >= 0; i-- {
		job := m.jobs[m.order[i]]
		out = append(out, snapshot(job))
	}
	return out
}

// Get returns one job.
func (m *Manager) Get(id string) (Job, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	job, ok := m.jobs[id]
	if !ok {
		return Job{}, false
	}
	return snapshot(job), true
}

// Cancel stops a running job.
func (m *Manager) Cancel(id string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	job, ok := m.jobs[id]
	if !ok || job.Status != "running" || job.cancel == nil {
		return false
	}
	job.cancel()
	return true
}

// StartObsidian makes one repository folder an Obsidian vault, then checks the CLI.
func (m *Manager) StartObsidian(kind, folder string) (Job, error) {
	if !toolchain.Known(kind) {
		return Job{}, fmt.Errorf("invalid repo")
	}
	folder = strings.TrimSpace(folder)
	if folder == "" {
		return Job{}, fmt.Errorf("folder is not set")
	}
	cli, err := toolchain.InstallObsidianCommand()
	if err != nil {
		return Job{}, err
	}
	info, err := os.Stat(folder)
	if err != nil || !info.IsDir() {
		return Job{}, fmt.Errorf("folder %s does not exist", folder)
	}
	ignorePath, err := gitignore.Index(folder, ".codegraph", ".obsidian")
	if err != nil {
		return Job{}, err
	}
	note, err := toolchain.EnsureVault(folder)
	if err != nil {
		return Job{}, err
	}
	if cli != "" {
		note = note + "\nObsidian CLI: " + cli
	}
	if ignorePath != "" {
		note = note + "\nGit ignore: " + ignorePath
	}
	if err := toolchain.OpenVault(cli, folder); err != nil {
		return Job{}, fmt.Errorf("%s (%w)", note, err)
	}
	job, err := m.start("obsidian", kind, folder, toolchain.ObsidianStatus(folder), note)
	if err != nil {
		return Job{}, fmt.Errorf("%s (%w)", note, err)
	}
	return job, nil
}

// StartCodegraph runs codegraph init for one repository folder.
func (m *Manager) StartCodegraph(root, kind, folder string) (Job, error) {
	if !toolchain.Known(kind) {
		return Job{}, fmt.Errorf("invalid repo")
	}
	if strings.TrimSpace(folder) == "" {
		return Job{}, fmt.Errorf("folder is not set")
	}
	info, err := os.Stat(folder)
	if err != nil || !info.IsDir() {
		return Job{}, fmt.Errorf("folder %s does not exist", folder)
	}
	ignorePath, err := gitignore.Index(folder, ".codegraph", ".obsidian")
	if err != nil {
		return Job{}, err
	}
	note := ""
	if ignorePath != "" {
		note = "Git ignore: " + ignorePath
	}
	return m.start("codegraph", kind, root, toolchain.CodegraphInit(folder), note)
}

// Check runs the status command for the index saved on each repository.
func (m *Manager) Check(ctx context.Context, root string, targets []Target) (Report, error) {
	report := Report{Repos: map[string]Check{}}
	var mu sync.Mutex
	var wg sync.WaitGroup
	for _, target := range targets {
		wg.Add(1)
		go func(target Target) {
			defer wg.Done()
			check := m.checkTarget(ctx, root, target)
			mu.Lock()
			report.Repos[target.Kind] = check
			mu.Unlock()
		}(target)
	}
	wg.Wait()

	m.mu.Lock()
	m.last = copyReport(report)
	m.mu.Unlock()
	return report, nil
}

// RefreshFolders reports whether each repository using index still has its
// .obsidian or .codegraph folder. Other results stay as they were.
func (m *Manager) RefreshFolders(targets []Target, index string) (Report, error) {
	name, err := indexFolderName(index)
	if err != nil {
		return Report{}, err
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.last.Repos == nil {
		m.last.Repos = map[string]Check{}
	}
	for _, target := range targets {
		if target.Index != index {
			continue
		}
		folder := strings.TrimSpace(target.Folder)
		exists := false
		summary := name + " is missing"
		if folder == "" {
			summary = "folder is not set"
		} else if isDir(filepath.Join(folder, name)) {
			exists = true
			summary = name + " exists"
		}
		m.last.Repos[target.Kind] = Check{
			Ready:     exists,
			DirExists: exists,
			Summary:   summary,
			CheckedAt: now(),
		}
	}
	return copyReport(m.last), nil
}

func indexFolderName(index string) (string, error) {
	switch index {
	case "obsidian":
		return ".obsidian", nil
	case "codegraph":
		return ".codegraph", nil
	default:
		return "", fmt.Errorf("unknown index")
	}
}

// CheckOne runs the status command for one repository and keeps the other results.
func (m *Manager) CheckOne(ctx context.Context, root string, targets []Target, kind string) (Report, error) {
	var found bool
	var target Target
	for _, item := range targets {
		if item.Kind == kind {
			target = item
			found = true
			break
		}
	}
	if !found || !toolchain.Known(kind) {
		return Report{}, fmt.Errorf("unknown repository")
	}
	check := m.checkTarget(ctx, root, target)
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.last.Repos == nil {
		m.last.Repos = map[string]Check{}
	}
	m.last.Repos[kind] = check
	return copyReport(m.last), nil
}

func (m *Manager) checkTarget(ctx context.Context, root string, target Target) Check {
	folder := strings.TrimSpace(target.Folder)
	switch target.Index {
	case "":
		return Check{Summary: "No index is set for this repository", CheckedAt: now()}
	case "obsidian":
		return m.runObsidianCheck(ctx, folder)
	case "codegraph":
		if folder == "" {
			return Check{Summary: "folder is not set", CheckedAt: now()}
		}
		return m.runCheck(ctx, root, filepath.Join(folder, ".codegraph"), toolchain.CodegraphStatus(folder), "not initialized")
	default:
		return Check{Summary: "unknown index", CheckedAt: now()}
	}
}

func (m *Manager) runObsidianCheck(ctx context.Context, folder string) Check {
	if folder == "" {
		return Check{Summary: "folder is not set", CheckedAt: now()}
	}
	abs, err := filepath.Abs(folder)
	if err != nil {
		abs = folder
	}
	indexDir := filepath.Join(abs, ".obsidian")
	exists := isDir(indexDir)
	args := toolchain.ObsidianStatus(abs)
	bin, err := m.lookPath(args[0])
	if err != nil {
		return Check{
			DirExists: exists,
			Summary:   fmt.Sprintf("command %s was not found on PATH", args[0]),
			CheckedAt: now(),
		}
	}
	cctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	cmd := exec.CommandContext(cctx, bin, args[1:]...)
	cmd.Dir = abs
	cmd.Env = commandEnv()
	out, runErr := cmd.CombinedOutput()
	text := cleanOutput(string(out))
	ready := exists && runErr == nil && pathMentioned(text, abs)
	if !ready && strings.Contains(text, "Vault not found") && cctx.Err() == nil {
		if err := toolchain.OpenVault(bin, abs); err == nil {
			again := exec.CommandContext(cctx, bin, args[1:]...)
			again.Dir = abs
			again.Env = commandEnv()
			out, runErr = again.CombinedOutput()
			text = cleanOutput(string(out))
			ready = exists && runErr == nil && pathMentioned(text, abs)
		}
	}
	summary := firstLine(text)
	switch {
	case runErr != nil && summary == "":
		summary = runErr.Error()
	case !exists && summary == "":
		summary = "Vault folder is missing"
	case !ready && summary == "":
		summary = "Obsidian did not report this document folder"
	case ready && summary == "":
		summary = "Ready"
	}
	if len(summary) > 400 {
		summary = summary[:400]
	}
	return Check{
		Ready:     ready,
		DirExists: exists,
		Summary:   summary,
		CheckedAt: now(),
	}
}

func pathMentioned(text, folder string) bool {
	folder = filepath.Clean(folder)
	if strings.Contains(text, folder) {
		return true
	}
	resolved, err := filepath.EvalSymlinks(folder)
	if err != nil {
		return false
	}
	resolved = filepath.Clean(resolved)
	return resolved != folder && strings.Contains(text, resolved)
}

func (m *Manager) runCheck(ctx context.Context, dir, indexDir string, args []string, phrase string) Check {
	exists := isDir(indexDir)
	bin, err := m.lookPath(args[0])
	if err != nil {
		return Check{
			DirExists: exists,
			Summary:   fmt.Sprintf("command %s was not found on PATH", args[0]),
			CheckedAt: now(),
		}
	}
	cctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	cmd := exec.CommandContext(cctx, bin, args[1:]...)
	cmd.Dir = dir
	cmd.Env = commandEnv()
	out, runErr := cmd.CombinedOutput()
	return evaluate(exists, string(out), runErr, phrase)
}

func evaluate(dirExists bool, output string, runErr error, phrase string) Check {
	text := cleanOutput(output)
	missing := strings.Contains(strings.ToLower(text), phrase)
	ready := dirExists && runErr == nil && !missing
	summary := firstLine(text)
	switch {
	case missing:
		summary = "The command reports that setup is missing (" + phrase + ")"
	case runErr != nil && summary == "":
		summary = runErr.Error()
	case !dirExists && summary == "":
		summary = "Index folder is missing"
	case ready && summary == "":
		summary = "Ready"
	}
	if len(summary) > 400 {
		summary = summary[:400]
	}
	return Check{
		Ready:     ready,
		DirExists: dirExists,
		Summary:   summary,
		CheckedAt: now(),
	}
}

func (m *Manager) start(kind, target, dir string, args []string, prefix string) (Job, error) {
	bin, err := m.lookPath(args[0])
	if err != nil {
		return Job{}, fmt.Errorf("command %s was not found on PATH", args[0])
	}
	key := kind + ":" + target
	m.mu.Lock()
	defer m.mu.Unlock()
	if id, ok := m.running[key]; ok {
		return snapshot(m.jobs[id]), nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), 45*time.Minute)
	job := &Job{
		ID:        fmt.Sprintf("%s-%s-%d", kind, target, time.Now().UnixNano()),
		Kind:      kind,
		Target:    target,
		Status:    "running",
		Command:   formatArgs(args),
		Log:       prefixLog(prefix),
		StartedAt: now(),
		cancel:    cancel,
	}
	m.jobs[job.ID] = job
	m.order = append(m.order, job.ID)
	m.running[key] = job.ID
	m.trimLocked()
	go m.run(ctx, job, key, dir, append([]string{bin}, args[1:]...))
	return snapshot(job), nil
}

func (m *Manager) trimLocked() {
	const maxJobs = 30
	var finished, running []string
	for _, id := range m.order {
		job := m.jobs[id]
		if job != nil && job.Status == "running" {
			running = append(running, id)
			continue
		}
		finished = append(finished, id)
	}
	if len(finished) > maxJobs {
		for _, id := range finished[:len(finished)-maxJobs] {
			delete(m.jobs, id)
		}
		finished = finished[len(finished)-maxJobs:]
	}
	m.order = append(finished, running...)
}

func (m *Manager) run(ctx context.Context, job *Job, key, dir string, args []string) {
	cmd := exec.CommandContext(ctx, args[0], args[1:]...)
	cmd.Dir = dir
	cmd.Env = commandEnv()
	cmd.Stdout = &lockedWriter{m: m, job: job}
	cmd.Stderr = cmd.Stdout
	err := cmd.Run()

	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.running, key)
	job.FinishedAt = now()
	switch {
	case ctx.Err() == context.Canceled:
		job.Status = "cancelled"
		job.Error = "cancelled"
	case err != nil:
		job.Status = "error"
		job.Error = err.Error()
		if strings.TrimSpace(job.Log) == "" {
			job.Log = err.Error()
		}
	default:
		job.Status = "ok"
		if job.Kind == "obsidian" && strings.Contains(job.Log, "Vault not found.") {
			job.Status = "error"
			job.Error = "Vault not found."
		}
	}
}

type lockedWriter struct {
	m   *Manager
	job *Job
}

func (w *lockedWriter) Write(p []byte) (int, error) {
	w.m.mu.Lock()
	defer w.m.mu.Unlock()
	w.job.Log += string(p)
	if len(w.job.Log) > 512*1024 {
		w.job.Log = w.job.Log[len(w.job.Log)-400*1024:]
	}
	return len(p), nil
}

func prefixLog(prefix string) string {
	prefix = strings.TrimSpace(prefix)
	if prefix == "" {
		return ""
	}
	return prefix + "\n"
}

func snapshot(job *Job) Job {
	if job == nil {
		return Job{}
	}
	copyJob := *job
	copyJob.cancel = nil
	return copyJob
}

func copyReport(in Report) Report {
	out := in
	out.Repos = map[string]Check{}
	for key, value := range in.Repos {
		out.Repos[key] = value
	}
	return out
}

func isDir(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.IsDir()
}

func firstLine(text string) string {
	text = cleanOutput(text)
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSpace(line)
		if line != "" {
			return line
		}
	}
	return ""
}

var ansiEscape = regexp.MustCompile(`\x1b\[[0-9;]*[A-Za-z]`)

func cleanOutput(text string) string {
	return strings.TrimSpace(ansiEscape.ReplaceAllString(text, ""))
}

func now() string {
	return time.Now().UTC().Format(time.RFC3339)
}

func formatArgs(args []string) string {
	quoted := make([]string, len(args))
	for i, arg := range args {
		if strings.ContainsAny(arg, " \t") {
			quoted[i] = fmt.Sprintf("%q", arg)
		} else {
			quoted[i] = arg
		}
	}
	return strings.Join(quoted, " ")
}

func lookPath(name string) (string, error) {
	for _, dir := range filepath.SplitList(pathValue()) {
		candidate := filepath.Join(dir, name)
		if executableFile(candidate) {
			return candidate, nil
		}
	}
	if name == "obsidian" {
		if bundled := toolchain.BundledObsidianCLI(); bundled != "" {
			return bundled, nil
		}
	}
	return "", fmt.Errorf("%s not found", name)
}

func executableFile(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir() && info.Mode()&0o111 != 0
}

func pathValue() string {
	for _, item := range commandEnv() {
		if strings.HasPrefix(item, "PATH=") {
			return strings.TrimPrefix(item, "PATH=")
		}
	}
	return os.Getenv("PATH")
}

func commandEnv() []string {
	home, _ := os.UserHomeDir()
	extra := []string{
		"/opt/homebrew/bin",
		"/usr/local/bin",
		filepath.Join(home, ".npm-global", "bin"),
		filepath.Join(home, ".local", "bin"),
	}
	base := os.Getenv("PATH")
	merged := strings.Join(append(extra, base), string(os.PathListSeparator))
	out := make([]string, 0, len(os.Environ())+1)
	for _, item := range os.Environ() {
		if strings.HasPrefix(item, "PATH=") {
			continue
		}
		out = append(out, item)
	}
	return append(out, "PATH="+merged)
}

// FormatArgs joins a command for display.
func FormatArgs(args []string) string {
	return formatArgs(args)
}
