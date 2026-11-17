package store

import (
	"database/sql"
	"errors"
	"strings"
)

// ErrGroupNotFound is returned when a group id is not in the database.
var ErrGroupNotFound = errors.New("group was not found")

// Group is a board group. Creating one creates its Jira epic.
type Group struct {
	ID            string `json:"id"`
	Name          string `json:"name"`
	Description   string `json:"description"`
	EpicKey       string `json:"epicKey"`
	EpicBoardID   string `json:"epicBoardId"`
	EpicBoardName string `json:"epicBoardName"`
	CreatedAt     string `json:"createdAt"`
	Deleted       bool   `json:"deleted"`
}

// GroupInput is the stored part of a group. The epic is already created.
type GroupInput struct {
	Name          string
	Description   string
	EpicKey       string
	EpicBoardID   string
	EpicBoardName string
}

func (s *Store) initGroups() error {
	_, err := s.db.Exec(`CREATE TABLE IF NOT EXISTS groups (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL,
			description TEXT NOT NULL,
			epic_key TEXT NOT NULL,
			epic_board_id TEXT NOT NULL,
			epic_board_name TEXT NOT NULL,
			created_at TEXT NOT NULL,
			deleted_at TEXT NOT NULL DEFAULT ''
		)`)
	if err != nil {
		return err
	}
	return s.ensureGroupDeleted()
}

func (s *Store) ensureGroupDeleted() error {
	rows, err := s.db.Query(`PRAGMA table_info(groups)`)
	if err != nil {
		return err
	}
	defer rows.Close()
	for rows.Next() {
		var cid, notnull, pk int
		var name, kind string
		var fallback sql.NullString
		if err := rows.Scan(&cid, &name, &kind, &notnull, &fallback, &pk); err != nil {
			return err
		}
		if name == "deleted_at" {
			return rows.Err()
		}
	}
	if err := rows.Err(); err != nil {
		return err
	}
	_, err = s.db.Exec(`ALTER TABLE groups ADD COLUMN deleted_at TEXT NOT NULL DEFAULT ''`)
	return err
}

// Groups returns every group, newest first.
func (s *Store) Groups() ([]Group, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.groupsLocked()
}

// Group returns one group.
func (s *Store) Group(id string) (Group, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.groupLocked(id)
}

