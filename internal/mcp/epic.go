package mcp

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
)

// Epic is a Jira epic created for one Kiki group.
type Epic struct {
	Key       string
	ID        string
	BoardID   string
	BoardName string
	URL       string
}

// PostJSON writes one Atlassian URL with basic auth. Tests replace it.
var PostJSON = func(ctx context.Context, rawURL, user, token string, payload []byte) (int, []byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, rawURL, bytes.NewReader(payload))
	if err != nil {
		return 0, nil, err
	}
	req.SetBasicAuth(user, token)
	req.Header.Set("Accept", "application/json")
	req.Header.Set("Content-Type", "application/json")
	client := &http.Client{Timeout: 20 * time.Second}
	res, err := client.Do(req)
	if err != nil {
		return 0, nil, err
	}
	defer res.Body.Close()
	body, err := io.ReadAll(io.LimitReader(res.Body, 1<<20))
	if err != nil {
		return res.StatusCode, nil, err
	}
	return res.StatusCode, body, nil
}

// DeleteJSON removes one Atlassian URL with basic auth. Tests replace it.
var DeleteJSON = func(ctx context.Context, rawURL, user, token string) (int, []byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodDelete, rawURL, nil)
	if err != nil {
		return 0, nil, err
	}
	req.SetBasicAuth(user, token)
	req.Header.Set("Accept", "application/json")
	client := &http.Client{Timeout: 20 * time.Second}
	res, err := client.Do(req)
	if err != nil {
		return 0, nil, err
	}
	defer res.Body.Close()
	body, err := io.ReadAll(io.LimitReader(res.Body, 1<<20))
	if err != nil {
		return res.StatusCode, nil, err
	}
	return res.StatusCode, body, nil
}

var issueKeyPattern = regexp.MustCompile(`^[A-Z][A-Z0-9]{1,9}-[0-9]+$`)

// DeleteIssue deletes one Jira issue. An issue Jira no longer has is already gone.
func DeleteIssue(ctx context.Context, cfg Atlassian, key string) error {
	key = strings.ToUpper(strings.TrimSpace(key))
	if !issueKeyPattern.MatchString(key) {
		return fmt.Errorf("that ticket is not a Jira issue")
	}
	origin, user, token, err := serviceLogin(cfg, cfg.Jira, "Jira")
	if err != nil {
		return err
	}
	status, body, err := DeleteJSON(ctx, origin+"/rest/api/3/issue/"+url.PathEscape(key), user, token)
	if err != nil {
		return fmt.Errorf("Jira ticket: %s", oneLine(err.Error()))
	}
	if status == http.StatusNotFound {
		return nil
	}
	if status == http.StatusUnauthorized || status == http.StatusForbidden {
		return fmt.Errorf("Jira ticket: Atlassian rejected the email or API token")
	}
	if status < 200 || status >= 300 {
		message := jiraMessage(body)
		if message == "" {
			message = http.StatusText(status)
		}
		return fmt.Errorf("Jira ticket: %s", message)
	}
	return nil
}

// CreateEpic creates an Epic on the Jira board saved in Settings.
// The summary is the group name. The description is the group description.
func CreateEpic(ctx context.Context, cfg Atlassian, summary, description string) (Epic, error) {
	origin, user, token, err := serviceLogin(cfg, cfg.Jira, "Jira")
	if err != nil {
		return Epic{}, err
	}
	boardID := strings.TrimSpace(cfg.Jira.BoardID)
	if boardID == "" {
		return Epic{}, fmt.Errorf("choose a Jira board in Settings before creating an epic")
	}
	summary = clipSummary(summary)
	if summary == "" {
		return Epic{}, fmt.Errorf("enter a task name")
	}
	description = strings.TrimSpace(description)
	projectKey, boardName, err := boardProject(ctx, origin, user, token, boardID)
	if err != nil {
		return Epic{}, err
	}
	if boardName == "" {
		boardName = strings.TrimSpace(cfg.Jira.BoardName)
	}
	fields, err := epicFields(ctx, origin, user, token, projectKey, summary, description)
	if err != nil {
		fields = fallbackEpicFields(projectKey, summary, description)
	}
	raw, err := json.Marshal(map[string]any{"fields": fields})
	if err != nil {
		return Epic{}, err
	}
	body, err := writeOK(ctx, origin+"/rest/api/3/issue", user, token, "Jira epic", raw)
	if err != nil {
		return Epic{}, err
	}
	var created struct {
		ID  string `json:"id"`
		Key string `json:"key"`
	}
	if err := json.Unmarshal(body, &created); err != nil {
		return Epic{}, fmt.Errorf("Jira epic: %s", oneLine(err.Error()))
	}
	created.Key = strings.ToUpper(strings.TrimSpace(created.Key))
	if created.Key == "" {
		return Epic{}, fmt.Errorf("Jira created an epic but did not return its key")
	}
	return Epic{
		Key:       created.Key,
		ID:        strings.TrimSpace(created.ID),
		BoardID:   boardID,
		BoardName: boardName,
		URL:       origin + "/browse/" + url.PathEscape(created.Key),
	}, nil
}

