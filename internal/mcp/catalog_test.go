package mcp

import (
	"context"
	"errors"
	"strings"
	"testing"
)

func TestBoardsReadsTheAgileList(t *testing.T) {
	restore := stubJSON(t, func(rawURL, _, _ string) (int, []byte, error) {
		if !strings.Contains(rawURL, "/rest/agile/1.0/board") || !strings.Contains(rawURL, "name=KIKI") {
			t.Fatalf("url = %s", rawURL)
		}
		return 200, []byte(`{"values":[{"id":12,"name":"KIKI"},{"id":"","name":"skip"}]}`), nil
	})
	defer restore()
	got, err := Boards(context.Background(), sampleAtlassian(), "KIKI")
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 1 || got[0].ID != "12" || got[0].Name != "KIKI" {
		t.Fatalf("boards = %#v", got)
	}
}

func TestSpacesFiltersByName(t *testing.T) {
	restore := stubJSON(t, func(rawURL, _, _ string) (int, []byte, error) {
		if strings.Contains(rawURL, "cursor=next") {
			return 200, []byte(`{"results":[{"id":"2","key":"OPS","name":"Operations"}]}`), nil
		}
		return 200, []byte(`{"results":[{"id":"1","key":"ENG","name":"Engineering"}],"_links":{"next":"/wiki/api/v2/spaces?cursor=next"}}`), nil
	})
	defer restore()
	got, err := Spaces(context.Background(), sampleAtlassian(), "ops")
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 1 || got[0].Key != "OPS" {
		t.Fatalf("spaces = %#v", got)
	}
}

func TestPagesSearchUsesTheSpaceKey(t *testing.T) {
	restore := stubJSON(t, func(rawURL, _, _ string) (int, []byte, error) {
		if !strings.Contains(rawURL, "/wiki/rest/api/content/search") || !strings.Contains(rawURL, "ENG") || !strings.Contains(rawURL, "Requirements") {
			t.Fatalf("url = %s", rawURL)
		}
		return 200, []byte(`{"results":[{"title":"Requirements","content":{"id":"99","title":"Requirements"}}]}`), nil
	})
	defer restore()
	got, err := Pages(context.Background(), sampleAtlassian(), "111", "ENG", "Requirements")
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 1 || got[0].ID != "99" || got[0].Name != "Requirements" {
		t.Fatalf("pages = %#v", got)
	}
}

func TestBoardsReportsARejectedToken(t *testing.T) {
	restore := stubJSON(t, func(string, string, string) (int, []byte, error) {
		return 401, []byte(`{"message":"client must be authenticated"}`), nil
	})
	defer restore()
	_, err := Boards(context.Background(), sampleAtlassian(), "")
	if err == nil || !strings.Contains(err.Error(), "rejected") {
		t.Fatalf("err = %v", err)
	}
}

func TestBoardsNeedsTheSavedToken(t *testing.T) {
	_, err := Boards(context.Background(), Atlassian{Enabled: true, Jira: Service{Enabled: true, URL: "https://acme.atlassian.net"}}, "")
	if err == nil || !strings.Contains(err.Error(), "API token") {
		t.Fatalf("err = %v", err)
	}
}

func stubJSON(t *testing.T, fn func(rawURL, user, token string) (int, []byte, error)) func() {
	t.Helper()
	previous := GetJSON
	GetJSON = func(ctx context.Context, rawURL, user, token string) (int, []byte, error) {
		if ctx == nil {
			return 0, nil, errors.New("missing context")
		}
		return fn(rawURL, user, token)
	}
	return func() { GetJSON = previous }
}
