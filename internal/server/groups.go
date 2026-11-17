package server

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"time"

	"kiki/internal/mcp"
	"kiki/internal/store"
)

func (s *Server) handleCreateGroup(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Name        string `json:"name"`
		Description string `json:"description"`
	}
	if !readJSON(w, r, &body) {
		return
	}
	name := strings.TrimSpace(body.Name)
	description := strings.TrimSpace(body.Description)
	if name == "" {
		writeTaskErr(w, store.TaskError{Message: "Enter a group name."})
		return
	}
	if s.Store == nil {
		writeTaskErr(w, store.TaskError{Message: "Turn on Jira in Settings before creating a group."})
		return
	}
	cfg, err := s.Store.Atlassian()
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	in := store.GroupInput{Name: name, Description: description}
	if cfg.Enabled && cfg.Jira.Enabled {
		ctx, cancel := context.WithTimeout(r.Context(), 45*time.Second)
		defer cancel()
		epic, err := mcp.CreateEpic(ctx, cfg, name, description)
		if err != nil {
			writeTaskErr(w, store.TaskError{Message: err.Error()})
			return
		}
		in.EpicKey = epic.Key
		in.EpicBoardID = epic.BoardID
		in.EpicBoardName = epic.BoardName
	}
	group, err := s.Store.CreateGroup(in)
	if err != nil {
		writeTaskErr(w, err)
		return
	}
	writeJSON(w, http.StatusCreated, map[string]any{"group": group})
}

func (s *Server) handleDeleteGroup(w http.ResponseWriter, r *http.Request) {
	if s.Store == nil {
		writeErr(w, http.StatusNotFound, store.ErrGroupNotFound)
		return
	}
	var body struct {
		Jira bool `json:"jira"`
	}
	if r.Body != nil {
		err := json.NewDecoder(http.MaxBytesReader(w, r.Body, 1<<20)).Decode(&body)
		if err != nil && !errors.Is(err, io.EOF) {
			writeErr(w, http.StatusBadRequest, errors.New("invalid data"))
			return
		}
	}
	id := r.PathValue("id")
	if body.Jira {
		if err := s.deleteGroupEpic(r.Context(), id); err != nil {
			if errors.Is(err, store.ErrGroupNotFound) {
				writeErr(w, http.StatusNotFound, err)
				return
			}
			writeTaskErr(w, err)
			return
		}
	}
	if err := s.Store.DeleteGroup(id); err != nil {
		if errors.Is(err, store.ErrGroupNotFound) {
			writeErr(w, http.StatusNotFound, err)
			return
		}
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}

// deleteGroupEpic removes the Jira epic stored on the group.
// A group with no epic has nothing to remove. The group stays when Jira refuses.
func (s *Server) deleteGroupEpic(ctx context.Context, id string) error {
	group, err := s.Store.Group(id)
	if err != nil {
		return err
	}
	key := strings.TrimSpace(group.EpicKey)
	if key == "" {
		return nil
	}
	cfg, err := s.Store.Atlassian()
	if err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(ctx, 45*time.Second)
	defer cancel()
	if err := mcp.DeleteIssue(ctx, cfg, key); err != nil {
		return store.TaskError{Message: err.Error()}
	}
	return nil
}
