package store

import (
	"crypto/rand"
	"database/sql"
	"errors"
	"regexp"
	"strings"
	"time"
)

const (
	// ColumnTodo is the TODO column.
	ColumnTodo = "todo"
	// ColumnProgress is the In Progress column.
	ColumnProgress = "progress"
	// ColumnDone is the Done column.
	ColumnDone = "done"

	// TaskPending means the agent has not started this task.
	TaskPending = "pending"
	// TaskDoing means the agent is working on this task.
	TaskDoing = "doing"
	// TaskReview means the agent is waiting for review.
	TaskReview = "review"
	// TaskCompleted means the agent finished this task.
	TaskCompleted = "completed"
)

// ErrTaskNotFound is returned when a task id is not in the database.
var ErrTaskNotFound = errors.New("task was not found")

// TaskError is a task the user can fix.
type TaskError struct {
	Message string
}

func (e TaskError) Error() string { return e.Message }

// Task is one board card.
type Task struct {
	ID            string      `json:"id"`
	Name          string      `json:"name"`
	Description   string      `json:"description"`
	Column        string      `json:"column"`
	CreatedAt     string      `json:"createdAt"`
	UpdatedAt     string      `json:"updatedAt"`
	DoneAt        string      `json:"doneAt"`
	BranchSlug    string      `json:"branchSlug"`
	BranchFor     string      `json:"branchFor"`
	Started       bool        `json:"started"`
		ParentID      string      `json:"parentId"`
		GroupID       string      `json:"groupId"`
		EpicKey       string      `json:"epicKey"`
		EpicBoardID   string      `json:"epicBoardId"`
		EpicBoardName string      `json:"epicBoardName"`
		EpicOwner     bool        `json:"epicOwner"`
	Agents        []TaskAgent `json:"agents"`
	Runs          []TaskRun   `json:"runs"`
}

// TaskAgent is one agent's status on a task.
type TaskAgent struct {
	AgentID string `json:"agentId"`
	Status  string `json:"status"`
}

// TaskInput is the editable part of a task.
type TaskInput struct {
	Name          string
	Description   string
	Column        string
		ParentID      string
		GroupID       string
		EpicKey       string
		EpicBoardID   string
		EpicBoardName string
		EpicOwner     bool
	Agents        []TaskAgent
}

const taskSelect = `id, name, description, column_name, created_at, updated_at, done_at, branch_slug, branch_for, started, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id`

var epicKeyPattern = regexp.MustCompile(`^[A-Z][A-Z0-9]{1,9}-[0-9]+$`)

func (s *Store) initTasks() error {
	if _, err := s.db.Exec(`CREATE TABLE IF NOT EXISTS tasks (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL,
			description TEXT NOT NULL,
			column_name TEXT NOT NULL,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			done_at TEXT NOT NULL,
				branch_slug TEXT NOT NULL DEFAULT '',
					branch_for TEXT NOT NULL DEFAULT '',
					started INTEGER NOT NULL DEFAULT 0,
					parent_id TEXT NOT NULL DEFAULT '',
					epic_key TEXT NOT NULL DEFAULT '',
					epic_board_id TEXT NOT NULL DEFAULT '',
					epic_board_name TEXT NOT NULL DEFAULT '',
					epic_owner INTEGER NOT NULL DEFAULT 0,
					group_id TEXT NOT NULL DEFAULT ''
				)`); err != nil {
		return err
	}
	if _, err := s.db.Exec(`CREATE TABLE IF NOT EXISTS task_agents (
			task_id TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			status TEXT NOT NULL,
			position INTEGER NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		)`); err != nil {
		return err
	}
	return s.ensureTaskColumns()
}

