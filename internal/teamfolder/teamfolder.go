// Package teamfolder reads a local team folder.
// Each immediate subfolder is one member. That member's agents live in an
// agents folder inside it. The user syncs the team folder with their own tool.
package teamfolder

import (
	"errors"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
)

const (
	maxFile  = 8 << 20
	maxTotal = 32 << 20
	maxDepth = 10
)

type fileBytes struct {
	rel  string
	body []byte
}

// Dir checks that raw is an existing absolute directory.
func Dir(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", errors.New("Choose a team folder first.")
	}
	if !filepath.IsAbs(raw) {
		return "", errors.New("Enter the full path of the team folder.")
	}
	dir := filepath.Clean(raw)
	info, err := os.Stat(dir)
	if err != nil || !info.IsDir() {
		return "", errors.New("That team folder is not available.")
	}
	return dir, nil
}

// Members lists the member folders directly inside the team folder.
// Hidden folders and symlinks are skipped.
func Members(raw string) ([]string, error) {
	dir, err := Dir(raw)
	if err != nil {
		return nil, err
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, errors.New("That team folder is not available.")
	}
	names := []string{}
	for _, entry := range entries {
		name := entry.Name()
		if !safeName(name) || strings.HasPrefix(name, ".") {
			continue
		}
		info, err := entry.Info()
		if err != nil || info.Mode()&os.ModeSymlink != 0 || !entry.IsDir() {
			continue
		}
		names = append(names, name)
	}
	sort.Strings(names)
	return names, nil
}

// ValidName reports whether name can be one member folder.
func ValidName(name string) bool {
	name = strings.TrimSpace(name)
	return safeName(name) && !strings.HasPrefix(name, ".")
}

// UserDir returns the member folder for name, creating nothing.
// The folder may be missing. An existing file or symlink is rejected.
func UserDir(team, name string) (string, error) {
	dir, err := Dir(team)
	if err != nil {
		return "", err
	}
	name = strings.TrimSpace(name)
	if !ValidName(name) {
		return "", errors.New("Enter your name.")
	}
	child := filepath.Join(dir, name)
	rel, err := filepath.Rel(dir, child)
	if err != nil || rel != name {
		return "", errors.New("Enter your name.")
	}
	info, err := os.Lstat(child)
	if err != nil {
		if os.IsNotExist(err) {
			return child, nil
		}
		return "", err
	}
	if !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return "", errors.New("That name is not a folder in the team folder.")
	}
	return child, nil
}

// AgentsDir is the agents folder inside a member folder.
func AgentsDir(memberDir string) string {
	return filepath.Join(filepath.Clean(memberDir), "agents")
}

// HasAgents reports whether memberDir contains a real agents directory.
func HasAgents(memberDir string) bool {
	info, err := os.Lstat(AgentsDir(memberDir))
	return err == nil && info.IsDir() && info.Mode()&os.ModeSymlink == 0
}

// Member returns the directory for one member inside the team folder.
func Member(team, name string) (string, error) {
	dir, err := Dir(team)
	if err != nil {
		return "", err
	}
	name = strings.TrimSpace(name)
	if !safeName(name) || strings.HasPrefix(name, ".") {
		return "", errors.New("That folder is not in the team folder.")
	}
	child := filepath.Join(dir, name)
	rel, err := filepath.Rel(dir, child)
	if err != nil || rel != name {
		return "", errors.New("That folder is not in the team folder.")
	}
	info, err := os.Lstat(child)
	if err != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return "", errors.New("That folder is not in the team folder.")
	}
	return child, nil
}

// Copy replaces agentDir with the files in memberDir.
// agentDir is left unchanged when the copy cannot be completed.
func Copy(memberDir, agentDir string) error {
	memberDir = filepath.Clean(strings.TrimSpace(memberDir))
	agentDir = filepath.Clean(strings.TrimSpace(agentDir))
	if memberDir == "" || agentDir == "" || !filepath.IsAbs(memberDir) || !filepath.IsAbs(agentDir) {
		return errors.New("The agent folder is not available.")
	}
	if overlaps(memberDir, agentDir) {
		return errors.New("The team folder cannot be the local agents folder.")
	}
	files, err := readTree(memberDir)
	if err != nil {
		return err
	}
	if len(files) == 0 {
		return errors.New("That member folder has no agent files to copy.")
	}
	return install(agentDir, files)
}

