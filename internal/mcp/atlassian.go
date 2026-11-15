// Package mcp installs the Atlassian MCP server into an AI tool.
package mcp

import (
	"fmt"
	"net/url"
	"strings"

	"gopkg.in/yaml.v3"
)

// Service is Jira or Confluence inside the Atlassian MCP setup.
// Own means this service uses its own email and API token.
// Otherwise it uses the email and API token saved on Atlassian.
type Service struct {
	Enabled     bool   `yaml:"enabled,omitempty"`
	URL         string `yaml:"url,omitempty"`
	Own         bool   `yaml:"own_credentials,omitempty"`
	Username    string `yaml:"username,omitempty"`
	APIToken    string `yaml:"api_token,omitempty"`
	BoardID     string `yaml:"board_id,omitempty"`
	BoardName   string `yaml:"board_name,omitempty"`
	SpaceID     string `yaml:"space_id,omitempty"`
	SpaceKey    string `yaml:"space_key,omitempty"`
	SpaceName   string `yaml:"space_name,omitempty"`
	ParentID    string `yaml:"parent_id,omitempty"`
	ParentTitle string `yaml:"parent_title,omitempty"`
}

// Atlassian is the MCP setup saved for Jira and Confluence.
// The email and API token on Atlassian are shared. A service can override them.
type Atlassian struct {
	Enabled    bool    `yaml:"enabled"`
	Username   string  `yaml:"username,omitempty"`
	APIToken   string  `yaml:"api_token,omitempty"`
	Jira       Service `yaml:"jira,omitempty"`
	Confluence Service `yaml:"confluence,omitempty"`
}

// Empty reports that nothing is worth storing.
func (a Atlassian) Empty() bool {
	a = a.Normalized()
	return !a.Enabled && a.Username == "" && a.APIToken == "" && a.Jira.cleared() && a.Confluence.cleared()
}

// Ready reports that a run should install Atlassian MCP.
func (a Atlassian) Ready() bool {
	a = a.Normalized()
	return a.Enabled && (a.Jira.Enabled || a.Confluence.Enabled)
}

// Normalized trims fields and drops a trailing slash from each site URL.
func (a Atlassian) Normalized() Atlassian {
	a.Username = strings.TrimSpace(a.Username)
	a.APIToken = strings.TrimSpace(a.APIToken)
	a.Jira = a.Jira.clean()
	a.Confluence = a.Confluence.clean()
	return a
}

// Validate checks a setup that will be stored or written into an AI tool.
// A setup that is off, or completely empty, is valid.
func (a Atlassian) Validate() error {
	a = a.Normalized()
	if !a.Enabled {
		return nil
	}
	if !a.Jira.Enabled && !a.Confluence.Enabled {
		return fmt.Errorf("turn on Jira or Confluence")
	}
	sharedNeeded := (a.Jira.Enabled && !a.Jira.Own) || (a.Confluence.Enabled && !a.Confluence.Own)
	if sharedNeeded && !hasCredential(a.Username, a.APIToken) {
		return fmt.Errorf("Atlassian needs an email and an API token")
	}
	if err := a.Jira.validate("Jira"); err != nil {
		return err
	}
	return a.Confluence.validate("Confluence")
}

// Apply merges a settings form into the saved setup.
// A blank token keeps the saved token. A form left entirely blank clears the setup.
func Apply(prev, form Atlassian) Atlassian {
	prev = prev.Normalized()
	form = form.Normalized()
	if form.Empty() {
		return Atlassian{}
	}
	next := form
	if next.APIToken == "" {
		next.APIToken = prev.APIToken
	}
	next.Jira = applyService(prev.Jira, form.Jira)
	next.Confluence = applyService(prev.Confluence, form.Confluence)
	return next
}

// Env is the environment mcp-atlassian reads. Empty fields are left out.
// A service that is off is left out. A service without its own email and API token
// uses the shared ones.
func (a Atlassian) Env() map[string]string {
	a = a.Normalized()
	if !a.Ready() {
		return map[string]string{}
	}
	out := map[string]string{}
	if a.Jira.Enabled {
		for key, value := range a.serviceEnv("JIRA", a.Jira) {
			out[key] = value
		}
	}
	if a.Confluence.Enabled {
		for key, value := range a.serviceEnv("CONFLUENCE", a.Confluence) {
			out[key] = value
		}
	}
	return out
}

// UnmarshalYAML reads the current setup and older files that stored credentials on each product.
func (a *Atlassian) UnmarshalYAML(value *yaml.Node) error {
	var raw struct {
		Enabled    *bool       `yaml:"enabled"`
		Username   string      `yaml:"username"`
		APIToken   string      `yaml:"api_token"`
		Jira       *serviceRaw `yaml:"jira"`
		Confluence *serviceRaw `yaml:"confluence"`
		Same       *bool       `yaml:"confluence_same_as_jira"`
	}
	if err := value.Decode(&raw); err != nil {
		return err
	}
	if raw.Enabled != nil {
		*a = Atlassian{
			Enabled:    *raw.Enabled,
			Username:   raw.Username,
			APIToken:   raw.APIToken,
			Jira:       raw.Jira.service(),
			Confluence: raw.Confluence.service(),
		}.Normalized()
		return nil
	}
	jira := legacy(raw.Jira)
	wiki := legacy(raw.Confluence)
	same := raw.Same != nil && *raw.Same
	sharedUser, sharedAPI := jira.Username, jira.APIToken
	wikiOwn := !same && wiki.hasCredential() && !sameCredential(jira, wiki)
	if !hasCredential(sharedUser, sharedAPI) {
		sharedUser, sharedAPI = wiki.Username, wiki.APIToken
		wikiOwn = false
	}
	confluence := Service{Enabled: wiki.present(), URL: wiki.URL}
	if wikiOwn {
		confluence.Own = true
		confluence.Username = wiki.Username
		confluence.APIToken = wiki.APIToken
	}
	*a = Atlassian{
		Enabled:    jira.present() || wiki.present(),
		Username:   sharedUser,
		APIToken:   sharedAPI,
		Jira:       Service{Enabled: jira.present(), URL: jira.URL},
		Confluence: confluence,
	}.Normalized()
	return nil
}