func (s *Store) ensureTaskColumns() error {
	rows, err := s.db.Query(`PRAGMA table_info(tasks)`)
	if err != nil {
		return err
	}
	defer rows.Close()
	have := map[string]bool{}
	for rows.Next() {
		var cid, notnull, pk int
		var name, kind string
		var fallback sql.NullString
		if err := rows.Scan(&cid, &name, &kind, &notnull, &fallback, &pk); err != nil {
			return err
		}
		have[name] = true
	}
	if err := rows.Err(); err != nil {
		return err
	}
	if !have["branch_slug"] {
		if _, err := s.db.Exec(`ALTER TABLE tasks ADD COLUMN branch_slug TEXT NOT NULL DEFAULT ''`); err != nil {
			return err
		}
	}
	if !have["branch_for"] {
		if _, err := s.db.Exec(`ALTER TABLE tasks ADD COLUMN branch_for TEXT NOT NULL DEFAULT ''`); err != nil {
			return err
		}
	}
	if !have["started"] {
		if _, err := s.db.Exec(`ALTER TABLE tasks ADD COLUMN started INTEGER NOT NULL DEFAULT 0`); err != nil {
			return err
		}
	}
	for _, column := range []struct{ name, ddl string }{
		{"parent_id", "parent_id TEXT NOT NULL DEFAULT ''"},
		{"epic_key", "epic_key TEXT NOT NULL DEFAULT ''"},
		{"epic_board_id", "epic_board_id TEXT NOT NULL DEFAULT ''"},
		{"epic_board_name", "epic_board_name TEXT NOT NULL DEFAULT ''"},
			{"epic_owner", "epic_owner INTEGER NOT NULL DEFAULT 0"},
			{"group_id", "group_id TEXT NOT NULL DEFAULT ''"},
		} {
		if have[column.name] {
			continue
		}
		if _, err := s.db.Exec(`ALTER TABLE tasks ADD COLUMN ` + column.ddl); err != nil {
			return err
		}
	}
	return nil
}

// Tasks returns every task, newest activity first.
func (s *Store) Tasks() ([]Task, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.tasksLocked()
}

// Task returns one task.
func (s *Store) Task(id string) (Task, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.taskLocked(id)
}

// CreateTask stores a new task.
func (s *Store) CreateTask(in TaskInput) (Task, error) {
	in, err := normalizeTask(in)
	if err != nil {
		return Task{}, err
	}
	in.Column = ColumnTodo
	s.mu.Lock()
	defer s.mu.Unlock()
		in, err = s.resolveTaskLinkLocked(in)
	if err != nil {
		return Task{}, err
	}
	stamp := now()
	return s.saveTask("", in, stamp, stamp, "")
}

