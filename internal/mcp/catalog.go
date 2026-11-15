package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Hit is one Jira board, Confluence space, or Confluence page.
type Hit struct {
	ID   string `json:"id"`
	Name string `json:"name"`
	Key  string `json:"key,omitempty"`
}

// GetJSON reads one Atlassian URL with basic auth. Tests replace it.
var GetJSON = func(ctx context.Context, rawURL, user, token string) (int, []byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, rawURL, nil)
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

// Boards lists Jira boards the saved token can see. query narrows the name.
func Boards(ctx context.Context, cfg Atlassian, query string) ([]Hit, error) {
	origin, user, token, err := serviceLogin(cfg, cfg.Jira, "Jira")
	if err != nil {
		return nil, err
	}
	endpoint, err := url.Parse(origin + "/rest/agile/1.0/board")
	if err != nil {
		return nil, err
	}
	values := endpoint.Query()
	values.Set("maxResults", "50")
	if text := clipQuery(query); text != "" {
		values.Set("name", text)
	}
	endpoint.RawQuery = values.Encode()
	body, err := readOK(ctx, endpoint.String(), user, token, "Jira boards")
	if err != nil {
		return nil, err
	}
	var payload struct {
		Values []struct {
			ID   flexID `json:"id"`
			Name string `json:"name"`
		} `json:"values"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		return nil, fmt.Errorf("Jira boards: %w", err)
	}
	out := make([]Hit, 0, len(payload.Values))
	for _, item := range payload.Values {
		id := strings.TrimSpace(item.ID.raw)
		name := strings.TrimSpace(item.Name)
		if id == "" || name == "" {
			continue
		}
		out = append(out, Hit{ID: id, Name: name})
	}
	return out, nil
}

// Spaces lists Confluence spaces. query keeps names and keys that contain it.
func Spaces(ctx context.Context, cfg Atlassian, query string) ([]Hit, error) {
	origin, user, token, err := serviceLogin(cfg, cfg.Confluence, "Confluence")
	if err != nil {
		return nil, err
	}
	next := origin + "/wiki/api/v2/spaces?limit=50&status=current"
	want := clipQuery(query)
	var out []Hit
	for page := 0; page < 4 && next != ""; page++ {
		body, err := readOK(ctx, next, user, token, "Confluence spaces")
		if err != nil {
			return nil, err
		}
		var payload struct {
			Results []struct {
				ID   flexID `json:"id"`
				Key  string `json:"key"`
				Name string `json:"name"`
			} `json:"results"`
			Links struct {
				Next string `json:"next"`
			} `json:"_links"`
		}
		if err := json.Unmarshal(body, &payload); err != nil {
			return nil, fmt.Errorf("Confluence spaces: %w", err)
		}
		for _, item := range payload.Results {
			hit := Hit{ID: strings.TrimSpace(item.ID.raw), Name: strings.TrimSpace(item.Name), Key: strings.TrimSpace(item.Key)}
			if hit.ID == "" || hit.Name == "" || !matches(want, hit.Name, hit.Key) {
				continue
			}
			out = append(out, hit)
			if len(out) == 50 {
				return out, nil
			}
		}
		if want == "" {
			return out, nil
		}
		next = joinNext(origin, payload.Links.Next)
	}
	return out, nil
}

// Pages lists pages in one Confluence space. query searches titles.
func Pages(ctx context.Context, cfg Atlassian, spaceID, spaceKey, query string) ([]Hit, error) {
	origin, user, token, err := serviceLogin(cfg, cfg.Confluence, "Confluence")
	if err != nil {
		return nil, err
	}
	spaceID = strings.TrimSpace(spaceID)
	spaceKey = strings.TrimSpace(spaceKey)
	if spaceID == "" && spaceKey == "" {
		return nil, fmt.Errorf("choose a Confluence space first")
	}
	query = clipQuery(query)
	if query != "" && spaceKey != "" {
		return searchPages(ctx, origin, user, token, spaceKey, query)
	}
	if spaceID == "" {
		return nil, fmt.Errorf("choose a Confluence space first")
	}
	endpoint, err := url.Parse(origin + "/wiki/api/v2/pages")
	if err != nil {
		return nil, err
	}
	values := endpoint.Query()
	values.Set("space-id", spaceID)
	values.Set("limit", "50")
	values.Set("status", "current")
	values.Set("sort", "title")
	endpoint.RawQuery = values.Encode()
	body, err := readOK(ctx, endpoint.String(), user, token, "Confluence pages")
	if err != nil {
		return nil, err
	}
	var payload struct {
		Results []struct {
			ID    flexID `json:"id"`
			Title string `json:"title"`
		} `json:"results"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		return nil, fmt.Errorf("Confluence pages: %w", err)
	}
	out := make([]Hit, 0, len(payload.Results))
	for _, item := range payload.Results {
		hit := Hit{ID: strings.TrimSpace(item.ID.raw), Name: strings.TrimSpace(item.Title)}
		if hit.ID == "" || hit.Name == "" || !matches(query, hit.Name) {
			continue
		}
		out = append(out, hit)
	}
	return out, nil
}