func boardProject(ctx context.Context, origin, user, token, boardID string) (string, string, error) {
	body, err := readOK(ctx, origin+"/rest/agile/1.0/board/"+url.PathEscape(boardID), user, token, "Jira board")
	if err != nil {
		return "", "", err
	}
	var board struct {
		Name     string `json:"name"`
		Location struct {
			ProjectKey string `json:"projectKey"`
		} `json:"location"`
	}
	if err := json.Unmarshal(body, &board); err != nil {
		return "", "", fmt.Errorf("Jira board: %s", oneLine(err.Error()))
	}
	projectKey := strings.ToUpper(strings.TrimSpace(board.Location.ProjectKey))
	if projectKey != "" {
		return projectKey, strings.TrimSpace(board.Name), nil
	}
	listed, err := readOK(ctx, origin+"/rest/agile/1.0/board/"+url.PathEscape(boardID)+"/project", user, token, "Jira board")
	if err != nil {
		return "", "", err
	}
	var projects struct {
		Values []struct {
			Key string `json:"key"`
		} `json:"values"`
	}
	if err := json.Unmarshal(listed, &projects); err != nil {
		return "", "", fmt.Errorf("Jira board: %s", oneLine(err.Error()))
	}
	for _, project := range projects.Values {
		projectKey = strings.ToUpper(strings.TrimSpace(project.Key))
		if projectKey != "" {
			return projectKey, strings.TrimSpace(board.Name), nil
		}
	}
	return "", "", fmt.Errorf("Jira board %s has no project", boardID)
}

func epicFields(ctx context.Context, origin, user, token, projectKey, summary, description string) (map[string]any, error) {
	body, err := readOK(ctx, origin+"/rest/api/3/issue/createmeta/"+url.PathEscape(projectKey)+"/issuetypes", user, token, "Jira epic")
	if err != nil {
		return nil, err
	}
	issueType, ok, err := pickEpicType(body)
	if err != nil {
		return nil, err
	}
	if !ok {
		return nil, fmt.Errorf("Jira project %s has no Epic issue type", projectKey)
	}
	fieldsBody, err := readOK(ctx, origin+"/rest/api/3/issue/createmeta/"+url.PathEscape(projectKey)+"/issuetypes/"+url.PathEscape(issueType.ID), user, token, "Jira epic")
	if err != nil {
		return nil, err
	}
	specs, err := decodeFields(fieldsBody)
	if err != nil {
		return nil, err
	}
	return epicFieldMap(projectKey, issueType.ID, summary, description, specs), nil
}

func fallbackEpicFields(projectKey, summary, description string) map[string]any {
	fields := map[string]any{
		"project":   map[string]string{"key": projectKey},
		"summary":   summary,
		"issuetype": map[string]string{"name": "Epic"},
	}
	if description != "" {
		fields["description"] = adfDoc(description)
	}
	return fields
}

type issueType struct {
	ID           string `json:"id"`
	Name         string `json:"name"`
	Untranslated string `json:"untranslatedName"`
	Hierarchy    int    `json:"hierarchyLevel"`
}

func pickEpicType(body []byte) (issueType, bool, error) {
	list, err := decodeIssueTypes(body)
	if err != nil {
		return issueType{}, false, err
	}
	var level issueType
	hasLevel := false
	for _, item := range list {
		item.ID = strings.TrimSpace(item.ID)
		if item.ID == "" {
			continue
		}
		if strings.EqualFold(strings.TrimSpace(item.Name), "Epic") || strings.EqualFold(strings.TrimSpace(item.Untranslated), "Epic") {
			return item, true, nil
		}
		if item.Hierarchy == 1 && !hasLevel {
			level = item
			hasLevel = true
		}
	}
	if hasLevel {
		return level, true, nil
	}
	return issueType{}, false, nil
}

func decodeIssueTypes(body []byte) ([]issueType, error) {
	var wrapped struct {
		IssueTypes []issueType `json:"issueTypes"`
		Values     []issueType `json:"values"`
	}
	if err := json.Unmarshal(body, &wrapped); err != nil {
		return nil, fmt.Errorf("Jira epic: %s", oneLine(err.Error()))
	}
	if len(wrapped.IssueTypes) > 0 {
		return wrapped.IssueTypes, nil
	}
	if len(wrapped.Values) > 0 {
		return wrapped.Values, nil
	}
	var list []issueType
	if err := json.Unmarshal(body, &list); err != nil || len(list) == 0 {
		return nil, fmt.Errorf("Jira did not list issue types")
	}
	return list, nil
}

type fieldSpec struct {
	FieldID  string
	Name     string
	Required bool
	Custom   string
	System   string
}

