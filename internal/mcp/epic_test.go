package mcp

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
)

func TestCreateEpicUsesTheSettingsBoard(t *testing.T) {
	cfg := sampleAtlassian()
	cfg.Jira.BoardID = "12"
	cfg.Jira.BoardName = "KIKI"
	var posted []byte
	restoreGet := stubJSON(t, func(rawURL, _, _ string) (int, []byte, error) {
		switch {
		case strings.Contains(rawURL, "/rest/agile/1.0/board/12") && !strings.Contains(rawURL, "/project"):
			return 200, []byte(`{"name":"KIKI","location":{"projectKey":"KIKI"}}`), nil
		case strings.HasSuffix(rawURL, "/issuetypes"):
			return 200, []byte(`{"issueTypes":[{"id":"10001","name":"Epic","hierarchyLevel":1}]}`), nil
		case strings.Contains(rawURL, "/issuetypes/10001"):
			return 200, []byte(`{"fields":[{"fieldId":"summary","name":"Summary","required":true},{"fieldId":"customfield_10011","name":"Epic Name","required":true,"schema":{"custom":"com.pyxis.greenhopper.jira:gh-epic-label"}},{"fieldId":"description","name":"Description","required":false,"schema":{"system":"description"}}]}`), nil
		default:
			t.Fatalf("url = %s", rawURL)
			return 0, nil, nil
		}
	})
	defer restoreGet()
	restorePost := stubPost(t, func(rawURL string, payload []byte) (int, []byte, error) {
		if !strings.HasSuffix(rawURL, "/rest/api/3/issue") {
			t.Fatalf("url = %s", rawURL)
		}
		posted = append([]byte(nil), payload...)
		return 201, []byte(`{"id":"10008","key":"kiki-8"}`), nil
	})
	defer restorePost()

	got, err := CreateEpic(context.Background(), cfg, " Ship billing ", "Add the invoice page.")
	if err != nil {
		t.Fatal(err)
	}
	if got.Key != "KIKI-8" || got.ID != "10008" || got.BoardID != "12" || got.BoardName != "KIKI" || got.URL != "https://acme.atlassian.net/browse/KIKI-8" {
		t.Fatalf("epic = %+v", got)
	}
	var body struct {
		Fields map[string]any `json:"fields"`
	}
	if err := json.Unmarshal(posted, &body); err != nil {
		t.Fatal(err)
	}
	if body.Fields["summary"] != "Ship billing" || body.Fields["customfield_10011"] != "Ship billing" {
		t.Fatalf("fields = %#v", body.Fields)
	}
	project, _ := body.Fields["project"].(map[string]any)
	issueType, _ := body.Fields["issuetype"].(map[string]any)
	if project["key"] != "KIKI" || issueType["id"] != "10001" {
		t.Fatalf("fields = %#v", body.Fields)
	}
	description, _ := body.Fields["description"].(map[string]any)
	if description["type"] != "doc" {
		t.Fatalf("description = %#v", description)
	}
}

func TestCreateEpicNeedsABoard(t *testing.T) {
	_, err := CreateEpic(context.Background(), sampleAtlassian(), "Ship billing", "")
	if err == nil || !strings.Contains(err.Error(), "Jira board") {
		t.Fatalf("err = %v", err)
	}
}

func TestCreateEpicReportsJira(t *testing.T) {
	cfg := sampleAtlassian()
	cfg.Jira.BoardID = "12"
	restoreGet := stubJSON(t, func(rawURL, _, _ string) (int, []byte, error) {
		if strings.Contains(rawURL, "/issuetype") {
			return 404, []byte(`{"message":"missing"}`), nil
		}
		return 200, []byte(`{"name":"KIKI","location":{"projectKey":"KIKI"}}`), nil
	})
	defer restoreGet()
	restorePost := stubPost(t, func(string, []byte) (int, []byte, error) {
		return 400, []byte(`{"errorMessages":["Epic Name is required"],"errors":{"customfield_10011":"Epic Name is required"}}`), nil
	})
	defer restorePost()
	_, err := CreateEpic(context.Background(), cfg, "Ship billing", "")
	if err == nil || !strings.Contains(err.Error(), "Epic Name is required") {
		t.Fatalf("err = %v", err)
	}
}

func TestDeleteIssue(t *testing.T) {
	cfg := sampleAtlassian()
	var got string
	restore := stubDelete(t, func(rawURL string) (int, []byte, error) {
		got = rawURL
		return 204, nil, nil
	})
	defer restore()
	if err := DeleteIssue(context.Background(), cfg, "kiki-12"); err != nil {
		t.Fatal(err)
	}
	if got != "https://acme.atlassian.net/rest/api/3/issue/KIKI-12" {
		t.Fatalf("url = %s", got)
	}

	restoreMissing := stubDelete(t, func(string) (int, []byte, error) {
		return 404, []byte(`{"errorMessages":["Issue does not exist"]}`), nil
	})
	defer restoreMissing()
	if err := DeleteIssue(context.Background(), cfg, "KIKI-12"); err != nil {
		t.Fatal(err)
	}

	restoreReject := stubDelete(t, func(string) (int, []byte, error) {
		return 400, []byte(`{"errorMessages":["The issue can not be deleted because it has sub-tasks."]}`), nil
	})
	defer restoreReject()
	err := DeleteIssue(context.Background(), cfg, "KIKI-12")
	if err == nil || !strings.Contains(err.Error(), "sub-tasks") {
		t.Fatalf("err = %v", err)
	}
	if err := DeleteIssue(context.Background(), cfg, "not a key"); err == nil {
		t.Fatal("expected a bad key to fail")
	}
}

func stubDelete(t *testing.T, fn func(rawURL string) (int, []byte, error)) func() {
	t.Helper()
	previous := DeleteJSON
	DeleteJSON = func(ctx context.Context, rawURL, user, token string) (int, []byte, error) {
		if ctx == nil || user == "" || token == "" {
			return 0, nil, context.Canceled
		}
		return fn(rawURL)
	}
	return func() { DeleteJSON = previous }
}

func stubPost(t *testing.T, fn func(rawURL string, payload []byte) (int, []byte, error)) func() {
	t.Helper()
	previous := PostJSON
	PostJSON = func(ctx context.Context, rawURL, user, token string, payload []byte) (int, []byte, error) {
		if ctx == nil || user == "" || token == "" {
			return 0, nil, context.Canceled
		}
		return fn(rawURL, payload)
	}
	return func() { PostJSON = previous }
}