func applyService(prev, form Service) Service {
	next := form
	if next.APIToken == "" {
		next.APIToken = prev.APIToken
	}
	return next
}

func (s Service) clean() Service {
	s.URL = cleanURL(s.URL)
	s.Username = strings.TrimSpace(s.Username)
	s.APIToken = strings.TrimSpace(s.APIToken)
	s.BoardID = strings.TrimSpace(s.BoardID)
	s.BoardName = strings.TrimSpace(s.BoardName)
	s.SpaceID = strings.TrimSpace(s.SpaceID)
	s.SpaceKey = strings.TrimSpace(s.SpaceKey)
	s.SpaceName = strings.TrimSpace(s.SpaceName)
	s.ParentID = strings.TrimSpace(s.ParentID)
	s.ParentTitle = strings.TrimSpace(s.ParentTitle)
	return s
}

func (s Service) cleared() bool {
	return !s.Enabled && !s.Own && s.URL == "" && s.Username == "" && s.APIToken == "" &&
		s.BoardID == "" && s.BoardName == "" && s.SpaceID == "" && s.SpaceKey == "" && s.SpaceName == "" &&
		s.ParentID == "" && s.ParentTitle == ""
}

func (s Service) validate(name string) error {
	if !s.Enabled {
		return nil
	}
	if s.URL == "" {
		return fmt.Errorf("%s site is required", name)
	}
	if err := validateSite(s.URL); err != nil {
		return fmt.Errorf("%s site %s", name, err.Error())
	}
	if s.Own && !hasCredential(s.Username, s.APIToken) {
		return fmt.Errorf("%s needs an email and an API token", name)
	}
	return nil
}

func (a Atlassian) serviceEnv(prefix string, service Service) map[string]string {
	user, api := a.Username, a.APIToken
	if service.Own {
		user, api = service.Username, service.APIToken
	}
	out := map[string]string{}
	if service.URL != "" {
		out[prefix+"_URL"] = service.URL
	}
	if user != "" {
		out[prefix+"_USERNAME"] = user
	}
	if api != "" {
		out[prefix+"_API_TOKEN"] = api
	}
	return out
}

func hasCredential(username, api string) bool {
	return username != "" && api != ""
}

func cleanURL(raw string) string {
	raw = strings.TrimSpace(raw)
	for strings.HasSuffix(raw, "/") {
		raw = strings.TrimSuffix(raw, "/")
	}
	return raw
}

func validateSite(raw string) error {
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Host == "" || (parsed.Scheme != "https" && parsed.Scheme != "http") || parsed.User != nil {
		return fmt.Errorf("must be an http or https URL")
	}
	return nil
}

type serviceRaw struct {
	Enabled     *bool  `yaml:"enabled"`
	URL         string `yaml:"url"`
	Own         *bool  `yaml:"own_credentials"`
	Username    string `yaml:"username"`
	APIToken    string `yaml:"api_token"`
	BoardID     string `yaml:"board_id"`
	BoardName   string `yaml:"board_name"`
	SpaceID     string `yaml:"space_id"`
	SpaceKey    string `yaml:"space_key"`
	SpaceName   string `yaml:"space_name"`
	ParentID    string `yaml:"parent_id"`
	ParentTitle string `yaml:"parent_title"`
}

func (s *serviceRaw) service() Service {
	if s == nil {
		return Service{}
	}
	out := Service{
		URL:         s.URL,
		Username:    s.Username,
		APIToken:    s.APIToken,
		BoardID:     s.BoardID,
		BoardName:   s.BoardName,
		SpaceID:     s.SpaceID,
		SpaceKey:    s.SpaceKey,
		SpaceName:   s.SpaceName,
		ParentID:    s.ParentID,
		ParentTitle: s.ParentTitle,
	}
	if s.Enabled != nil {
		out.Enabled = *s.Enabled
	}
	if s.Own != nil {
		out.Own = *s.Own
	}
	return out
}

func legacy(s *serviceRaw) Service {
	if s == nil {
		return Service{}
	}
	return Service{
		URL:      s.URL,
		Username: s.Username,
		APIToken: s.APIToken,
	}
}

func (s Service) present() bool {
	return s.URL != "" || s.hasCredential()
}

func (s Service) hasCredential() bool {
	return hasCredential(s.Username, s.APIToken)
}

func sameCredential(a, b Service) bool {
	return a.Username == b.Username && a.APIToken == b.APIToken
}
