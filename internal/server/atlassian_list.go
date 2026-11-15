package server

import (
	"net/http"

	"kiki/internal/mcp"
)

func (s *Server) handleJiraBoards(w http.ResponseWriter, r *http.Request) {
	s.writeAtlassianList(w, r, func(cfg mcp.Atlassian) ([]mcp.Hit, error) {
		return mcp.Boards(r.Context(), cfg, r.URL.Query().Get("q"))
	})
}

func (s *Server) handleConfluenceSpaces(w http.ResponseWriter, r *http.Request) {
	s.writeAtlassianList(w, r, func(cfg mcp.Atlassian) ([]mcp.Hit, error) {
		return mcp.Spaces(r.Context(), cfg, r.URL.Query().Get("q"))
	})
}

func (s *Server) handleConfluencePages(w http.ResponseWriter, r *http.Request) {
	s.writeAtlassianList(w, r, func(cfg mcp.Atlassian) ([]mcp.Hit, error) {
		return mcp.Pages(r.Context(), cfg, r.URL.Query().Get("space"), r.URL.Query().Get("key"), r.URL.Query().Get("q"))
	})
}

func (s *Server) writeAtlassianList(w http.ResponseWriter, r *http.Request, load func(mcp.Atlassian) ([]mcp.Hit, error)) {
	if s.Store == nil {
		writeErr(w, http.StatusBadRequest, errString("Atlassian is not set up"))
		return
	}
	cfg, err := s.Store.Atlassian()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	items, err := load(cfg)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if items == nil {
		items = []mcp.Hit{}
	}
	writeJSON(w, http.StatusOK, map[string]any{"items": items})
}

type errString string

func (e errString) Error() string { return string(e) }
