// Package jobs runs GitNexus and CodeGraph setup, and checks whether they are ready.
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

// Report is the last status check.
type Report struct {
	GitNexus  Check            `json:"gitnexus"`
	Codegraph map[string]Check `json:"codegraph"`
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
			Codegraph: map[string]Check{},
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

// StartGitNexus runs gitnexus analyze at the repository root.
// name is the workspace id from the setup YAML.
func (m *Manager) StartGitNexus(root, name string) (Job, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return Job{}, fmt.Errorf("workspace id is not set")
	}
	return m.start("gitnexus", "root", root, toolchain.GitNexusAnalyze(name))
}

// StartCodegraph runs codegraph init for one repository folder.
func (m *Manager) StartCodegraph(root, kind, folder string) (Job, error) {
	if !knownKind(kind) {
		return Job{}, fmt.Errorf("invalid repo")
	}
	if strings.TrimSpace(folder) == "" {
		return Job{}, fmt.Errorf("folder is not set")
	}
	info, err := os.Stat(folder)
	if err != nil || !info.IsDir() {
		return Job{}, fmt.Errorf("folder %s does not exist", folder)
	}
	return m.start("codegraph", kind, root, toolchain.CodegraphInit(folder))
}

// Check runs the status commands the agent documents use.
// folders maps backend, website, and mobile to the chosen repository path.
func (m *Manager) Check(ctx context.Context, root string, folders map[string]string) (Report, error) {
	report := Report{Codegraph: map[string]Check{}}
	var mu sync.Mutex
	var wg sync.WaitGroup

	wg.Add(1)
	go func() {
		defer wg.Done()
		dir := filepath.Join(root, ".gitnexus")
		check := m.runCheck(ctx, root, dir, toolchain.GitNexusStatus(), "not indexed")
		mu.Lock()
		report.GitNexus = check
		mu.Unlock()
	}()
	for _, kind := range toolchain.CodeKinds {
		wg.Add(1)
		go func(kind string) {
			defer wg.Done()
			folder := strings.TrimSpace(folders[kind])
			var check Check
			if folder == "" {
				check = Check{Summary: "folder is not set", CheckedAt: now()}
			} else {
				check = m.runCheck(ctx, root, filepath.Join(folder, ".codegraph"), toolchain.CodegraphStatus(folder), "not initialized")
			}
			mu.Lock()
			report.Codegraph[kind] = check
			mu.Unlock()
		}(kind)
	}
	wg.Wait()

	m.mu.Lock()
	m.last = copyReport(report)
	m.mu.Unlock()
	return report, nil
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

func (m *Manager) start(kind, target, dir string, args []string) (Job, error) {
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
	out.Codegraph = map[string]Check{}
	for key, value := range in.Codegraph {
		out.Codegraph[key] = value
	}
	return out
}

func knownKind(kind string) bool {
	for _, item := range toolchain.CodeKinds {
		if item == kind {
			return true
		}
	}
	return false
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
		info, err := os.Stat(candidate)
		if err != nil || info.IsDir() || info.Mode()&0o111 == 0 {
			continue
		}
		return candidate, nil
	}
	return "", fmt.Errorf("%s not found", name)
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