// CreateGroup stores a group that already has a Jira epic.
func (s *Store) CreateGroup(in GroupInput) (Group, error) {
	in, err := normalizeGroup(in)
	if err != nil {
		return Group{}, err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.createGroupLocked(in)
}

// DeleteGroup marks a group deleted. Tasks that already use it keep that link.
func (s *Store) DeleteGroup(id string) error {
	id = strings.TrimSpace(id)
	if id == "" {
		return ErrGroupNotFound
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	res, err := s.db.Exec(`UPDATE groups SET deleted_at = ? WHERE id = ? AND deleted_at = ''`, now(), id)
	if err != nil {
		return err
	}
	n, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if n > 0 {
		return nil
	}
	group, err := s.groupLocked(id)
	if err != nil {
		return err
	}
	if group.Deleted {
		return nil
	}
	return ErrGroupNotFound
}

func (s *Store) groupsLocked() ([]Group, error) {
	rows, err := s.db.Query(`SELECT id, name, description, epic_key, epic_board_id, epic_board_name, created_at, deleted_at FROM groups ORDER BY created_at DESC, id DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Group{}
	for rows.Next() {
		group, err := scanGroup(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, group)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return out, nil
}

func (s *Store) groupLocked(id string) (Group, error) {
	row := s.db.QueryRow(`SELECT id, name, description, epic_key, epic_board_id, epic_board_name, created_at, deleted_at FROM groups WHERE id = ?`, strings.TrimSpace(id))
	group, err := scanGroup(row)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return Group{}, ErrGroupNotFound
		}
		return Group{}, err
	}
	return group, nil
}

func (s *Store) createGroupLocked(in GroupInput) (Group, error) {
	id, err := newTaskID()
	if err != nil {
		return Group{}, err
	}
	created := now()
	if _, err := s.db.Exec(`INSERT INTO groups (id, name, description, epic_key, epic_board_id, epic_board_name, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)`,
		id, in.Name, in.Description, in.EpicKey, in.EpicBoardID, in.EpicBoardName, created); err != nil {
		return Group{}, err
	}
	return s.groupLocked(id)
}

func scanGroup(row taskScanner) (Group, error) {
	var group Group
	var deletedAt string
	err := row.Scan(&group.ID, &group.Name, &group.Description, &group.EpicKey, &group.EpicBoardID, &group.EpicBoardName, &group.CreatedAt, &deletedAt)
	group.Deleted = strings.TrimSpace(deletedAt) != ""
	return group, err
}

func normalizeGroup(in GroupInput) (GroupInput, error) {
	in.Name = strings.TrimSpace(in.Name)
	in.Description = strings.TrimSpace(in.Description)
	in.EpicKey = strings.ToUpper(strings.TrimSpace(in.EpicKey))
	in.EpicBoardID = strings.TrimSpace(in.EpicBoardID)
	in.EpicBoardName = strings.TrimSpace(in.EpicBoardName)
	if in.Name == "" {
		return GroupInput{}, TaskError{Message: "Enter a group name."}
	}
	if len(in.Name) > 200 {
		return GroupInput{}, TaskError{Message: "Group name is longer than 200 characters."}
	}
	if len(in.Description) > 4000 {
		return GroupInput{}, TaskError{Message: "Description is longer than 4000 characters."}
	}
	if in.EpicKey == "" {
		in.EpicBoardID = ""
		in.EpicBoardName = ""
		return in, nil
	}
	if !epicKeyPattern.MatchString(in.EpicKey) {
		return GroupInput{}, TaskError{Message: "The Jira epic key is not valid."}
	}
	if in.EpicBoardID == "" {
		return GroupInput{}, TaskError{Message: "The Jira epic needs a board."}
	}
	return in, nil
}

// migrateTaskGroups turns tasks that already have a Jira epic into groups,
// then points those tasks at the group.
func (s *Store) migrateTaskGroups() error {
	rows, err := s.db.Query(`SELECT DISTINCT epic_key FROM tasks WHERE group_id = '' AND epic_key != ''`)
	if err != nil {
		return err
	}
	defer rows.Close()
	var keys []string
	for rows.Next() {
		var key string
		if err := rows.Scan(&key); err != nil {
			return err
		}
		keys = append(keys, strings.TrimSpace(key))
	}
	if err := rows.Err(); err != nil {
		return err
	}
	for _, key := range keys {
		if !epicKeyPattern.MatchString(strings.ToUpper(key)) {
			continue
		}
		var groupID string
		err := s.db.QueryRow(`SELECT id FROM groups WHERE epic_key = ?`, strings.ToUpper(key)).Scan(&groupID)
		if errors.Is(err, sql.ErrNoRows) {
			var name, description, boardID, boardName string
			err = s.db.QueryRow(`SELECT name, description, epic_board_id, epic_board_name FROM tasks WHERE epic_key = ? AND epic_owner = 1 ORDER BY created_at LIMIT 1`, key).Scan(&name, &description, &boardID, &boardName)
			if errors.Is(err, sql.ErrNoRows) {
				err = s.db.QueryRow(`SELECT name, description, epic_board_id, epic_board_name FROM tasks WHERE epic_key = ? ORDER BY created_at LIMIT 1`, key).Scan(&name, &description, &boardID, &boardName)
			}
			if err != nil {
				return err
			}
			if strings.TrimSpace(boardID) == "" {
				continue
			}
			if strings.TrimSpace(name) == "" {
				name = strings.ToUpper(key)
			}
			group, err := s.createGroupLocked(GroupInput{
				Name:          name,
				Description:   description,
				EpicKey:       key,
				EpicBoardID:   boardID,
				EpicBoardName: boardName,
			})
			if err != nil {
				return err
			}
			groupID = group.ID
		} else if err != nil {
			return err
		}
		if _, err := s.db.Exec(`UPDATE tasks SET group_id = ?, parent_id = '', epic_owner = 0 WHERE epic_key = ? AND group_id = ''`, groupID, key); err != nil {
			return err
		}
	}
	return nil
}
