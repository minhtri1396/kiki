package store

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

// Migrate moves files an older KIKI wrote beside the app into the .kiki folder.
// Repository folders stay where they are. Agent files always end up in .kiki/agents.
func Migrate(root string) error {
	root, err := filepath.Abs(strings.TrimSpace(root))
	if err != nil {
		return err
	}
	root = filepath.Clean(root)
	destDir := DataDir(root)
	if _, err := os.Stat(filepath.Join(destDir, "kiki.yaml")); err == nil {
		if err := pinAgents(root); err != nil {
			return err
		}
		return dropModelsFiles(root)
	} else if !os.IsNotExist(err) {
		return err
	}
	legacySetup := filepath.Join(root, "kiki.yaml")
	if _, err := os.Stat(legacySetup); err != nil {
		if os.IsNotExist(err) {
			return dropModelsFiles(root)
		}
		return err
	}
	agentPath, err := legacyAgentPath(legacySetup)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return err
	}
	oldAgents := filepath.Join(root, "agents")
	newAgents := filepath.Join(destDir, "agents")
	if agentPath == "" || filepath.Clean(agentPath) == oldAgents {
		if err := movePath(oldAgents, newAgents); err != nil {
			return err
		}
	}
	for _, name := range []string{
		"kiki.db", "kiki.db-shm", "kiki.db-wal", "kiki.db-journal",
	} {
		if err := movePath(filepath.Join(root, name), filepath.Join(destDir, name)); err != nil {
			return err
		}
	}
	if err := movePath(legacySetup, filepath.Join(destDir, "kiki.yaml")); err != nil {
		return err
	}
	if err := pinAgents(root); err != nil {
		return err
	}
	return dropModelsFiles(root)
}

// dropModelsFiles removes the old per-agent model list. Tool and model live in each agent file.
func dropModelsFiles(root string) error {
	for _, path := range []string{
		filepath.Join(root, "models.yaml"),
		filepath.Join(root, "models.yaml.tmp"),
		filepath.Join(DataDir(root), "models.yaml"),
		filepath.Join(DataDir(root), "models.yaml.tmp"),
	} {
		if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
			return err
		}
	}
	return nil
}

func legacyAgentPath(path string) (string, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	var doc fileDoc
	if err := yaml.Unmarshal(raw, &doc); err != nil {
		return "", fmt.Errorf("read setup file: %w", err)
	}
	return strings.TrimSpace(doc.AgentPath), nil
}

func pinAgents(root string) error {
	setup := Path(root)
	agentPath, err := legacyAgentPath(setup)
	if err != nil {
		return err
	}
	fixed := filepath.Join(DataDir(root), "agents")
	if err := adoptAgents(agentPath, fixed); err != nil {
		return err
	}
	if strings.TrimSpace(agentPath) == "" {
		return nil
	}
	return setAgentPath(setup, "")
}

func adoptAgents(from, to string) error {
	from = filepath.Clean(strings.TrimSpace(from))
	to = filepath.Clean(to)
	if from == "" || from == to {
		return nil
	}
	info, err := os.Stat(from)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if !info.IsDir() {
		return nil
	}
	occupied, err := dirHasContent(to)
	if err != nil || occupied {
		return err
	}
	if err := os.RemoveAll(to); err != nil && !os.IsNotExist(err) {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(to), 0o755); err != nil {
		return err
	}
	return os.Rename(from, to)
}

func dirHasContent(dir string) (bool, error) {
	info, err := os.Stat(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return false, nil
		}
		return false, err
	}
	if !info.IsDir() {
		return true, nil
	}
	found := false
	err = filepath.WalkDir(dir, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if path == dir || strings.HasPrefix(entry.Name(), ".") {
			if entry.IsDir() && path != dir {
				return filepath.SkipDir
			}
			return nil
		}
		found = true
		return filepath.SkipAll
	})
	if err != nil && err != filepath.SkipAll {
		return false, err
	}
	return found, nil
}

func setAgentPath(path, agentPath string) error {
	s := &Store{path: path}
	doc, err := s.read()
	if err != nil {
		return err
	}
	doc.AgentPath = agentPath
	return s.write(doc)
}

func movePath(from, to string) error {
	if _, err := os.Stat(from); err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if _, err := os.Stat(to); err == nil {
		return fmt.Errorf("%s already exists", to)
	} else if !os.IsNotExist(err) {
		return err
	}
	return os.Rename(from, to)
}