func decodeFields(body []byte) ([]fieldSpec, error) {
	var wrapped struct {
		Fields json.RawMessage `json:"fields"`
	}
	if err := json.Unmarshal(body, &wrapped); err != nil {
		return nil, fmt.Errorf("Jira epic: %s", oneLine(err.Error()))
	}
	if len(bytesTrim(wrapped.Fields)) == 0 || string(bytesTrim(wrapped.Fields)) == "null" {
		return nil, fmt.Errorf("Jira did not list epic fields")
	}
	raw := bytesTrim(wrapped.Fields)
	var list []rawField
	if raw[0] == '[' {
		if err := json.Unmarshal(raw, &list); err != nil {
			return nil, fmt.Errorf("Jira epic: %s", oneLine(err.Error()))
		}
	} else {
		var object map[string]rawField
		if err := json.Unmarshal(raw, &object); err != nil {
			return nil, fmt.Errorf("Jira epic: %s", oneLine(err.Error()))
		}
		list = make([]rawField, 0, len(object))
		for id, field := range object {
			if strings.TrimSpace(field.FieldID) == "" {
				field.FieldID = id
			}
			list = append(list, field)
		}
	}
	out := make([]fieldSpec, 0, len(list))
	for _, field := range list {
		out = append(out, fieldSpec{
			FieldID:  strings.TrimSpace(field.FieldID),
			Name:     strings.TrimSpace(field.Name),
			Required: field.Required,
			Custom:   field.Schema.Custom,
			System:   field.Schema.System,
		})
	}
	return out, nil
}

type rawField struct {
	FieldID  string `json:"fieldId"`
	Name     string `json:"name"`
	Required bool   `json:"required"`
	Schema   struct {
		System string `json:"system"`
		Custom string `json:"custom"`
	} `json:"schema"`
}

func epicFieldMap(projectKey, typeID, summary, description string, specs []fieldSpec) map[string]any {
	fields := map[string]any{
		"project":   map[string]string{"key": projectKey},
		"issuetype": map[string]string{"id": typeID},
		"summary":   summary,
	}
	for _, spec := range specs {
		switch spec.FieldID {
		case "", "project", "issuetype", "summary", "reporter", "assignee":
			continue
		}
		if epicNameField(spec) {
			fields[spec.FieldID] = summary
			continue
		}
		if spec.FieldID == "description" || spec.System == "description" {
			text := description
			if text == "" && spec.Required {
				text = summary
			}
			if text != "" {
				fields["description"] = adfDoc(text)
			}
		}
	}
	if description != "" {
		if _, ok := fields["description"]; !ok {
			fields["description"] = adfDoc(description)
		}
	}
	return fields
}

func epicNameField(spec fieldSpec) bool {
	if strings.EqualFold(spec.Name, "Epic Name") {
		return true
	}
	return strings.Contains(spec.Custom, "gh-epic-label")
}

func adfDoc(text string) map[string]any {
	text = strings.TrimSpace(text)
	if len(text) > 4000 {
		text = text[:4000]
	}
	return map[string]any{
		"type":    "doc",
		"version": 1,
		"content": []any{
			map[string]any{
				"type": "paragraph",
				"content": []any{
					map[string]any{"type": "text", "text": text},
				},
			},
		},
	}
}

func clipSummary(text string) string {
	text = strings.TrimSpace(text)
	if len(text) > 255 {
		text = text[:255]
	}
	return text
}

func writeOK(ctx context.Context, rawURL, user, token, label string, payload []byte) ([]byte, error) {
	status, body, err := PostJSON(ctx, rawURL, user, token, payload)
	if err != nil {
		return nil, fmt.Errorf("%s: %s", label, oneLine(err.Error()))
	}
	if status == http.StatusUnauthorized || status == http.StatusForbidden {
		return nil, fmt.Errorf("%s: Atlassian rejected the email or API token", label)
	}
	if status < 200 || status >= 300 {
		message := jiraMessage(body)
		if message == "" {
			message = http.StatusText(status)
		}
		return nil, fmt.Errorf("%s: %s", label, message)
	}
	return body, nil
}

func jiraMessage(body []byte) string {
	var payload struct {
		ErrorMessages []string          `json:"errorMessages"`
		Errors        map[string]string `json:"errors"`
		Message       string            `json:"message"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		return oneLine(string(body))
	}
	parts := make([]string, 0, len(payload.ErrorMessages)+len(payload.Errors)+1)
	parts = append(parts, payload.ErrorMessages...)
	for _, message := range payload.Errors {
		message = strings.TrimSpace(message)
		if message != "" {
			parts = append(parts, message)
		}
	}
	if strings.TrimSpace(payload.Message) != "" {
		parts = append(parts, payload.Message)
	}
	if len(parts) == 0 {
		return oneLine(string(body))
	}
	return oneLine(strings.Join(parts, " "))
}
