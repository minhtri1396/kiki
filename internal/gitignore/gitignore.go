// Package gitignore keeps Git from tracking files KIKI creates.
package gitignore

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"kiki/internal/toolchain"
)

const headerComment = "# KIKI generated files"
const indexHeader = "# KIKI index"

// Entry is a file or directory the app writes.
type Entry struct {
	Path string
	Dir  bool
}

// AppEntries lists the files and folders KIKI writes for this app folder.
// agentDir is included when it is set. Repo folders are always included.
func AppEntries(root, agentDir, setupPath, logPath string) []Entry {
	entries := []Entry{
		{Path: setupPath},
		{Path: setupPath + ".tmp"},
		{Path: logPath},
		{Path: logPath + "-shm"},
		{Path: logPath + "-wal"},
		{Path: logPath + "-journal"},
	}
	for _, kind := range toolchain.Kinds {
		entries = append(entries, Entry{Path: toolchain.Folder(root, kind), Dir: true})
	}
	if strings.TrimSpace(agentDir) != "" {
		entries = append(entries, Entry{Path: agentDir, Dir: true})
	}
	return entries
}

// Ensure adds ignore rules for entries when dir is inside a Git work tree.
// The returned path is the .gitignore file that was written. It is empty when
// nothing changed, including when dir is not inside a work tree.
// Paths outside that work tree are left alone. Rules already present are kept.
func Ensure(dir string, entries []Entry) (string, error) {
	return ensure(dir, entries, headerComment)
}

