package orca

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
)

// TaskScope is the Orca state created for one Kiki task.
// ID is the worktree name. GroupID is the sidebar folder.
// Folders maps a repo kind to that repo's folder.
// KeepGroup leaves the folder in place. KeepRepos leaves those projects in place.
type TaskScope struct {
	ID        string
	GroupID   string
	Workspace string
	Slug      string
	Ticket    string
	Repos     []string
	Folders   map[string]string
	Terminals []string
	KeepGroup bool
	KeepRepos []string
}

// RemoveTask closes the task's agent terminals and removes its worktrees.
// The group folder and repo projects stay when another task in the group still uses them.
func (c *Client) RemoveTask(ctx context.Context, scope TaskScope) error {
	if c == nil {
		c = &Client{}
	}
	if err := c.ensureRunning(ctx); err != nil {
		return err
	}
	seenTerminal := map[string]bool{}
	for _, handle := range scope.Terminals {
		handle = strings.TrimSpace(handle)
		if handle == "" || seenTerminal[handle] {
			continue
		}
		seenTerminal[handle] = true
		if err := c.runGone(ctx, "terminal", "close", "--terminal", handle, "--tab", "--json"); err != nil {
			return err
		}
	}
	keep := map[string]bool{}
	for _, kind := range scope.KeepRepos {
		kind = strings.TrimSpace(kind)
		if kind != "" {
			keep[kind] = true
		}
	}
	seenRepo := map[string]bool{}
	for _, kind := range scope.Repos {
		kind = strings.TrimSpace(kind)
		if kind == "" || seenRepo[kind] {
			continue
		}
		seenRepo[kind] = true
		branch := ""
		if ValidBranchSlug(scope.Slug) {
			var err error
			branch, err = FeatureBranch(scope.Ticket, scope.Slug, kind)
			if err != nil {
				return err
			}
		}
		root := ""
		if scope.Folders != nil {
			root = scope.Folders[kind]
		}
		if err := c.removeTaskWorktree(ctx, root, scope.ID, branch); err != nil {
			return err
		}
		if keep[kind] || strings.TrimSpace(scope.GroupID) == "" {
			continue
		}
		name, err := FeatureProjectName(scope.Workspace, scope.GroupID, kind)
		if err != nil {
			return err
		}
		setups, err := c.setups(ctx)
		if err != nil {
			return err
		}
		for _, setup := range setups {
			if setup.DisplayName != name || strings.TrimSpace(setup.ID) == "" {
				continue
			}
			if err := c.runGone(ctx, "project", "setup-delete", "--setup", setup.ID, "--force", "--json"); err != nil {
				return err
			}
		}
	}
	if scope.KeepGroup || strings.TrimSpace(scope.GroupID) == "" {
		return nil
	}
	return c.deleteGroup(ctx, scope.GroupID)
}

func (c *Client) removeTaskWorktree(ctx context.Context, root, name, branch string) error {
	root = strings.TrimSpace(root)
	name = strings.TrimSpace(name)
	if root != "" && name != "" {
		out, err := c.commandOutput(ctx, "worktree", "list", "--repo", "path:"+root, "--json")
		if err == nil {
			if path := worktreeNamed(out, name); path != "" {
				return c.runGone(ctx, "worktree", "rm", "--worktree", "path:"+path, "--json")
			}
		}
	}
	if branch != "" {
		return c.runGone(ctx, "worktree", "rm", "--worktree", "branch:"+branch, "--json")
	}
	if name != "" {
		return c.runGone(ctx, "worktree", "rm", "--worktree", "name:"+name, "--json")
	}
	return nil
}

func (c *Client) deleteGroup(ctx context.Context, name string) error {
	name = strings.TrimSpace(name)
	if name == "" {
		return nil
	}
	raw, err := c.runtime(ctx, "projectGroup.list", nil)
	if err != nil {
		return err
	}
	var listed struct {
		Groups []struct {
			ID   string `json:"id"`
			Name string `json:"name"`
		} `json:"groups"`
	}
	if err := json.Unmarshal(raw, &listed); err != nil {
		return fmt.Errorf("read Orca project groups: Orca returned an unreadable response")
	}
	for _, group := range listed.Groups {
		if group.Name != name || strings.TrimSpace(group.ID) == "" {
			continue
		}
		if _, err := c.runtime(ctx, "projectGroup.delete", map[string]any{"groupId": group.ID}); err != nil {
			if orcaGoneText(err.Error()) {
				continue
			}
			return fmt.Errorf("remove Orca folder %s: %w", name, err)
		}
	}
	return nil
}

func (c *Client) runGone(ctx context.Context, args ...string) error {
	out, err := c.commandOutput(ctx, args...)
	if err != nil {
		return err
	}
	if jsonOK(out) || orcaGone(out) {
		return nil
	}
	return fmt.Errorf("orca %s failed\n%s", args[0], trim(out))
}

func orcaGone(out []byte) bool {
	var env struct {
		Error struct {
			Code    string `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.Unmarshal(out, &env); err != nil {
		return false
	}
	return orcaGoneText(env.Error.Code) || orcaGoneText(env.Error.Message)
}

func orcaGoneText(text string) bool {
	text = strings.ToLower(strings.TrimSpace(text))
	if text == "" {
		return false
	}
	if text == "terminal_handle_stale" || strings.Contains(text, "not_found") || strings.Contains(text, "not found") {
		return true
	}
	return false
}
