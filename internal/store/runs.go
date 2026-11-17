package store

import (
	"database/sql"
	"encoding/json"
	"strings"
)

// TaskRun is one agent's Orca terminal for a task.
type TaskRun struct {
	TaskID     string   `json:"-"`
	AgentID    string   `json:"agentId"`
	Terminal   string   `json:"terminal"`
	Status     string   `json:"status"`
	Log        string   `json:"log"`
	AskPrompt  string   `json:"askPrompt"`
	AskOptions []string `json:"askOptions"`
	StartedAt  string   `json:"startedAt"`
	UpdatedAt  string   `json:"updatedAt"`
}

func (s *Store) initRuns() error {
	_, err := s.db.Exec(`CREATE TABLE IF NOT EXISTS task_runs (
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
	)`)
	if err != nil {
		return err
	}
	return s.ensureRunColumns()
}

func (s *Store) ensureRunColumns() error {
	rows, err := s.db.Query(`PRAGMA table_info(task_runs)`)
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
	if !have["ask_prompt"] {
		if _, err := s.db.Exec(`ALTER TABLE task_runs ADD COLUMN ask_prompt TEXT NOT NULL DEFAULT ''`); err != nil {
			return err
		}
	}
	if !have["ask_options"] {
		if _, err := s.db.Exec(`ALTER TABLE task_runs ADD COLUMN ask_options TEXT NOT NULL DEFAULT '[]'`); err != nil {
			return err
		}
	}
	return nil
}

