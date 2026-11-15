package store

import (
	"fmt"
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"

	"kiki/internal/tools"
)

func (s *Store) toolsPath() string {
	return filepath.Join(filepath.Dir(s.path), "tools.yaml")
}

// Tools reads the tools turned on for this computer.
// A missing file means none are turned on.
func (s *Store) Tools() (tools.Settings, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.readTools()
}

func (s *Store) readTools() (tools.Settings, error) {
	raw, err := os.ReadFile(s.toolsPath())
	if err != nil {
		if os.IsNotExist(err) {
			return tools.Settings{}, nil
		}
		return tools.Settings{}, err
	}
	var cfg tools.Settings
	if err := yaml.Unmarshal(raw, &cfg); err != nil {
		return tools.Settings{}, fmt.Errorf("read tool settings: %w", err)
	}
	return cfg, nil
}

// SaveTools stores which tools are turned on.
// Turning every tool off removes the file. Agents read the setup file, not this one.
func (s *Store) SaveTools(cfg tools.Settings) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.writeTools(cfg)
}

func (s *Store) writeTools(cfg tools.Settings) error {
	path := s.toolsPath()
	if cfg.Empty() {
		err := os.Remove(path)
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	raw, err := yaml.Marshal(cfg)
	if err != nil {
		return err
	}
	text := "# Tools turned on for this computer. Agents do not read this file.\n" + string(raw)
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, []byte(text), 0o644); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return nil
}
