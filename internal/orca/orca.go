// Package orca starts an embedded KIKI agent in the Orca app.
package orca

import (
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

// Client talks to the local Orca CLI.
type Client struct {
	Bin string
	Dir string
}

// LaunchArgs is `orca agent launch` for a terminal agent in this repository.
func LaunchArgs(runtime, root, prompt, model string) []string {
	args := []string{
		"agent", "launch",
		"--agent", runtime,
	}
	if strings.TrimSpace(model) != "" {
		args = append(args, "--model", model)
	}
	return append(args,
		"--worktree", "path:"+root,
		"--surface", "terminal",
		"--focus",
		"--prompt", prompt,
		"--json",
	)
}

// ProjectName is the Orca project that holds one repo for a workspace.
func ProjectName(workspace, repo string) (string, error) {
	workspace = strings.TrimSpace(workspace)
	repo = strings.TrimSpace(repo)
	if workspace == "" || repo == "" {
		return "", fmt.Errorf("workspace and repo are required to name the Orca project")
	}
	return workspace + "-" + repo, nil
}

var (
	featureStampPattern = regexp.MustCompile(`^[0-9]{10,14}$`)
	featureSlugPattern  = regexp.MustCompile(`^[a-z0-9][a-z0-9_]{0,79}$`)
)

// FeatureProjectName appends one shared timestamp to the project name for a feature start.
func FeatureProjectName(workspace, repo, stamp string) (string, error) {
	base, err := ProjectName(workspace, repo)
	if err != nil {
		return "", err
	}
	stamp = strings.TrimSpace(stamp)
	if !featureStampPattern.MatchString(stamp) {
		return "", fmt.Errorf("feature timestamp is not valid")
	}
	return base + "-" + stamp, nil
}

// FeatureBranch is the git branch for the worktree created for one repo in a feature start.
// The document repo is spelled docs.
func FeatureBranch(stamp, slug, kind string) (string, error) {
	stamp = strings.TrimSpace(stamp)
	slug = strings.TrimSpace(slug)
	if !featureStampPattern.MatchString(stamp) || !featureSlugPattern.MatchString(slug) {
		return "", fmt.Errorf("feature name is not valid")
	}
	repo := branchRepo(kind)
	if repo == "" || strings.ContainsAny(repo, " /") {
		return "", fmt.Errorf("feature repo is not valid")
	}
	return "feat/" + stamp + "-" + slug + "-" + repo, nil
}

func branchRepo(kind string) string {
	kind = strings.TrimSpace(kind)
	if kind == "document" {
		return "docs"
	}
	return kind
}

// Launch opens the repo folder in the named Orca project, starts the agent, and titles the tab.
func (c *Client) Launch(ctx context.Context, runtime, root, prompt, project, tab, model string) (string, error) {
	if err := c.ensureRunning(ctx); err != nil {
		return "", err
	}
	if err := c.ensureProject(ctx, root, project); err != nil {
		return "", err
	}
	out, err := c.run(ctx, LaunchArgs(runtime, root, prompt, model)...)
	if err != nil {
		return "", err
	}
	handle := terminalHandle(out)
	if handle == "" {
		return "", fmt.Errorf("Orca did not return a terminal for the agent")
	}
	if _, err := c.run(ctx, "terminal", "rename", "--terminal", handle, "--title", tab, "--json"); err != nil {
		return "", err
	}
	return string(out), nil
}

// LaunchFeature names the project with the shared timestamp, groups it under the feature folder,
// creates the feature worktree, and starts the agent there.
func (c *Client) LaunchFeature(ctx context.Context, runtime, root, prompt, project, tab, model, group, branch string) (string, error) {
	if err := c.ensureRunning(ctx); err != nil {
		return "", err
	}
	if err := c.ensureProject(ctx, root, project); err != nil {
		return "", err
	}
	if err := c.ensureGroup(ctx, group, project); err != nil {
		return "", err
	}
	checkout, err := c.ensureWorktree(ctx, root, project, branch)
	if err != nil {
		return "", err
	}
	out, err := c.run(ctx, LaunchArgs(runtime, checkout, prompt, model)...)
	if err != nil {
		return "", err
	}
	handle := terminalHandle(out)
	if handle == "" {
		return "", fmt.Errorf("Orca did not return a terminal for the agent")
	}
	if _, err := c.run(ctx, "terminal", "rename", "--terminal", handle, "--title", tab, "--json"); err != nil {
		return "", err
	}
	return string(out), nil
}

func (c *Client) ensureGroup(ctx context.Context, name, project string) error {
	name = strings.TrimSpace(name)
	if name == "" {
		return fmt.Errorf("feature folder name is required")
	}
	if c.Bin != "" {
		_, err := c.run(ctx, "project", "group", "--name", name, "--project", project, "--json")
		return err
	}
	return c.groupProject(ctx, name, project)
}

func (c *Client) ensureWorktree(ctx context.Context, root, project, branch string) (string, error) {
	listed, err := c.run(ctx, "worktree", "list", "--repo", "path:"+root, "--json")
	if err != nil {
		return "", err
	}
	if path := worktreePath(listed, branch); path != "" {
		return path, nil
	}
	name := strings.TrimPrefix(branch, "feat/")
	name = strings.ReplaceAll(name, "/", "-")
	created, err := c.run(ctx, "worktree", "create",
		"--project", project,
		"--name", name,
		"--branch", branch,
		"--no-parent",
		"--json",
	)
	if err != nil {
		return "", err
	}
	path := worktreePath(created, branch)
	if path == "" {
		path = worktreePath(created, "")
	}
	if path == "" {
		return "", fmt.Errorf("Orca created worktree %s but did not return its folder", branch)
	}
	return path, nil
}

func worktreePath(out []byte, branch string) string {
	var env struct {
		Result struct {
			Path      string        `json:"path"`
			Branch    string        `json:"branch"`
			Worktree  worktreeRow   `json:"worktree"`
			Worktrees []worktreeRow `json:"worktrees"`
		} `json:"result"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return ""
	}
	rows := env.Result.Worktrees
	if env.Result.Worktree.Path != "" {
		rows = append(rows, env.Result.Worktree)
	}
	if env.Result.Path != "" {
		rows = append(rows, worktreeRow{Path: env.Result.Path, Branch: env.Result.Branch})
	}
	if branch == "" {
		if len(rows) == 1 {
			return rows[0].Path
		}
		return ""
	}
	for _, row := range rows {
		if row.Path != "" && branchMatches(row.Branch, branch) {
			return row.Path
		}
	}
	return ""
}

type worktreeRow struct {
	Path   string `json:"path"`
	Branch string `json:"branch"`
}

func branchMatches(got, want string) bool {
	got = strings.TrimPrefix(got, "refs/heads/")
	want = strings.TrimPrefix(want, "refs/heads/")
	return got != "" && got == want
}

func (c *Client) ensureRunning(ctx context.Context) error {
	running, err := c.appRunning(ctx)
	if err != nil || !running {
		if _, openErr := c.run(ctx, "open", "--json"); openErr != nil && err != nil {
			return err
		} else if openErr != nil {
			return openErr
		}
		running, err = c.appRunning(ctx)
		if err != nil {
			return err
		}
		if !running {
			return fmt.Errorf("Orca is not running. Open the Orca app and try again")
		}
	}
	return nil
}

func (c *Client) ensureProject(ctx context.Context, root, name string) error {
	projects, err := c.projects(ctx)
	if err != nil {
		return err
	}
	setups, err := c.setups(ctx)
	if err != nil {
		return err
	}
	namedID := ""
	for _, project := range projects {
		if project.DisplayName == name {
			namedID = project.ID
			break
		}
	}
	var owned *hostSetup
	for i := range setups {
		if samePath(setups[i].Path, root) {
			owned = &setups[i]
			break
		}
	}
	if namedID != "" {
		if owned != nil && owned.ProjectID == namedID {
			return nil
		}
		return fmt.Errorf("Orca project %s already exists for a different folder", name)
	}
	if owned != nil {
		if owned.DisplayName == name {
			return nil
		}
		return c.renameSetup(ctx, owned.ID, name)
	}
	if c.Bin != "" {
		return c.addViaCLI(ctx, root, name)
	}
	return c.addFolderProject(ctx, root, name)
}

func (c *Client) addViaCLI(ctx context.Context, root, name string) error {
	out, err := c.run(ctx, "repo", "add", "--path", root, "--json")
	if err != nil {
		return err
	}
	id, display := addedRepo(out)
	if id == "" {
		return fmt.Errorf("Orca registered %s but did not return a project id", root)
	}
	if display == name {
		return nil
	}
	return c.renameSetup(ctx, id, name)
}

func (c *Client) renameSetup(ctx context.Context, id, name string) error {
	_, err := c.run(ctx, "project", "setup-update", "--setup", id, "--display-name", name, "--json")
	return err
}

type orcaProject struct {
	ID          string `json:"id"`
	DisplayName string `json:"displayName"`
}

type hostSetup struct {
	ID          string `json:"id"`
	ProjectID   string `json:"projectId"`
	DisplayName string `json:"displayName"`
	Path        string `json:"path"`
}

func (c *Client) projects(ctx context.Context) ([]orcaProject, error) {
	out, err := c.run(ctx, "project", "list", "--json")
	if err != nil {
		return nil, err
	}
	var env struct {
		Result struct {
			Projects []orcaProject `json:"projects"`
		} `json:"result"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return nil, fmt.Errorf("read Orca projects: %w", err)
	}
	return env.Result.Projects, nil
}

func (c *Client) setups(ctx context.Context) ([]hostSetup, error) {
	out, err := c.run(ctx, "project", "setups", "--json")
	if err != nil {
		return nil, err
	}
	var env struct {
		Result struct {
			Setups []hostSetup `json:"setups"`
		} `json:"result"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return nil, fmt.Errorf("read Orca project setups: %w", err)
	}
	return env.Result.Setups, nil
}

func addedRepo(out []byte) (id, display string) {
	var env struct {
		Result struct {
			Repo struct {
				ID          string `json:"id"`
				DisplayName string `json:"displayName"`
			} `json:"repo"`
		} `json:"result"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return "", ""
	}
	return env.Result.Repo.ID, env.Result.Repo.DisplayName
}

func terminalHandle(out []byte) string {
	var env struct {
		Result struct {
			Outcome struct {
				Handle string `json:"handle"`
			} `json:"outcome"`
		} `json:"result"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return ""
	}
	return env.Result.Outcome.Handle
}

func samePath(a, b string) bool {
	a = filepath.Clean(a)
	b = filepath.Clean(b)
	if a == b {
		return true
	}
	aa, errA := filepath.EvalSymlinks(a)
	bb, errB := filepath.EvalSymlinks(b)
	return errA == nil && errB == nil && filepath.Clean(aa) == filepath.Clean(bb)
}

func (c *Client) appRunning(ctx context.Context) (bool, error) {
	out, err := c.run(ctx, "status", "--json")
	if err != nil {
		return false, err
	}
	var env struct {
		Result struct {
			App struct {
				Running bool `json:"running"`
			} `json:"app"`
		} `json:"result"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return false, fmt.Errorf("read Orca status: %w", err)
	}
	return env.Result.App.Running, nil
}

func (c *Client) run(ctx context.Context, args ...string) ([]byte, error) {
	if _, ok := ctx.Deadline(); !ok {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, 90*time.Second)
		defer cancel()
	}
	bin := c.Bin
	if bin == "" {
		bin = "orca"
	}
	cmd := exec.CommandContext(ctx, bin, args...)
	if c.Dir != "" {
		cmd.Dir = c.Dir
	}
	out, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("orca %s: %w\n%s", args[0], err, trim(out))
	}
	if !jsonOK(out) {
		return nil, fmt.Errorf("orca %s failed\n%s", args[0], trim(out))
	}
	return out, nil
}

func jsonOK(out []byte) bool {
	var env struct {
		OK bool `json:"ok"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return false
	}
	return env.OK
}

func trim(out []byte) string {
	text := strings.TrimSpace(string(out))
	if len(text) > 4000 {
		return text[:4000] + "…"
	}
	return text
}
