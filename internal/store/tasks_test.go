package store

import (
	"database/sql"
	"errors"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

var taskULID = regexp.MustCompile(`^[0-9A-HJKMNP-TV-Z]{26}$`)

func openTaskStore(t *testing.T) *Store {
	t.Helper()
	db, err := Open(filepath.Join(t.TempDir(), "kiki.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	return db
}

func TestEncodeULID(t *testing.T) {
	var zero [16]byte
	if got := encodeULID(zero); got != "00000000000000000000000000" {
		t.Fatalf("zero = %s", got)
	}
	var one [16]byte
	one[15] = 1
	if got := encodeULID(one); got != "00000000000000000000000001" {
		t.Fatalf("one = %s", got)
	}
	var last [16]byte
	last[15] = 31
	if got := encodeULID(last); got != "0000000000000000000000000Z" {
		t.Fatalf("last = %s", got)
	}
	var ms [16]byte
	ms[5] = 1
	if got := encodeULID(ms); got != "00000000010000000000000000" {
		t.Fatalf("ms = %s", got)
	}
}

func TestTasksRoundTrip(t *testing.T) {
	db := openTaskStore(t)
	if !strings.HasSuffix(db.LogFile(), "kiki.db") {
		t.Fatalf("log = %s", db.LogFile())
	}
	empty, err := db.Tasks()
	if err != nil {
		t.Fatal(err)
	}
	if len(empty) != 0 {
		t.Fatalf("tasks = %+v", empty)
	}
	created, err := db.CreateTask(TaskInput{
		Name:        "  Ship billing  ",
		Description: "Add the invoice page.",
		Agents: []TaskAgent{
			{AgentID: "requirement_planner", Status: ""},
			{AgentID: "requirement_planner", Status: TaskDoing},
			{AgentID: "backend_runner", Status: TaskReview},
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	if !taskULID.MatchString(created.ID) || created.Column != ColumnTodo || created.DoneAt != "" {
		t.Fatalf("created = %+v", created)
	}
	if len(created.Agents) != 2 || created.Agents[0].AgentID != "requirement_planner" || created.Agents[0].Status != TaskPending {
		t.Fatalf("agents = %+v", created.Agents)
	}
	if created.Agents[1].Status != TaskPending {
		t.Fatalf("agents = %+v", created.Agents)
	}
	loaded, err := db.Task(created.ID)
	if err != nil {
		t.Fatal(err)
	}
	if loaded.Name != "Ship billing" || loaded.Description != "Add the invoice page." {
		t.Fatalf("loaded = %+v", loaded)
	}
	done, err := db.UpdateTask(created.ID, TaskInput{
		Name:        "Ship billing",
		Description: "Add the invoice page.",
		Column:      ColumnDone,
		Agents:      []TaskAgent{{AgentID: "backend_runner", Status: TaskCompleted}},
	})
	if err != nil {
		t.Fatal(err)
	}
	if done.Column != ColumnTodo || done.DoneAt != "" || done.CreatedAt != created.CreatedAt || done.Started {
		t.Fatalf("done = %+v", done)
	}
	kept, err := db.UpdateTask(created.ID, TaskInput{
		Name:   "Ship billing",
		Column: ColumnDone,
		Agents: done.Agents,
	})
	if err != nil {
		t.Fatal(err)
	}
	if kept.Column != ColumnTodo || kept.DoneAt != "" || kept.Description != "" {
		t.Fatalf("kept = %+v", kept)
	}
	back, err := db.UpdateTask(created.ID, TaskInput{Name: "Ship billing", Column: ColumnProgress, Agents: done.Agents})
	if err != nil {
		t.Fatal(err)
	}
	if back.Column != ColumnTodo || back.DoneAt != "" {
		t.Fatalf("back = %+v", back)
	}
	if err := db.DeleteTask(created.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := db.Task(created.ID); err != ErrTaskNotFound {
		t.Fatalf("delete err = %v", err)
	}
	if err := db.DeleteTask(created.ID); err != ErrTaskNotFound {
		t.Fatalf("second delete err = %v", err)
	}
}

func TestTaskBranchSlugSurvivesEdit(t *testing.T) {
	db := openTaskStore(t)
	created, err := db.CreateTask(TaskInput{Name: "Ship billing", Description: "Add invoices."})
	if err != nil {
		t.Fatal(err)
	}
	named, err := db.SetTaskBranch(created.ID, "add_cart", created.Name)
	if err != nil {
		t.Fatal(err)
	}
	if named.BranchSlug != "add_cart" || named.BranchFor != "Ship billing" {
		t.Fatalf("named = %+v", named)
	}
	edited, err := db.UpdateTask(created.ID, TaskInput{
		Name:        created.Name,
		Description: "Add the invoice page.",
		Column:      ColumnProgress,
	})
	if err != nil {
		t.Fatal(err)
	}
	if edited.BranchSlug != "add_cart" || edited.BranchFor != "Ship billing" || edited.Description != "Add the invoice page." {
		t.Fatalf("edited = %+v", edited)
	}
}

func TestTaskRunFollowsAgent(t *testing.T) {
	db := openTaskStore(t)
	created, err := db.CreateTask(TaskInput{
		Name:   "Ship billing",
		Agents: []TaskAgent{{AgentID: "backend_runner", Status: TaskDoing}},
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := db.SaveTaskRun(created.ID, "backend_runner", "term_a"); err != nil {
		t.Fatal(err)
	}
	loaded, err := db.Task(created.ID)
	if err != nil {
		t.Fatal(err)
	}
	if !loaded.Started || loaded.Column != ColumnProgress || len(loaded.Runs) != 1 || loaded.Runs[0].Terminal != "term_a" || loaded.Runs[0].Status != "running" || loaded.Runs[0].Log != "" {
		t.Fatalf("loaded = %+v", loaded)
	}
	open, err := db.OpenRuns()
	if err != nil || len(open) != 1 || open[0].TaskID != created.ID {
		t.Fatalf("open = %+v err = %v", open, err)
	}
	finished, err := db.RecordTaskRun(created.ID, "backend_runner", "exited", "I added the invoice route.", TaskReview, "", nil)
	if err != nil {
		t.Fatal(err)
	}
	if finished.Agents[0].Status != TaskReview || finished.Runs[0].Status != "exited" || finished.Runs[0].Log != "I added the invoice route." {
		t.Fatalf("finished = %+v", finished)
	}
	approved, err := db.ApproveAgent(created.ID, "backend_runner")
	if err != nil {
		t.Fatal(err)
	}
	if approved.Agents[0].Status != TaskCompleted || approved.Column != ColumnDone || approved.DoneAt == "" {
		t.Fatalf("approved = %+v", approved)
	}
	if _, err := db.ApproveAgent(created.ID, "backend_runner"); err == nil {
		t.Fatal("second approve succeeded")
	}
	open, err = db.OpenRuns()
	if err != nil || len(open) != 0 {
		t.Fatalf("open = %+v err = %v", open, err)
	}
	if _, err := db.UpdateTask(created.ID, TaskInput{
		Name:   "Renamed",
		Column: ColumnTodo,
		Agents: []TaskAgent{{AgentID: "requirement_planner", Status: TaskPending}},
	}); err == nil || err.Error() != "This task has already started." {
		t.Fatalf("update err = %v", err)
	}
	kept, err := db.Task(created.ID)
	if err != nil {
		t.Fatal(err)
	}
	if kept.Name != "Ship billing" || len(kept.Runs) != 1 || kept.Agents[0].AgentID != "backend_runner" || kept.Agents[0].Status != TaskCompleted {
		t.Fatalf("kept = %+v", kept)
	}
	if err := db.DeleteTask(created.ID); err != nil {
		t.Fatal(err)
	}
	open, err = db.OpenRuns()
	if err != nil || len(open) != 0 {
		t.Fatalf("open after delete = %+v err = %v", open, err)
	}
}

func TestTaskValidation(t *testing.T) {
	db := openTaskStore(t)
	if _, err := db.CreateTask(TaskInput{}); err == nil || err.Error() != "Enter a task name." {
		t.Fatalf("name err = %v", err)
	}
	if _, err := db.CreateTask(TaskInput{Name: "Ship", Column: "later"}); err == nil || err.Error() != "Choose TODO, In Progress, or Done." {
		t.Fatalf("column err = %v", err)
	}
	if _, err := db.CreateTask(TaskInput{Name: "Ship", Agents: []TaskAgent{{AgentID: "backend_runner", Status: "paused"}}}); err == nil || err.Error() != "Agent status is not valid." {
		t.Fatalf("status err = %v", err)
	}
	long := strings.Repeat("a", 4001)
	if _, err := db.CreateTask(TaskInput{Name: "Ship", Description: long}); err == nil || err.Error() != "Description is longer than 4000 characters." {
		t.Fatalf("description err = %v", err)
	}
}

func TestTaskGroup(t *testing.T) {
	db := openTaskStore(t)
	if _, err := db.CreateGroup(GroupInput{}); err == nil || err.Error() != "Enter a group name." {
		t.Fatalf("name err = %v", err)
	}
	if _, err := db.CreateGroup(GroupInput{Name: "Billing", EpicKey: "nope", EpicBoardID: "12"}); err == nil || err.Error() != "The Jira epic key is not valid." {
		t.Fatalf("key err = %v", err)
	}
	local, err := db.CreateGroup(GroupInput{Name: "Local"})
	if err != nil {
		t.Fatal(err)
	}
	if local.EpicKey != "" || local.EpicBoardID != "" || local.Name != "Local" {
		t.Fatalf("local = %+v", local)
	}
	group, err := db.CreateGroup(GroupInput{Name: "Billing", Description: "The epic.", EpicKey: "kiki-8", EpicBoardID: "12", EpicBoardName: "KIKI"})
	if err != nil {
		t.Fatal(err)
	}
	if group.EpicKey != "KIKI-8" || group.EpicBoardID != "12" || group.Name != "Billing" || !taskULID.MatchString(group.ID) {
		t.Fatalf("group = %+v", group)
	}
	task, err := db.CreateTask(TaskInput{Name: "Invoice page", GroupID: group.ID})
	if err != nil {
		t.Fatal(err)
	}
	if task.GroupID != group.ID || task.EpicKey != "KIKI-8" || task.EpicBoardName != "KIKI" || task.EpicOwner || task.ParentID != "" {
		t.Fatalf("task = %+v", task)
	}
	if _, err := db.CreateTask(TaskInput{Name: "Missing", GroupID: "01ARZ3NDEKTSV4RRFFQ69G5FAV"}); err == nil || err.Error() != "That group was not found." {
		t.Fatalf("missing err = %v", err)
	}
	other, err := db.CreateGroup(GroupInput{Name: "Accounts", EpicKey: "KIKI-9", EpicBoardID: "12", EpicBoardName: "KIKI"})
	if err != nil {
		t.Fatal(err)
	}
	moved, err := db.UpdateTask(task.ID, TaskInput{Name: "Invoice page", GroupID: other.ID})
	if err != nil {
		t.Fatal(err)
	}
	if moved.GroupID != other.ID || moved.EpicKey != "KIKI-9" {
		t.Fatalf("moved = %+v", moved)
	}
	cleared, err := db.UpdateTask(task.ID, TaskInput{Name: "Invoice page"})
	if err != nil {
		t.Fatal(err)
	}
	if cleared.GroupID != "" || cleared.EpicKey != "" || cleared.EpicOwner {
		t.Fatalf("cleared = %+v", cleared)
	}
	listed, err := db.Groups()
	if err != nil || len(listed) != 3 || listed[0].ID != other.ID {
		t.Fatalf("groups = %+v err = %v", listed, err)
	}
}

func TestLegacyEpicBecomesGroup(t *testing.T) {
	db := openTaskStore(t)
	_, err := db.db.Exec(`INSERT INTO tasks (id, name, description, column_name, created_at, updated_at, done_at, parent_id, epic_key, epic_board_id, epic_board_name, epic_owner, group_id)
		VALUES ('1', 'Billing epic', 'The epic.', 'todo', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z', '', '', 'KIKI-8', '12', 'KIKI', 1, ''),
		       ('2', 'Invoice page', 'A child.', 'todo', '2020-01-02T00:00:00Z', '2020-01-02T00:00:00Z', '', '1', 'KIKI-8', '12', 'KIKI', 0, '')`)
	if err != nil {
		t.Fatal(err)
	}
	if err := db.migrateTaskGroups(); err != nil {
		t.Fatal(err)
	}
	groups, err := db.Groups()
	if err != nil || len(groups) != 1 || groups[0].Name != "Billing epic" || groups[0].EpicKey != "KIKI-8" || groups[0].Description != "The epic." {
		t.Fatalf("groups = %+v err = %v", groups, err)
	}
	owner, err := db.Task("1")
	if err != nil {
		t.Fatal(err)
	}
	child, err := db.Task("2")
	if err != nil {
		t.Fatal(err)
	}
	if owner.GroupID != groups[0].ID || owner.EpicOwner || owner.ParentID != "" || owner.EpicKey != "KIKI-8" {
		t.Fatalf("owner = %+v", owner)
	}
	if child.GroupID != groups[0].ID || child.ParentID != "" || child.EpicKey != "KIKI-8" {
		t.Fatalf("child = %+v", child)
	}
}

func TestLegacyNumericTaskIDStays(t *testing.T) {
	dir := t.TempDir()
	raw, err := sql.Open("sqlite", sqliteDSN(filepath.Join(dir, "kiki.db")))
	if err != nil {
		t.Fatal(err)
	}
	statements := []string{
		`CREATE TABLE tasks (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			name TEXT NOT NULL,
			description TEXT NOT NULL,
			column_name TEXT NOT NULL,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			done_at TEXT NOT NULL,
			branch_slug TEXT NOT NULL DEFAULT '',
			branch_for TEXT NOT NULL DEFAULT '',
			started INTEGER NOT NULL DEFAULT 0
		)`,
		`CREATE TABLE task_agents (
			task_id INTEGER NOT NULL,
			agent_id TEXT NOT NULL,
			status TEXT NOT NULL,
			position INTEGER NOT NULL,
			PRIMARY KEY (task_id, agent_id)
		)`,
		`CREATE TABLE task_runs (
			task_id INTEGER NOT NULL,
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
		`INSERT INTO tasks (id, name, description, column_name, created_at, updated_at, done_at, started)
			VALUES (1, 'Requirement', 'Manage stores', 'done', '2026-11-16T00:00:00Z', '2026-11-16T00:00:00Z', '2026-11-16T00:00:00Z', 1)`,
		`INSERT INTO task_agents (task_id, agent_id, status, position) VALUES (1, 'requirement_planner', 'completed', 0)`,
		`INSERT INTO task_runs (task_id, agent_id, terminal, orca_status, log, started_at, updated_at)
			VALUES (1, 'requirement_planner', 'term_old', 'idle', 'done', '2026-11-16T00:00:00Z', '2026-11-16T00:00:00Z')`,
	}
	for _, statement := range statements {
		if _, err := raw.Exec(statement); err != nil {
			_ = raw.Close()
			t.Fatal(err)
		}
	}
	if err := raw.Close(); err != nil {
		t.Fatal(err)
	}
	db, err := Open(filepath.Join(dir, "kiki.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = db.Close() })
	loaded, err := db.Task("1")
	if err != nil {
		t.Fatal(err)
	}
	if loaded.ID != "1" || loaded.Name != "Requirement" || !loaded.Started || loaded.Column != ColumnDone {
		t.Fatalf("legacy = %+v", loaded)
	}
	if len(loaded.Agents) != 1 || loaded.Agents[0].AgentID != "requirement_planner" || loaded.Agents[0].Status != TaskCompleted {
		t.Fatalf("agents = %+v", loaded.Agents)
	}
	if len(loaded.Runs) != 1 || loaded.Runs[0].Terminal != "term_old" {
		t.Fatalf("runs = %+v", loaded.Runs)
	}
	created, err := db.CreateTask(TaskInput{Name: "Next", Description: "A new task"})
	if err != nil {
		t.Fatal(err)
	}
	if !taskULID.MatchString(created.ID) || created.ID == "1" {
		t.Fatalf("created id = %q", created.ID)
	}
}

func TestDeleteGroupKeepsTask(t *testing.T) {
	db := openTaskStore(t)
	group, err := db.CreateGroup(GroupInput{Name: "Billing", EpicKey: "KIKI-8", EpicBoardID: "12", EpicBoardName: "KIKI"})
	if err != nil {
		t.Fatal(err)
	}
	task, err := db.CreateTask(TaskInput{Name: "Invoice page", GroupID: group.ID})
	if err != nil {
		t.Fatal(err)
	}
	if err := db.DeleteGroup(group.ID); err != nil {
		t.Fatal(err)
	}
	if err := db.DeleteGroup(group.ID); err != nil {
		t.Fatal(err)
	}
	listed, err := db.Groups()
	if err != nil || len(listed) != 1 || !listed[0].Deleted || listed[0].Name != "Billing" || listed[0].EpicKey != "KIKI-8" {
		t.Fatalf("groups = %+v err = %v", listed, err)
	}
	kept, err := db.Task(task.ID)
	if err != nil {
		t.Fatal(err)
	}
	if kept.GroupID != group.ID || kept.EpicKey != "KIKI-8" {
		t.Fatalf("task = %+v", kept)
	}
	saved, err := db.UpdateTask(task.ID, TaskInput{Name: "Invoice page", GroupID: group.ID})
	if err != nil {
		t.Fatal(err)
	}
	if saved.GroupID != group.ID || saved.EpicKey != "KIKI-8" {
		t.Fatalf("saved = %+v", saved)
	}
	if err := db.DeleteGroup("01ARZ3NDEKTSV4RRFFQ69G5FAV"); !errors.Is(err, ErrGroupNotFound) {
		t.Fatalf("missing err = %v", err)
	}
}