func safeName(name string) bool {
	return name != "" && name != "." && name != ".." && !strings.ContainsAny(name, "/\\\r\n\x00")
}

func overlaps(a, b string) bool {
	return within(a, b) || within(b, a)
}

func within(root, target string) bool {
	rel, err := filepath.Rel(root, target)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

func readTree(dir string) ([]fileBytes, error) {
	info, err := os.Stat(dir)
	if err != nil || !info.IsDir() {
		return nil, errors.New("That member folder is not available.")
	}
	var files []fileBytes
	var total int64
	err = filepath.WalkDir(dir, func(current string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		name := entry.Name()
		if current != dir && strings.HasPrefix(name, ".") {
			if entry.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		if entry.Type()&os.ModeSymlink != 0 {
			if entry.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		if entry.IsDir() {
			return nil
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if !info.Mode().IsRegular() {
			return nil
		}
		if info.Size() > maxFile {
			return fmt.Errorf("agent file %s is larger than 8MB", name)
		}
		rel, err := filepath.Rel(dir, current)
		if err != nil {
			return err
		}
		rel = filepath.ToSlash(rel)
		if strings.Count(rel, "/") > maxDepth {
			return errors.New("That member folder is too deep.")
		}
		rel, err = safeRel(rel)
		if err != nil {
			return err
		}
		body, err := os.ReadFile(current)
		if err != nil {
			return err
		}
		total += int64(len(body))
		if total > maxTotal {
			return errors.New("The agent folder is larger than 32MB.")
		}
		files = append(files, fileBytes{rel: rel, body: body})
		return nil
	})
	if err != nil {
		return nil, err
	}
	return files, nil
}

func install(dir string, files []fileBytes) error {
	if dir == "" || !filepath.IsAbs(dir) {
		return errors.New("The agent folder is not available.")
	}
	planned := make([]fileBytes, len(files))
	for i, file := range files {
		rel, err := safeRel(file.rel)
		if err != nil {
			return err
		}
		planned[i] = fileBytes{rel: rel, body: file.body}
	}
	info, err := os.Stat(dir)
	if err != nil {
		if !os.IsNotExist(err) {
			return err
		}
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	} else if !info.IsDir() {
		return errors.New("The agent folder is not available.")
	}
	tmp, err := os.MkdirTemp(filepath.Dir(dir), ".kiki-agents-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmp)
	for _, file := range planned {
		dest := filepath.Join(tmp, filepath.FromSlash(file.rel))
		if !inside(tmp, dest) {
			return fmt.Errorf("agent file path %q is not valid", file.rel)
		}
		if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
			return err
		}
		if err := os.WriteFile(dest, file.body, 0o644); err != nil {
			return err
		}
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if err := os.RemoveAll(filepath.Join(dir, entry.Name())); err != nil {
			return err
		}
	}
	written, err := os.ReadDir(tmp)
	if err != nil {
		return err
	}
	for _, entry := range written {
		from := filepath.Join(tmp, entry.Name())
		to := filepath.Join(dir, entry.Name())
		if err := os.Rename(from, to); err != nil {
			return err
		}
	}
	return nil
}

func inside(root, target string) bool {
	rel, err := filepath.Rel(root, target)
	return err == nil && rel != "." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)) && rel != ".."
}

func safeRel(rel string) (string, error) {
	rel = strings.TrimSpace(rel)
	rel = strings.ReplaceAll(rel, "\\", "/")
	if rel == "" || strings.HasPrefix(rel, "/") || strings.Contains(rel, "\x00") {
		return "", fmt.Errorf("agent file path %q is not valid", rel)
	}
	clean := path.Clean(rel)
	if clean == "." || clean == ".." || strings.HasPrefix(clean, "../") {
		return "", fmt.Errorf("agent file path %q is not valid", rel)
	}
	return clean, nil
}