// SaveTaskRun records the Orca terminal for one agent on a task.
// A new start replaces the previous terminal and log.
func (s *Store) SaveTaskRun(taskID string, agentID, terminal string) error {
	agentID = strings.TrimSpace(agentID)
	terminal = strings.TrimSpace(terminal)
	if taskID == "" || agentID == "" || terminal == "" {
		return TaskError{Message: "Orca did not return a terminal for the agent."}
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	var n int
	if err := s.db.QueryRow(`SELECT COUNT(*) FROM tasks WHERE id = ?`, taskID).Scan(&n); err != nil {
		return err
	}
	if n == 0 {
		return ErrTaskNotFound
	}
	stamp := now()
	_, err := s.db.Exec(`INSERT INTO task_runs (task_id, agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at)
		VALUES (?, ?, ?, 'running', '', '', '[]', ?, ?)
		ON CONFLICT(task_id, agent_id) DO UPDATE SET
			terminal = excluded.terminal,
			orca_status = 'running',
			log = '',
			ask_prompt = '',
			ask_options = '[]',
			started_at = excluded.started_at,
			updated_at = excluded.updated_at`,
		taskID, agentID, terminal, stamp, stamp)
	if err != nil {
		return err
	}
	if _, err = s.db.Exec(`UPDATE task_agents SET status = ? WHERE task_id = ? AND agent_id = ?`, TaskDoing, taskID, agentID); err != nil {
		return err
	}
	if err = s.markStartedLocked(taskID); err != nil {
		return err
	}
	return s.syncColumnLocked(taskID)
}

// OpenRuns returns terminals that have not exited, oldest task first.
func (s *Store) OpenRuns() ([]TaskRun, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	rows, err := s.db.Query(`SELECT task_id, agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at
		FROM task_runs WHERE orca_status != 'exited' ORDER BY task_id, agent_id`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanRuns(rows)
}

// RecordTaskRun stores the latest Orca status, transcript, and question for one agent.
// phase is Doing or Review. Completed stays Completed until the user approves.
func (s *Store) RecordTaskRun(taskID string, agentID, status, log, phase, askPrompt string, askOptions []string) (Task, error) {
	agentID = strings.TrimSpace(agentID)
	status = strings.TrimSpace(status)
	phase = strings.TrimSpace(phase)
	if taskID == "" || agentID == "" || status == "" {
		return Task{}, TaskError{Message: "Orca did not return a terminal for the agent."}
	}
	if phase == TaskDoing {
		askPrompt = ""
		askOptions = nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	res, err := s.db.Exec(`UPDATE task_runs SET orca_status = ?, log = ?, updated_at = ? WHERE task_id = ? AND agent_id = ?`,
		status, tailText(log, 8000), now(), taskID, agentID)
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
	if phase == TaskDoing || phase == TaskReview {
		encoded := []byte("[]")
		if askOptions != nil {
			encoded, err = json.Marshal(askOptions)
			if err != nil {
				return Task{}, err
			}
		}
		if _, err := s.db.Exec(`UPDATE task_runs SET ask_prompt = ?, ask_options = ? WHERE task_id = ? AND agent_id = ?`,
			strings.TrimSpace(askPrompt), string(encoded), taskID, agentID); err != nil {
			return Task{}, err
		}
		if _, err := s.db.Exec(`UPDATE task_agents SET status = ? WHERE task_id = ? AND agent_id = ? AND status != ?`,
			phase, taskID, agentID, TaskCompleted); err != nil {
			return Task{}, err
		}
	}
	if err := s.syncColumnLocked(taskID); err != nil {
		return Task{}, err
	}
	return s.taskLocked(taskID)
}

// ResumeAgent sends an agent that is waiting for review back to work.
func (s *Store) ResumeAgent(taskID, agentID string) (Task, error) {
	agentID = strings.TrimSpace(agentID)
	s.mu.Lock()
	defer s.mu.Unlock()
	res, err := s.db.Exec(`UPDATE task_agents SET status = ? WHERE task_id = ? AND agent_id = ? AND status = ?`,
		TaskDoing, taskID, agentID, TaskReview)
	if err != nil {
		return Task{}, err
	}
	n, err := res.RowsAffected()
	if err != nil {
		return Task{}, err
	}
	if n == 0 {
		return Task{}, TaskError{Message: "This agent is not waiting for review."}
	}
	if _, err := s.db.Exec(`UPDATE task_runs SET orca_status = 'running', ask_prompt = '', ask_options = '[]', updated_at = ? WHERE task_id = ? AND agent_id = ?`,
		now(), taskID, agentID); err != nil {
		return Task{}, err
	}
	if err := s.syncColumnLocked(taskID); err != nil {
		return Task{}, err
	}
	return s.taskLocked(taskID)
}

// ApproveAgent marks a waiting agent completed. The user is the only one who can do this.
func (s *Store) ApproveAgent(taskID, agentID string) (Task, error) {
	agentID = strings.TrimSpace(agentID)
	s.mu.Lock()
	defer s.mu.Unlock()
	res, err := s.db.Exec(`UPDATE task_agents SET status = ? WHERE task_id = ? AND agent_id = ? AND status = ?`,
		TaskCompleted, taskID, agentID, TaskReview)
	if err != nil {
		return Task{}, err
	}
	n, err := res.RowsAffected()
	if err != nil {
		return Task{}, err
	}
	if n == 0 {
		return Task{}, TaskError{Message: "Approve is available when the agent is waiting for review."}
	}
	if err := s.syncColumnLocked(taskID); err != nil {
		return Task{}, err
	}
	return s.taskLocked(taskID)
}

func (s *Store) attachRunsLocked(tasks []Task) error {
	for i := range tasks {
		tasks[i].Runs = []TaskRun{}
	}
	if len(tasks) == 0 {
		return nil
	}
	rows, err := s.db.Query(`SELECT task_id, agent_id, terminal, orca_status, log, ask_prompt, ask_options, started_at, updated_at FROM task_runs ORDER BY task_id, agent_id`)
	if err != nil {
		return err
	}
	defer rows.Close()
	runs, err := scanRuns(rows)
	if err != nil {
		return err
	}
	byTask := map[string][]TaskRun{}
	for _, run := range runs {
		byTask[run.TaskID] = append(byTask[run.TaskID], run)
	}
	for i := range tasks {
		if list := byTask[tasks[i].ID]; list != nil {
			tasks[i].Runs = list
		}
	}
	return nil
}

type runScanner interface {
	Next() bool
	Scan(dest ...any) error
	Err() error
}

func scanRuns(rows runScanner) ([]TaskRun, error) {
	out := []TaskRun{}
	for rows.Next() {
		var run TaskRun
		var options string
		if err := rows.Scan(&run.TaskID, &run.AgentID, &run.Terminal, &run.Status, &run.Log, &run.AskPrompt, &options, &run.StartedAt, &run.UpdatedAt); err != nil {
			return nil, err
		}
		run.AskOptions = decodeOptions(options)
		out = append(out, run)
	}
	return out, rows.Err()
}

func decodeOptions(raw string) []string {
	out := []string{}
	if strings.TrimSpace(raw) == "" {
		return out
	}
	if err := json.Unmarshal([]byte(raw), &out); err != nil || out == nil {
		return []string{}
	}
	return out
}

func tailText(text string, max int) string {
	runes := []rune(text)
	if len(runes) <= max {
		return text
	}
	return string(runes[len(runes)-max:])
}
