package store

import (
	"fmt"
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"

	"kiki/internal/mcp"
)

// mcpDoc is every MCP saved for this computer. New MCP sections go in this file.
type mcpDoc struct {
	Atlassian mcp.Atlassian `yaml:"atlassian,omitempty"`
}

func (s *Store) mcpPath() string {
	return filepath.Join(filepath.Dir(s.path), "mcp.yaml")
}

func (s *Store) legacyAtlassianPath() string {
	return filepath.Join(filepath.Dir(s.path), "atlassian.yaml")
}

// Atlassian reads the Jira and Confluence section of mcp.yaml.
// A missing file is an empty setup. An older atlassian.yaml is moved into mcp.yaml.
func (s *Store) Atlassian() (mcp.Atlassian, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.readAtlassian()
}

func (s *Store) readAtlassian() (mcp.Atlassian, error) {
	raw, err := os.ReadFile(s.mcpPath())
	if err != nil && !os.IsNotExist(err) {
		return mcp.Atlassian{}, err
	}
	if os.IsNotExist(err) {
		return s.migrateAtlassianFile()
	}
	var doc mcpDoc
	if err := yaml.Unmarshal(raw, &doc); err != nil {
		return mcp.Atlassian{}, fmt.Errorf("read MCP settings: %w", err)
	}
	if err := s.removeLegacyAtlassian(); err != nil {
		return mcp.Atlassian{}, err
	}
	return doc.Atlassian.Normalized(), nil
}

// migrateAtlassianFile copies a root-level atlassian.yaml into mcp.yaml and removes the old file.
func (s *Store) migrateAtlassianFile() (mcp.Atlassian, error) {
	raw, err := os.ReadFile(s.legacyAtlassianPath())
	if err != nil {
		if os.IsNotExist(err) {
			return mcp.Atlassian{}, nil
		}
		return mcp.Atlassian{}, err
	}
	var cfg mcp.Atlassian
	if err := yaml.Unmarshal(raw, &cfg); err != nil {
		return mcp.Atlassian{}, fmt.Errorf("read Atlassian MCP settings: %w", err)
	}
	cfg = cfg.Normalized()
	if cfg.Empty() {
		return mcp.Atlassian{}, s.removeLegacyAtlassian()
	}
	if err := s.writeMCP(cfg); err != nil {
		return mcp.Atlassian{}, err
	}
	if err := s.removeLegacyAtlassian(); err != nil {
		return mcp.Atlassian{}, err
	}
	return cfg, nil
}

// SaveAtlassian stores Jira and Confluence inside mcp.yaml.
// An empty setup removes the file. Agents read the setup file, not this one.
func (s *Store) SaveAtlassian(cfg mcp.Atlassian) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	cfg = cfg.Normalized()
	if err := s.writeMCP(cfg); err != nil {
		return err
	}
	return s.removeLegacyAtlassian()
}

func (s *Store) writeMCP(cfg mcp.Atlassian) error {
	path := s.mcpPath()
	if cfg.Empty() {
		err := os.Remove(path)
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if err := cfg.Validate(); err != nil {
		return err
	}
	raw, err := yaml.Marshal(mcpDoc{Atlassian: cfg})
	if err != nil {
		return err
	}
	text := "# MCP credentials for this computer. Agents do not read this file.\n" + string(raw)
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, []byte(text), 0o600); err != nil {
		return err
	}
	if err := os.Chmod(tmp, 0o600); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return os.Chmod(path, 0o600)
}

func (s *Store) removeLegacyAtlassian() error {
	for _, path := range []string{s.legacyAtlassianPath(), s.legacyAtlassianPath() + ".tmp"} {
		err := os.Remove(path)
		if err != nil && !os.IsNotExist(err) {
			return err
		}
	}
	return nil
}
