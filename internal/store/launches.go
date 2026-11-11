package store

import (
	"errors"
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"

	_ "modernc.org/sqlite"
)

// ErrNotFound is returned when a run id is not in the log.
var ErrNotFound = errors.New("run was not found")

func sqliteDSN(path string) string {
	cleaned := filepath.ToSlash(path)
	var b strings.Builder
	for i, part := range strings.Split(cleaned, "/") {
		if i > 0 {
			b.WriteByte('/')
		}
		b.WriteString(url.PathEscape(part))
	}
	return "file:" + b.String() + "?_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)"
}

func (s *Store) initLogs() error {
	_, err := s.db.Exec(`CREATE TABLE IF NOT EXISTS launches (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		agent_id TEXT NOT NULL,
		runtime TEXT NOT NULL,
		note TEXT NOT NULL,
		status TEXT NOT NULL,
		detail TEXT NOT NULL,
		created_at TEXT NOT NULL
	)`)
	return err
}

// migrateLaunches copies runs still stored in kiki.yaml into SQLite, then
// removes that list from the setup file. Agents read the setup file and do
// not need the run log.
func (s *Store) migrateLaunches() error {
	raw, err := os.ReadFile(s.path)
	if err != nil {
		return err
	}
	var doc struct {
		Launches []Launch `yaml:"launches"`
	}
	if err := yaml.Unmarshal(raw, &doc); err != nil {
		return fmt.Errorf("read setup file: %w", err)
	}
	if len(doc.Launches) == 0 {
		return nil
	}
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	stmt, err := tx.Prepare(`INSERT OR IGNORE INTO launches
		(id, agent_id, runtime, note, status, detail, created_at)
		VALUES (?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		_ = tx.Rollback()
		return err
	}
	defer stmt.Close()
	for _, item := range doc.Launches {
		if item.ID <= 0 {
			continue
		}
		if _, err := stmt.Exec(item.ID, item.AgentID, item.Runtime, item.Note, item.Status, item.Detail, item.CreatedAt); err != nil {
			_ = tx.Rollback()
			return err
		}
	}
	if err := tx.Commit(); err != nil {
		return err
	}
	setup, err := s.read()
	if err != nil {
		return err
	}
	return s.write(setup)
}

// AddLaunch stores the result of an agent start.
func (s *Store) AddLaunch(agentID, runtime, note, status, detail string) (Launch, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	item := Launch{
		AgentID:   agentID,
		Runtime:   runtime,
		Note:      note,
		Status:    status,
		Detail:    detail,
		CreatedAt: now(),
	}
	res, err := s.db.Exec(`INSERT INTO launches (agent_id, runtime, note, status, detail, created_at) VALUES (?, ?, ?, ?, ?, ?)`,
		item.AgentID, item.Runtime, item.Note, item.Status, item.Detail, item.CreatedAt)
	if err != nil {
		return Launch{}, err
	}
	id, err := res.LastInsertId()
	if err != nil {
		return Launch{}, err
	}
	item.ID = id
	return item, nil
}

// Launches returns the newest launches.
func (s *Store) Launches(limit int) ([]Launch, error) {
	if limit <= 0 {
		limit = 15
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	rows, err := s.db.Query(`SELECT id, agent_id, runtime, note, status, detail, created_at
		FROM launches ORDER BY id DESC LIMIT ?`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]Launch, 0, limit)
	for rows.Next() {
		var item Launch
		if err := rows.Scan(&item.ID, &item.AgentID, &item.Runtime, &item.Note, &item.Status, &item.Detail, &item.CreatedAt); err != nil {
			return nil, err
		}
		out = append(out, item)
	}
	return out, rows.Err()
}

// DeleteLaunch removes one run.
func (s *Store) DeleteLaunch(id int64) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	res, err := s.db.Exec(`DELETE FROM launches WHERE id = ?`, id)
	if err != nil {
		return err
	}
	n, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if n == 0 {
		return ErrNotFound
	}
	return nil
}

// ClearLaunches removes every run.
func (s *Store) ClearLaunches() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	_, err := s.db.Exec(`DELETE FROM launches`)
	return err
}
