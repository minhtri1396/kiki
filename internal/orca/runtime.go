package orca

import (
	"bufio"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"runtime"
	"time"
)

// addFolderProject registers a plain folder as an Orca project with the requested name.
// The CLI repo command only imports Git repositories and names them from the Git remote,
// so a folder project is created through the local Orca runtime.
func (c *Client) addFolderProject(ctx context.Context, root, name string) error {
	raw, err := c.call(ctx, "repo.add", map[string]string{
		"path":        root,
		"kind":        "folder",
		"displayName": name,
	})
	if err != nil {
		return fmt.Errorf("create Orca project %s: %w", name, err)
	}
	var result struct {
		Repo struct {
			DisplayName string `json:"displayName"`
		} `json:"repo"`
	}
	if err := json.Unmarshal(raw, &result); err != nil {
		return fmt.Errorf("create Orca project %s: Orca returned an unreadable response", name)
	}
	if result.Repo.DisplayName != name {
		return fmt.Errorf("create Orca project %s: Orca named it %s", name, result.Repo.DisplayName)
	}
	return nil
}

// groupProject puts the named project in one Orca sidebar folder. The folder is created when missing.
// projectGroup.moveProject addresses the registered repo. A durable project id
// (github:owner/repo or repo:<id>) is not a repo selector and comes back as repo_not_found.
func (c *Client) groupProject(ctx context.Context, name, displayName string) error {
	projects, err := c.projects(ctx)
	if err != nil {
		return err
	}
	repoID := ""
	found := false
	for _, project := range projects {
		if project.DisplayName != displayName {
			continue
		}
		found = true
		if len(project.SourceRepoIDs) > 0 {
			repoID = project.SourceRepoIDs[0]
		} else {
			repoID = displayName
		}
		break
	}
	if !found {
		return fmt.Errorf("Orca project %s was not found", displayName)
	}
	raw, err := c.call(ctx, "projectGroup.list", nil)
	if err != nil {
		return fmt.Errorf("group Orca projects in %s: %w", name, err)
	}
	var listed struct {
		Groups []struct {
			ID   string `json:"id"`
			Name string `json:"name"`
		} `json:"groups"`
	}
	if err := json.Unmarshal(raw, &listed); err != nil {
		return fmt.Errorf("group Orca projects in %s: Orca returned an unreadable response", name)
	}
	groupID := ""
	for _, group := range listed.Groups {
		if group.Name == name {
			groupID = group.ID
			break
		}
	}
	if groupID == "" {
		raw, err = c.call(ctx, "projectGroup.create", map[string]any{
			"name":        name,
			"createdFrom": "manual",
		})
		if err != nil {
			return fmt.Errorf("group Orca projects in %s: %w", name, err)
		}
		var created struct {
			ID    string `json:"id"`
			Group struct {
				ID string `json:"id"`
			} `json:"group"`
		}
		if err := json.Unmarshal(raw, &created); err != nil {
			return fmt.Errorf("group Orca projects in %s: Orca returned an unreadable response", name)
		}
		groupID = created.Group.ID
		if groupID == "" {
			groupID = created.ID
		}
		if groupID == "" {
			return fmt.Errorf("group Orca projects in %s: Orca did not return a folder", name)
		}
	}
	if _, err := c.call(ctx, "projectGroup.moveProject", map[string]any{
		"repo":    repoID,
		"groupId": groupID,
	}); err != nil {
		return fmt.Errorf("group Orca projects in %s: %w", name, err)
	}
	return nil
}

func (c *Client) call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	meta, err := readRuntimeMetadata()
	if err != nil {
		return nil, err
	}
	endpoint := ""
	for _, transport := range meta.Transports {
		if transport.Kind == "unix" || transport.Kind == "named-pipe" {
			endpoint = transport.Endpoint
			break
		}
	}
	if endpoint == "" && (meta.Transport.Kind == "unix" || meta.Transport.Kind == "named-pipe") {
		endpoint = meta.Transport.Endpoint
	}
	if endpoint == "" || meta.AuthToken == "" {
		return nil, fmt.Errorf("Orca is not running. Open the Orca app and try again")
	}
	var id [16]byte
	if _, err := rand.Read(id[:]); err != nil {
		return nil, err
	}
	body := map[string]any{
		"id":        hex.EncodeToString(id[:]),
		"authToken": meta.AuthToken,
		"method":    method,
	}
	if params != nil {
		body["params"] = params
	}
	payload, err := json.Marshal(body)
	if err != nil {
		return nil, err
	}
	dialer := net.Dialer{}
	conn, err := dialer.DialContext(ctx, "unix", endpoint)
	if err != nil {
		return nil, fmt.Errorf("Orca is not running. Open the Orca app and try again")
	}
	defer conn.Close()
	deadline := time.Now().Add(30 * time.Second)
	if until, ok := ctx.Deadline(); ok && until.Before(deadline) {
		deadline = until
	}
	_ = conn.SetDeadline(deadline)
	if _, err := conn.Write(append(payload, '\n')); err != nil {
		return nil, err
	}
	reader := bufio.NewReader(conn)
	for {
		line, err := reader.ReadBytes('\n')
		if err != nil {
			return nil, err
		}
		var frame struct {
			Keepalive bool `json:"_keepalive"`
			OK        bool `json:"ok"`
			Error     struct {
				Message string `json:"message"`
			} `json:"error"`
			Result json.RawMessage `json:"result"`
		}
		if err := json.Unmarshal(line, &frame); err != nil {
			return nil, fmt.Errorf("Orca returned an unreadable response")
		}
		if frame.Keepalive {
			continue
		}
		if !frame.OK {
			message := frame.Error.Message
			if message == "" {
				message = "Orca rejected the request"
			}
			return nil, fmt.Errorf("%s", message)
		}
		if len(frame.Result) == 0 {
			frame.Result = []byte("{}")
		}
		return frame.Result, nil
	}
}

type runtimeMetadata struct {
	AuthToken  string `json:"authToken"`
	Transports []struct {
		Kind     string `json:"kind"`
		Endpoint string `json:"endpoint"`
	} `json:"transports"`
	Transport struct {
		Kind     string `json:"kind"`
		Endpoint string `json:"endpoint"`
	} `json:"transport"`
}

func readRuntimeMetadata() (runtimeMetadata, error) {
	var meta runtimeMetadata
	path, err := runtimeMetadataPath()
	if err != nil {
		return meta, err
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		return meta, fmt.Errorf("Orca is not running. Open the Orca app and try again")
	}
	if err := json.Unmarshal(raw, &meta); err != nil {
		return meta, fmt.Errorf("Orca is not running. Open the Orca app and try again")
	}
	return meta, nil
}

func runtimeMetadataPath() (string, error) {
	if dir := os.Getenv("ORCA_USER_DATA_PATH"); dir != "" {
		return filepath.Join(dir, "orca-runtime.json"), nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("Orca is not running. Open the Orca app and try again")
	}
	switch runtime.GOOS {
	case "darwin":
		return filepath.Join(home, "Library", "Application Support", "orca", "orca-runtime.json"), nil
	case "windows":
		appData := os.Getenv("APPDATA")
		if appData == "" {
			return "", fmt.Errorf("Orca is not running. Open the Orca app and try again")
		}
		return filepath.Join(appData, "orca", "orca-runtime.json"), nil
	default:
		base := os.Getenv("XDG_CONFIG_HOME")
		if base == "" {
			base = filepath.Join(home, ".config")
		}
		return filepath.Join(base, "orca", "orca-runtime.json"), nil
	}
}