func searchPages(ctx context.Context, origin, user, token, spaceKey, query string) ([]Hit, error) {
	cql := fmt.Sprintf(`type=page AND space="%s" AND title~"%s"`, escapeCQL(spaceKey), escapeCQL(query))
	endpoint, err := url.Parse(origin + "/wiki/rest/api/content/search")
	if err != nil {
		return nil, err
	}
	values := endpoint.Query()
	values.Set("cql", cql)
	values.Set("limit", "25")
	endpoint.RawQuery = values.Encode()
	body, err := readOK(ctx, endpoint.String(), user, token, "Confluence pages")
	if err != nil {
		return nil, err
	}
	var payload struct {
		Results []struct {
			Title   string `json:"title"`
			Content struct {
				ID    flexID `json:"id"`
				Title string `json:"title"`
			} `json:"content"`
		} `json:"results"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		return nil, fmt.Errorf("Confluence pages: %w", err)
	}
	out := make([]Hit, 0, len(payload.Results))
	for _, item := range payload.Results {
		name := strings.TrimSpace(item.Content.Title)
		if name == "" {
			name = strings.TrimSpace(item.Title)
		}
		id := strings.TrimSpace(item.Content.ID.raw)
		if id == "" || name == "" {
			continue
		}
		out = append(out, Hit{ID: id, Name: name})
	}
	return out, nil
}

func serviceLogin(cfg Atlassian, service Service, name string) (origin, user, token string, err error) {
	cfg = cfg.Normalized()
	service = service.clean()
	if !cfg.Enabled || !service.Enabled {
		return "", "", "", fmt.Errorf("turn on %s in Settings", name)
	}
	user, token = cfg.Username, cfg.APIToken
	if service.Own {
		user, token = service.Username, service.APIToken
	}
	if !hasCredential(user, token) {
		return "", "", "", fmt.Errorf("set the Atlassian email and API token in Settings")
	}
	if service.URL == "" {
		return "", "", "", fmt.Errorf("%s site is required", name)
	}
	parsed, err := url.Parse(service.URL)
	if err != nil || parsed.Host == "" || (parsed.Scheme != "https" && parsed.Scheme != "http") {
		return "", "", "", fmt.Errorf("%s site must be an http or https URL", name)
	}
	return parsed.Scheme + "://" + parsed.Host, user, token, nil
}

func readOK(ctx context.Context, rawURL, user, token, label string) ([]byte, error) {
	status, body, err := GetJSON(ctx, rawURL, user, token)
	if err != nil {
		return nil, fmt.Errorf("%s: %s", label, oneLine(err.Error()))
	}
	if status == http.StatusUnauthorized || status == http.StatusForbidden {
		return nil, fmt.Errorf("%s: Atlassian rejected the email or API token", label)
	}
	if status < 200 || status >= 300 {
		message := oneLine(string(body))
		if message == "" {
			message = http.StatusText(status)
		}
		return nil, fmt.Errorf("%s: %s", label, message)
	}
	return body, nil
}

func matches(query string, fields ...string) bool {
	query = strings.ToLower(strings.TrimSpace(query))
	if query == "" {
		return true
	}
	for _, field := range fields {
		if strings.Contains(strings.ToLower(field), query) {
			return true
		}
	}
	return false
}

func clipQuery(query string) string {
	query = strings.TrimSpace(query)
	if len(query) > 80 {
		query = query[:80]
	}
	return query
}

func escapeCQL(text string) string {
	text = strings.ReplaceAll(text, `\`, "")
	text = strings.ReplaceAll(text, `"`, "")
	return text
}

func joinNext(origin, next string) string {
	next = strings.TrimSpace(next)
	if next == "" {
		return ""
	}
	if strings.HasPrefix(next, "http://") || strings.HasPrefix(next, "https://") {
		return next
	}
	if !strings.HasPrefix(next, "/") {
		next = "/" + next
	}
	return origin + next
}

func oneLine(text string) string {
	text = strings.TrimSpace(text)
	text = strings.ReplaceAll(text, "\n", " ")
	text = strings.ReplaceAll(text, "\r", " ")
	if len(text) > 180 {
		text = text[:180]
	}
	return text
}

type flexID struct {
	raw string
}

func (f *flexID) UnmarshalJSON(body []byte) error {
	body = bytesTrim(body)
	if len(body) == 0 || string(body) == "null" {
		f.raw = ""
		return nil
	}
	if body[0] == '"' {
		var text string
		if err := json.Unmarshal(body, &text); err != nil {
			return err
		}
		f.raw = text
		return nil
	}
	f.raw = string(body)
	return nil
}

func bytesTrim(body []byte) []byte {
	return []byte(strings.TrimSpace(string(body)))
}