// UpdateTask replaces the editable fields of one task.
// DoneAt stays when the task remains in Done, and is set when it enters Done.
func (s *Store) UpdateTask(id string, in TaskInput) (Task, error) {
	in, err := normalizeTask(in)
	if err != nil {
		return Task{}, err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	prev, err := s.taskLocked(id)
	if err != nil {
		return Task{}, err
	}
	if prev.Started {
		return Task{}, TaskError{Message: "This task has already started."}
	}
	in.Column = prev.Column
		in, err = s.resolveTaskLinkLocked(in)
	if err != nil {
		return Task{}, err
	}
	return s.saveTask(id, in, prev.CreatedAt, now(), prev.DoneAt)
}

// DeleteTask removes one task and its agents.
func (s *Store) DeleteTask(id string) error {
	if id == "" {
		return ErrTaskNotFound
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	if _, err := tx.Exec(`UPDATE tasks SET parent_id = '' WHERE parent_id = ?`, id); err != nil {
		_ = tx.Rollback()
		return err
	}
	if _, err := tx.Exec(`DELETE FROM task_agents WHERE task_id = ?`, id); err != nil {
		_ = tx.Rollback()
		return err
	}
	if _, err := tx.Exec(`DELETE FROM task_runs WHERE task_id = ?`, id); err != nil {
		_ = tx.Rollback()
		return err
	}
	res, err := tx.Exec(`DELETE FROM tasks WHERE id = ?`, id)
	if err != nil {
		_ = tx.Rollback()
		return err
	}
	n, err := res.RowsAffected()
	if err != nil {
		_ = tx.Rollback()
		return err
	}
	if n == 0 {
		_ = tx.Rollback()
		return ErrTaskNotFound
	}
	return tx.Commit()
}

// SetTaskBranch stores the English branch slug made from source, the task name it came from.
func (s *Store) SetTaskBranch(id string, slug, source string) (Task, error) {
	slug = strings.TrimSpace(slug)
	source = strings.TrimSpace(source)
	if id == "" {
		return Task{}, ErrTaskNotFound
	}
	if slug == "" {
		return Task{}, TaskError{Message: "The branch name could not be made from this task name."}
	}
	if len(slug) > 80 {
		return Task{}, TaskError{Message: "The branch name is too long."}
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	res, err := s.db.Exec(`UPDATE tasks SET branch_slug = ?, branch_for = ? WHERE id = ?`, slug, source, id)
	if err != nil {
		return Task{}, err
	}
	n, err := res.RowsAffected()
	if err != nil {
		return Task{}, err
	}
	if n == 0 {
		return Task{}, ErrTaskNotFound
	}
	return s.taskLocked(id)
}

func (s *Store) saveTask(id string, in TaskInput, createdAt, updatedAt, doneAt string) (Task, error) {
	tx, err := s.db.Begin()
	if err != nil {
		return Task{}, err
	}
	if id == "" {
		var err error
		id, err = newTaskID()
		if err != nil {
			_ = tx.Rollback()
			return Task{}, err
		}
		createdAt = updatedAt
		if _, err = tx.Exec(`INSERT INTO tasks (id, name, description, column_name, created_at, updated_at, done_at, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id)
				VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, id, in.Name, in.Description, in.Column, createdAt, updatedAt, doneAt, in.ParentID, in.EpicKey, in.EpicBoardID, in.EpicBoardName, epicOwnerBit(in.EpicOwner), in.GroupID); err != nil {
			_ = tx.Rollback()
			return Task{}, err
		}
	} else {
		res, err := tx.Exec(`UPDATE tasks SET name = ?, description = ?, column_name = ?, updated_at = ?, done_at = ?, parent_id = ?, epic_key = ?, epic_board_id = ?, epic_board_name = ?, epic_owner = ?, group_id = ? WHERE id = ?`,
			in.Name, in.Description, in.Column, updatedAt, doneAt, in.ParentID, in.EpicKey, in.EpicBoardID, in.EpicBoardName, epicOwnerBit(in.EpicOwner), in.GroupID, id)
		if err != nil {
			_ = tx.Rollback()
			return Task{}, err
		}
		n, err := res.RowsAffected()
		if err != nil {
			_ = tx.Rollback()
			return Task{}, err
		}
		if n == 0 {
			_ = tx.Rollback()
			return Task{}, ErrTaskNotFound
		}
	}
	previous := map[string]string{}
	if id != "" {
		rows, err := tx.Query(`SELECT agent_id, status FROM task_agents WHERE task_id = ?`, id)
		if err != nil {
			_ = tx.Rollback()
			return Task{}, err
		}
		for rows.Next() {
			var agentID, status string
			if err := rows.Scan(&agentID, &status); err != nil {
				rows.Close()
				_ = tx.Rollback()
				return Task{}, err
			}
			previous[agentID] = status
		}
		if err := rows.Err(); err != nil {
			rows.Close()
			_ = tx.Rollback()
			return Task{}, err
		}
		rows.Close()
		if _, err := tx.Exec(`DELETE FROM task_agents WHERE task_id = ?`, id); err != nil {
			_ = tx.Rollback()
			return Task{}, err
		}
	}
	if len(in.Agents) > 0 {
		stmt, err := tx.Prepare(`INSERT INTO task_agents (task_id, agent_id, status, position) VALUES (?, ?, ?, ?)`)
		if err != nil {
			_ = tx.Rollback()
			return Task{}, err
		}
		for i, agent := range in.Agents {
			status := TaskPending
			if stored, ok := previous[agent.AgentID]; ok && stored != "" {
				status = stored
			}
			if _, err := stmt.Exec(id, agent.AgentID, status, i); err != nil {
				stmt.Close()
				_ = tx.Rollback()
				return Task{}, err
			}
		}
		stmt.Close()
	}
	if _, err := tx.Exec(`DELETE FROM task_runs WHERE task_id = ? AND agent_id NOT IN (SELECT agent_id FROM task_agents WHERE task_id = ?)`, id, id); err != nil {
		_ = tx.Rollback()
		return Task{}, err
	}
	if err := tx.Commit(); err != nil {
		return Task{}, err
	}
	return s.taskLocked(id)
}

func (s *Store) tasksLocked() ([]Task, error) {
	rows, err := s.db.Query(`SELECT ` + taskSelect + ` FROM tasks ORDER BY updated_at DESC, id DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []Task{}
	for rows.Next() {
		item, err := scanTask(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, item)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	agents, err := s.db.Query(`SELECT task_id, agent_id, status FROM task_agents ORDER BY task_id, position, agent_id`)
	if err != nil {
		return nil, err
	}
	defer agents.Close()
	byTask := map[string][]TaskAgent{}
	for agents.Next() {
		var taskID string
		var agent TaskAgent
		if err := agents.Scan(&taskID, &agent.AgentID, &agent.Status); err != nil {
			return nil, err
		}
		byTask[taskID] = append(byTask[taskID], agent)
	}
	if err := agents.Err(); err != nil {
		return nil, err
	}
	for i := range out {
		list := byTask[out[i].ID]
		if list == nil {
			list = []TaskAgent{}
		}
		out[i].Agents = list
	}
	if err := s.attachRunsLocked(out); err != nil {
		return nil, err
	}
	return out, nil
}

func (s *Store) taskLocked(id string) (Task, error) {
	if id == "" {
		return Task{}, ErrTaskNotFound
	}
	row := s.db.QueryRow(`SELECT `+taskSelect+` FROM tasks WHERE id = ?`, id)
	item, err := scanTask(row)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return Task{}, ErrTaskNotFound
		}
		return Task{}, err
	}
	rows, err := s.db.Query(`SELECT agent_id, status FROM task_agents WHERE task_id = ? ORDER BY position, agent_id`, id)
	if err != nil {
		return Task{}, err
	}
	defer rows.Close()
	item.Agents = []TaskAgent{}
	for rows.Next() {
		var agent TaskAgent
		if err := rows.Scan(&agent.AgentID, &agent.Status); err != nil {
			return Task{}, err
		}
		item.Agents = append(item.Agents, agent)
	}
	if err := rows.Err(); err != nil {
		return Task{}, err
	}
	tasks := []Task{item}
	if err := s.attachRunsLocked(tasks); err != nil {
		return Task{}, err
	}
	return tasks[0], nil
}

type taskScanner interface {
	Scan(dest ...any) error
}

func scanTask(row taskScanner) (Task, error) {
	var item Task
	var started, owner int
	err := row.Scan(&item.ID, &item.Name, &item.Description, &item.Column, &item.CreatedAt, &item.UpdatedAt, &item.DoneAt, &item.BranchSlug, &item.BranchFor, &started, &item.ParentID, &item.EpicKey, &item.EpicBoardID, &item.EpicBoardName, &owner, &item.GroupID)
	item.Started = started != 0
	item.EpicOwner = owner != 0
	if err != nil {
		return Task{}, err
	}
	item.Agents = []TaskAgent{}
	item.Runs = []TaskRun{}
	return item, nil
}

func (s *Store) markStartedLocked(taskID string) error {
	_, err := s.db.Exec(`UPDATE tasks SET started = 1 WHERE id = ?`, taskID)
	return err
}

func (s *Store) syncColumnLocked(taskID string) error {
	var started int
	var column string
	if err := s.db.QueryRow(`SELECT started, column_name FROM tasks WHERE id = ?`, taskID).Scan(&started, &column); err != nil {
		return err
	}
	if started == 0 {
		return nil
	}
	var total, completed int
	if err := s.db.QueryRow(`SELECT COUNT(*), COALESCE(SUM(CASE WHEN status = ? THEN 1 ELSE 0 END), 0) FROM task_agents WHERE task_id = ?`, TaskCompleted, taskID).Scan(&total, &completed); err != nil {
		return err
	}
	next := ColumnProgress
	if total > 0 && completed == total {
		next = ColumnDone
	}
	if next == column {
		return nil
	}
	doneAt := ""
	if next == ColumnDone {
		doneAt = now()
	}
	_, err := s.db.Exec(`UPDATE tasks SET column_name = ?, done_at = ?, updated_at = ? WHERE id = ?`, next, doneAt, now(), taskID)
	return err
}

func normalizeTask(in TaskInput) (TaskInput, error) {
	in.Name = strings.TrimSpace(in.Name)
	in.Description = strings.TrimSpace(in.Description)
	in.Column = strings.TrimSpace(in.Column)
	if in.Column == "" {
		in.Column = ColumnTodo
	}
	switch in.Column {
	case ColumnTodo, ColumnProgress, ColumnDone:
	default:
		return TaskInput{}, TaskError{Message: "Choose TODO, In Progress, or Done."}
	}
	if in.Name == "" {
		return TaskInput{}, TaskError{Message: "Enter a task name."}
	}
	if len(in.Name) > 200 {
		return TaskInput{}, TaskError{Message: "Task name is longer than 200 characters."}
	}
	if len(in.Description) > 4000 {
		return TaskInput{}, TaskError{Message: "Description is longer than 4000 characters."}
	}
	if len(in.Agents) > 100 {
		return TaskInput{}, TaskError{Message: "A task can use at most 100 agents."}
	}
	seen := map[string]bool{}
	agents := make([]TaskAgent, 0, len(in.Agents))
	for _, agent := range in.Agents {
		id := strings.TrimSpace(agent.AgentID)
		status := strings.TrimSpace(agent.Status)
		if status == "" {
			status = TaskPending
		}
		if id == "" {
			return TaskInput{}, TaskError{Message: "Choose an agent."}
		}
		if !validAgentID(id) {
			return TaskInput{}, TaskError{Message: id + " is not an agent in this app."}
		}
		switch status {
		case TaskPending, TaskDoing, TaskReview, TaskCompleted:
		default:
			return TaskInput{}, TaskError{Message: "Agent status is not valid."}
		}
		if seen[id] {
			continue
		}
		seen[id] = true
		agents = append(agents, TaskAgent{AgentID: id, Status: status})
	}
	in.Agents = agents
	return in, nil
}

func (s *Store) resolveTaskLinkLocked(in TaskInput) (TaskInput, error) {
	in.GroupID = strings.TrimSpace(in.GroupID)
	in.ParentID = ""
	in.EpicKey = ""
	in.EpicBoardID = ""
	in.EpicBoardName = ""
	in.EpicOwner = false
	if in.GroupID == "" {
		return in, nil
	}
	group, err := s.groupLocked(in.GroupID)
	if err != nil {
		if errors.Is(err, ErrGroupNotFound) {
			return TaskInput{}, TaskError{Message: "That group was not found."}
		}
		return TaskInput{}, err
	}
	in.EpicKey = group.EpicKey
	in.EpicBoardID = group.EpicBoardID
	in.EpicBoardName = group.EpicBoardName
	return in, nil
}

func epicOwnerBit(owner bool) int {
	if owner {
		return 1
	}
	return 0
}

func validAgentID(id string) bool {
	if id == "" || len(id) > 80 {
		return false
	}
	for _, r := range id {
		if (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') || r == '_' || r == '-' {
			continue
		}
		return false
	}
	return true
}

const ulidAlphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

// newTaskID returns a ULID: 48 bits of millisecond time, then 80 random bits,
// written as 26 Crockford base32 characters.
func newTaskID() (string, error) {
	var raw [16]byte
	ms := uint64(time.Now().UnixMilli())
	raw[0] = byte(ms >> 40)
	raw[1] = byte(ms >> 32)
	raw[2] = byte(ms >> 24)
	raw[3] = byte(ms >> 16)
	raw[4] = byte(ms >> 8)
	raw[5] = byte(ms)
	if _, err := rand.Read(raw[6:]); err != nil {
		return "", err
	}
	return encodeULID(raw), nil
}

func encodeULID(id [16]byte) string {
	var dst [26]byte
	acc := 0
	bits := 0
	pos := 25
	for i := 15; i >= 0; i-- {
		acc |= int(id[i]) << bits
		bits += 8
		for bits >= 5 {
			dst[pos] = ulidAlphabet[acc&31]
			pos--
			acc >>= 5
			bits -= 5
		}
	}
	dst[pos] = ulidAlphabet[acc&31]
	return string(dst[:])
}

// migrateTaskIDs copies integer task ids into text. Existing ids stay their
// decimal form so an Orca group named with that id still matches. New tasks
// use ULIDs.
func (s *Store) migrateTaskIDs() error {
	var kind string
	if err := s.db.QueryRow(`SELECT type FROM pragma_table_info('tasks') WHERE name = 'id'`).Scan(&kind); err != nil {
		return err
	}
	if !strings.EqualFold(kind, "INTEGER") {
		return nil
	}
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	statements := []string{
		`CREATE TABLE tasks_text (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL,
			description TEXT NOT NULL,
			column_name TEXT NOT NULL,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			done_at TEXT NOT NULL,
			branch_slug TEXT NOT NULL DEFAULT '',
			branch_for TEXT NOT NULL DEFAULT '',
			started INTEGER NOT NULL DEFAULT 0,
			parent_id TEXT NOT NULL DEFAULT '',
			epic_key TEXT NOT NULL DEFAULT '',
			epic_board_id TEXT NOT NULL DEFAULT '',
			epic_board_name TEXT NOT NULL DEFAULT '',
			epic_owner INTEGER NOT NULL DEFAULT 0,
			group_id TEXT NOT NULL DEFAULT ''
		)`,
		`INSERT INTO tasks_text (id, name, description, column_name, created_at, updated_at, done_at, branch_slug, branch_for, started, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id)
			SELECT CAST(id AS TEXT), name, description, column_name, created_at, updated_at, done_at, branch_slug, branch_for, started, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id FROM tasks`,
		`DROP TABLE tasks`,
		`ALTER TABLE tasks_text RENAME TO tasks`,
		`CREATE TABLE task_agents_text (
			task_id TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			status TEXT NOT NULL,
			position INTEGER NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		)`,
		`INSERT INTO task_agents_text (task_id, agent_id, status, position)
			SELECT CAST(task_id AS TEXT), agent_id, status, position FROM task_agents`,
		`DROP TABLE task_agents`,
		`ALTER TABLE task_agents_text RENAME TO task_agents`,
		`CREATE TABLE task_runs_text (
			task_id TEXT NOT NULL,
			agent_id TEXT NOT NULL,
			terminal TEXT NOT NULL,
			orca_status TEXT NOT NULL,
			log TEXT NOT NULL,
			ask_prompt TEXT NOT NULL DEFAULT '',
			ask_options TEXT NOT NULL DEFAULT '[]',
			started_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		)`,
		`INSERT INTO task_runs_text (task_id, agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at)
			SELECT CAST(task_id AS TEXT), agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at FROM task_runs`,
		`DROP TABLE task_runs`,
		`ALTER TABLE task_runs_text RENAME TO task_runs`,
	}
	for _, statement := range statements {
		if _, err := tx.Exec(statement); err != nil {
			_ = tx.Rollback()
			return err
		}
	}
	return tx.Commit()
}