// Index ignores CodeGraph and Obsidian directories in the repository at dir.
// names are ".codegraph" and ".obsidian". A folder that is not inside a Git work tree is left unchanged.
func Index(dir string, names ...string) (string, error) {
	dir = strings.TrimSpace(dir)
	if dir == "" || len(names) == 0 {
		return "", fmt.Errorf("index path is invalid")
	}
	entries := make([]Entry, 0, len(names))
	for _, name := range names {
		name = strings.Trim(strings.TrimSpace(name), `/\`)
		if name == "" || strings.ContainsAny(name, `/\`) {
			return "", fmt.Errorf("index path is invalid")
		}
		entries = append(entries, Entry{Path: filepath.Join(dir, name), Dir: true})
	}
	return ensure(dir, entries, indexHeader)
}

func ensure(dir string, entries []Entry, header string) (string, error) {
	root, ok, err := findWorkTree(dir)
	if err != nil || !ok {
		return "", err
	}
	wants := make([]want, 0, len(entries))
	for _, entry := range entries {
		item, ok, err := wantFor(root, entry)
		if err != nil {
			return "", err
		}
		if ok {
			wants = append(wants, item)
		}
	}
	if len(wants) == 0 {
		return "", nil
	}
	path := filepath.Join(root, ".gitignore")
	raw, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return "", err
	}
	text := string(raw)
	missing := missingPatterns(text, wants)
	if len(missing) == 0 {
		return "", nil
	}
	next := appendPatterns(text, header, missing)
	if err := writeFile(path, next); err != nil {
		return "", err
	}
	return path, nil
}

type want struct {
	pattern string
	rel     string
	dir     bool
}

func findWorkTree(dir string) (string, bool, error) {
	dir, err := filepath.Abs(dir)
	if err != nil {
		return "", false, err
	}
	dir = filepath.Clean(dir)
	for {
		_, err := os.Lstat(filepath.Join(dir, ".git"))
		if err == nil {
			return dir, true, nil
		}
		if !os.IsNotExist(err) {
			return "", false, err
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", false, nil
		}
		dir = parent
	}
}

func wantFor(root string, entry Entry) (want, bool, error) {
	path := strings.TrimSpace(entry.Path)
	if path == "" {
		return want{}, false, nil
	}
	abs, err := filepath.Abs(path)
	if err != nil {
		return want{}, false, err
	}
	abs = filepath.Clean(abs)
	rel, err := filepath.Rel(root, abs)
	if err != nil {
		return want{}, false, err
	}
	rel = filepath.ToSlash(rel)
	if rel == "." || rel == "" || rel == ".." || strings.HasPrefix(rel, "../") {
		return want{}, false, nil
	}
	pattern := "/" + rel
	if entry.Dir {
		pattern += "/"
	}
	return want{pattern: pattern, rel: rel, dir: entry.Dir}, true, nil
}

func missingPatterns(text string, wants []want) []string {
	active := ignoreLines(text)
	out := make([]string, 0)
	seen := map[string]bool{}
	for _, item := range wants {
		if item.pattern == "" || seen[item.pattern] {
			continue
		}
		seen[item.pattern] = true
		if ignored(active, item.rel, item.dir) {
			continue
		}
		out = append(out, item.pattern)
		active = append(active, item.pattern)
	}
	return out
}

func ignoreLines(text string) []string {
	text = strings.ReplaceAll(text, "\r\n", "\n")
	text = strings.TrimSuffix(text, "\n")
	if text == "" {
		return nil
	}
	return strings.Split(text, "\n")
}

func ignored(lines []string, rel string, dir bool) bool {
	on := false
	for _, line := range lines {
		neg, pattern, ok := parsePattern(line)
		if !ok || !patternCovers(pattern, rel, dir) {
			continue
		}
		on = !neg
	}
	return on
}

func parsePattern(line string) (neg bool, pattern string, ok bool) {
	line = strings.TrimSpace(line)
	if line == "" || strings.HasPrefix(line, "#") {
		return false, "", false
	}
	if strings.HasPrefix(line, `\#`) || strings.HasPrefix(line, `\!`) {
		line = line[1:]
	}
	if strings.HasPrefix(line, "!") {
		pattern = strings.TrimSpace(strings.TrimPrefix(line, "!"))
		if pattern == "" {
			return false, "", false
		}
		return true, pattern, true
	}
	return false, line, true
}

// patternCovers reports whether a Git ignore pattern ignores rel.
// rel uses forward slashes and has no trailing slash. Globs are not treated as a match.
func patternCovers(pattern, rel string, dir bool) bool {
	pattern = strings.TrimSpace(pattern)
	if pattern == "" {
		return false
	}
	dirOnly := strings.HasSuffix(pattern, "/")
	pattern = strings.TrimSuffix(pattern, "/")
	if pattern == "" || strings.ContainsAny(pattern, "*?[") {
		return false
	}
	anchored := strings.HasPrefix(pattern, "/")
	pattern = strings.TrimPrefix(pattern, "/")
	rel = strings.Trim(rel, "/")
	if rel == "" {
		return false
	}
	if anchored || strings.Contains(pattern, "/") {
		if pattern == rel {
			return !dirOnly || dir
		}
		return strings.HasPrefix(rel, pattern+"/")
	}
	parts := strings.Split(rel, "/")
	for i, part := range parts {
		if part != pattern {
			continue
		}
		if i < len(parts)-1 {
			return true
		}
		return !dirOnly || dir
	}
	return false
}

func appendPatterns(raw, header string, patterns []string) string {
	nl := "\n"
	if strings.Contains(raw, "\r\n") {
		nl = "\r\n"
	}
	body := strings.ReplaceAll(raw, "\r\n", "\n")
	body = strings.TrimSuffix(body, "\n")
	needHeader := true
	if body != "" {
		for _, line := range strings.Split(body, "\n") {
			if strings.TrimSpace(line) == header {
				needHeader = false
				break
			}
		}
	}
	var b strings.Builder
	if body != "" {
		b.WriteString(body)
		b.WriteByte('\n')
		if needHeader {
			b.WriteByte('\n')
		}
	}
	if needHeader {
		b.WriteString(header)
		b.WriteByte('\n')
	}
	for _, pattern := range patterns {
		b.WriteString(pattern)
		b.WriteByte('\n')
	}
	text := b.String()
	if nl == "\r\n" {
		text = strings.ReplaceAll(text, "\n", "\r\n")
	}
	return text
}

func writeFile(path, text string) error {
	mode := os.FileMode(0o644)
	if info, err := os.Stat(path); err == nil {
		mode = info.Mode().Perm()
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, []byte(text), mode); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return nil
}
