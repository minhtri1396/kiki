const state = { data: null, agentId: null, editingAgentId: "", busySetup: "", busyKind: "", pendingRepoIndex: null, checkingIndexes: new Set(), tab: "agents", workflowSelection: null, indexChoice: {} };
let pageScrollLock = null;

function lockPageScroll() {
  if (pageScrollLock != null) return;
  pageScrollLock = window.scrollY;
  const gap = window.innerWidth - document.documentElement.clientWidth;
  document.body.style.position = "fixed";
  document.body.style.top = `-${pageScrollLock}px`;
  document.body.style.left = "0";
  document.body.style.right = "0";
  document.body.style.width = "100%";
  if (gap > 0) document.body.style.paddingRight = `${gap}px`;
}

function unlockPageScroll() {
  if (pageScrollLock == null || document.querySelector("dialog[open]")) return;
  const y = pageScrollLock;
  pageScrollLock = null;
  document.body.style.position = "";
  document.body.style.top = "";
  document.body.style.left = "";
  document.body.style.right = "";
  document.body.style.width = "";
  document.body.style.paddingRight = "";
  window.scrollTo(0, y);
}

function syncPageScroll() {
  if (document.querySelector("dialog[open]")) lockPageScroll();
  else unlockPageScroll();
}

document.addEventListener("toggle", (event) => {
  if (event.target instanceof HTMLDialogElement) syncPageScroll();
}, true);

const dialogShowModal = HTMLDialogElement.prototype.showModal;
HTMLDialogElement.prototype.showModal = function () {
  dialogShowModal.call(this);
  syncPageScroll();
};
const dialogClose = HTMLDialogElement.prototype.close;
HTMLDialogElement.prototype.close = function () {
  dialogClose.apply(this, arguments);
  syncPageScroll();
};
document.addEventListener(
  "cancel",
  (event) => {
    if (event.target instanceof HTMLDialogElement) event.preventDefault();
  },
  true,
);
let teamMemberLoad = 0;
let teamShare = null;
let teamStage = 0;
let teamListedPath = "";
let teamListedMembers = [];
let agentMemberBusy = false;
let agentMemberChoice = null;
let agentMemberTeam = "";

const repoLabel = { backend: "Backend", website: "Website", mobile: "Mobile", document: "Document" };
const indexToolURL = {
  obsidian: "https://obsidian.md/help/cli",
  codegraph: "https://github.com/colbymchenry/codegraph",
};

function orchestratorChoices() {
  const items = state.data && state.data.orchestrators;
  return Array.isArray(items) && items.length ? items : [{ id: "orca", label: "Orca" }];
}

function orchestratorId() {
  return (state.data && state.data.orchestrator) || "orca";
}

function orchestratorLabel() {
  const id = orchestratorId();
  const found = orchestratorChoices().find((item) => item.id === id);
  return (found && found.label) || id;
}

function toolLabel(name) {
  name = name || "";
  if (!name) return "";
  return name.charAt(0).toUpperCase() + name.slice(1);
}
const repoKinds = ["document", "backend", "website", "mobile"];
const workspacePattern = /^[a-z][a-z0-9_-]{0,31}$/;
const settingsFieldRequiredMsg = "This field needs information.";
const themeKey = "kiki-theme";

function savedTheme() {
  try {
    const value = localStorage.getItem(themeKey);
    if (value === "light" || value === "dark" || value === "system") return value;
  } catch (error) {}
  const set = document.documentElement.dataset.theme;
  if (set === "light" || set === "dark") return set;
  return "system";
}

function applyTheme(theme) {
  const next = theme === "light" || theme === "dark" ? theme : "system";
  if (next === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem(themeKey, next);
  } catch (error) {}
}

applyTheme(savedTheme());

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

function bannerIcon() {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 20 20");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("timer");
  for (const name of ["timer-track", "timer-value"]) {
    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    circle.setAttribute("class", name);
    circle.setAttribute("cx", "10");
    circle.setAttribute("cy", "10");
    circle.setAttribute("r", "8");
    svg.append(circle);
  }
  return svg;
}

function banner(message, isError) {
  if (!message) return;
  const stack = document.querySelector("#banners");
  const node = el("p", "banner" + (isError ? " error" : ""));
  node.setAttribute("role", "status");
  node.append(el("span", "", message), bannerIcon());
  stack.prepend(node);
  void node.offsetWidth;
  node.classList.add("show");
  setTimeout(() => node.remove(), 3000);
}

async function api(url, options) {
  const res = await fetch(url, options);
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    const error = new Error(data.error || res.statusText);
    error.code = data.code;
    error.payload = data;
    throw error;
  }
  return data;
}

function groups(agents) {
  const order = [];
  const map = new Map();
  for (const agent of agents) {
    if (!map.has(agent.group)) {
      map.set(agent.group, []);
      order.push(agent.group);
    }
    map.get(agent.group).push(agent);
  }
  return order.map((name) => ({ name, agents: map.get(name) }));
}

function categoryLabel(name) {
  const text = String(name || "").trim();
  if (!text) return "Agents";
  return text.slice(0, 1).toUpperCase() + text.slice(1);
}

function render(data) {
  state.data = data;
  document.querySelector("#root-path").textContent = data.root;
  unlockNavigation();
  renderAgents(data);
  renderRepos(data);
  renderConfig(data);
  renderWorkflow(data);
  renderHistory(data);
  syncSave();
  paintGenerateIcons();
}

function renderAgentSource(team) {
  showTeamShareList();
  const card = el("button", "team-share-card");
  card.type = "button";
  card.dataset.openTeamShare = "true";
  const copy = el("div", "team-share-copy");
  const heading = el("h2", "", memberInUseTitle(team.member));
  heading.id = "team-share-title";
  copy.append(heading);
  copy.append(el("p", "hint", "Share this app's agents with the team. The team folder keeps a folder of agents for you and for each member."));
  const badge = el("span", "badge", "");
  badge.id = "team-share-badge";
  badge.hidden = true;
  card.append(copy, badge);

  const body = document.querySelector("#team-share-body");
  if (!body) return card;
  body.replaceChildren();
  body.append(el("p", "hint", "Copy agents between this app and the team folder. Your own folder can go both ways. Another member's folder only copies into this app."));
  const picker = el("div", "team-share");
  const title = el("div", "team-share-title");
  title.append(el("h3", "", "Member"));
  picker.append(title);
  const note = el("p", "hint", "Loading members…");
  note.id = "agent-member-note";
  picker.append(note);
  const select = el("select");
  select.id = "agent-member";
  select.setAttribute("aria-label", "Member");
  select.disabled = true;
  const waiting = el("option", "", "Loading members…");
  waiting.value = "";
  select.append(waiting);
  picker.append(select);
  body.append(picker);
  body.append(renderCompare(
    "user-compare",
    "This app and " + (team.user || "your folder"),
    true,
    "Your folder in the team folder. Use → to copy this app's agents into that folder, or ← to copy that folder's agents into this app. The arrows enable only when the two sides differ."
  ));
  const memberCompare = renderCompare(
    "member-compare",
    "This app and a member",
    false,
    "Another member's folder. Choose a member above, then use ← to copy that member's agents into this app. The arrow enables only when the two sides differ. Choosing a member asks before replacing this app's agents."
  );
  if (chosenMember(team) === String(team.user || "").trim()) memberCompare.hidden = true;
  body.append(memberCompare);
  return card;
}

function renderCompare(id, title, push, description) {
  const block = el("section", "team-compare-block");
  block.id = id;
  const head = el("div", "team-share-title");
  head.append(el("h3", "", title));
  const badge = el("span", "badge", "");
  badge.hidden = true;
  head.append(badge);
  block.append(head);
  if (description) block.append(el("p", "hint", description));
  const row = el("div", "team-compare");
  row.append(agentColumn("This app's agents"), arrowColumn(push !== false), agentColumn("Folder agents"));
  block.append(row);
  return block;
}

function agentColumn(heading) {
  const view = el("div", "team-view");
  view.append(el("h4", "", heading));
  view.append(el("ul"));
  return view;
}

function arrowColumn(push) {
  const arrows = el("div", "team-arrows");
  if (push) arrows.append(arrowButton("→", "push"));
  arrows.append(arrowButton("←", "pull"));
  return arrows;
}

function arrowButton(glyph, direction) {
  const button = el("button", "text-button", glyph);
  button.type = "button";
  button.disabled = true;
  button.dataset.sync = direction;
  button.setAttribute("aria-label", glyph === "→" ? "Copy this app's agents to the folder" : "Copy the folder's agents to this app");
  return button;
}

function memberInUseTitle(name) {
  const member = String(name || "").trim();
  if (!member) return "Team Sharing";
  return "Team Sharing - using " + member + " folder";
}

function paintMemberInUse(name) {
  const title = document.querySelector("#team-share-title");
  if (!title) return;
  title.textContent = memberInUseTitle(name);
}

function chosenMember(team) {
  const key = String(team.path || "").trim() + "\0" + String(team.user || "").trim();
  if (key !== agentMemberTeam) {
    agentMemberTeam = key;
    agentMemberChoice = null;
  }
  return agentMemberChoice == null ? (team.member || "") : agentMemberChoice;
}

function renderAgents(data) {
  const panel = document.querySelector("#panel-agents");
  panel.replaceChildren();
  const team = data.teamFolder || {};
  if (String(team.path || "").trim() && String(team.user || "").trim()) {
    panel.append(renderAgentSource(team));
    loadAgentMembers(chosenMember(team));
  }
  for (const group of groups(data.agents)) {
    const section = el("section", "group");
    section.append(el("h2", "", categoryLabel(group.name)));
    const cards = el("div", "cards");
    for (const agent of group.agents) {
      const card = el("article", "card agent-card" + (agent.enabled === false ? " is-off" : ""));
      const head = el("div", "card-head");
      const title = el("div", "card-title");
      title.append(el("h3", "", agent.title));
      if (agent.enabled === false) title.append(el("span", "badge bad", "Off"));
      for (const mark of mcpOverrideLabels(agent)) {
        const badge = el("span", "badge warn", mark.text);
        badge.title = mark.title;
        title.append(badge);
      }
      head.append(title);
      const actions = el("div", "card-actions");
      const edit = el("button", "text-button", "Edit");
      edit.type = "button";
      edit.dataset.editAgent = agent.id;
      const prompt = el("button", "icon-button");
      prompt.type = "button";
      prompt.dataset.prompt = agent.id;
      prompt.setAttribute("aria-label", "Show the prompt for " + agent.title);
      prompt.append(promptIcon());
      actions.append(edit, prompt);
      head.append(actions);
      card.append(head);
      card.append(el("p", "id", "ID: " + agent.id));
      const body = el("div", "card-copy");
      body.append(el("p", "summary", agent.summary));
      const repos = agent.repos.map((kind) => repoLabel[kind] || kind).join(", ");
      body.append(el("p", "hint", "Repository: " + repos));
      const peers = agent.worksWith || [];
      if (peers.length) {
        const names = peers.map((peerId) => {
          const other = state.data.agents.find((item) => item.id === peerId);
          return other ? other.title : peerId;
        });
        body.append(el("p", "hint", "Agents Coordination: " + names.join(", ")));
      }
      card.append(body, agentPicks(agent.id));
      cards.append(card);
    }
    section.append(cards);
    panel.append(section);
  }
  const history = el("section", "history");
  history.id = "history";
  panel.append(history);
}

function renderHistory(data) {
  const history = document.querySelector("#history");
  if (!history) return;
  const launches = data.launches || [];
  const head = el("div", "history-head");
  head.append(el("h2", "", "Recent runs"));
  if (launches.length) {
    const clear = el("button", "danger", "Clear");
    clear.type = "button";
    clear.dataset.clearRuns = "true";
    head.append(clear);
  }
  history.replaceChildren(head);
  const list = el("ul");
  if (!launches.length) {
    list.append(el("li", "hint", "No agent has been started."));
  }
  for (const item of launches) {
    const row = el("li");
    const open = el("button", "run-open");
    open.type = "button";
    open.dataset.run = String(item.id);
    open.append(el("span", "id", item.agentId));
    const note = el("span", "note", runPreview(item.note));
    open.append(note);
    open.append(el("span", "when", runTime(item.createdAt)));
    open.append(el("span", "badge " + (item.status === "ok" ? "ok" : "bad"), item.status));
    row.append(open);
    const remove = el("button", "danger run-delete", "Delete");
    remove.type = "button";
    remove.dataset.deleteRun = String(item.id);
    row.append(remove);
    list.append(row);
  }
  history.append(list);
}

function runPreview(note) {
  const text = String(note || "").replace(/\s+/g, " ").trim();
  return text || "No message";
}

function runTime(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "";
  return date.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function formatDetail(text) {
  const trimmed = String(text || "").trim();
  if (!trimmed) return "No detail.";
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch (error) {
    return String(text);
  }
}

function openRun(id) {
  const item = ((state.data && state.data.launches) || []).find((row) => String(row.id) === String(id));
  if (!item) return;
  const dialog = document.querySelector("#run-dialog");
  dialog.dataset.id = String(item.id);
  document.querySelector("#run-meta").textContent =
    item.agentId + " · " + (item.runtime || "") + " · " + item.status + " · " + runTime(item.createdAt);
  document.querySelector("#run-note").textContent = item.note || "No message";
  document.querySelector("#run-detail").textContent = formatDetail(item.detail);
  dialog.showModal();
}

async function reloadHistory() {
  const fresh = await api("/api/state");
  state.data = fresh;
  renderHistory(fresh);
}

async function deleteRun(id) {
  await api("/api/launches/" + encodeURIComponent(id), { method: "DELETE" });
  const dialog = document.querySelector("#run-dialog");
  if (dialog.open && dialog.dataset.id === String(id)) dialog.close();
  await reloadHistory();
  banner("Deleted the run.");
}

async function clearRuns() {
  await api("/api/launches", { method: "DELETE" });
  const dialog = document.querySelector("#run-dialog");
  if (dialog.open) dialog.close();
  await reloadHistory();
  banner("Cleared runs.");
}

function renderRepos(data) {
  const panel = document.querySelector("#panel-repos");
  panel.replaceChildren();
  panel.append(el("p", "hint", "Each repository has an index and a folder. Turn the index on to use Obsidian or CodeGraph. A tool already in the folder is used as it is. Choose a folder and KIKI uses that path. Clone into it when the folder is empty. KIKI saves each repository in " + data.root + "/.kiki/repos."));

  const grid = el("div", "repo-grid");
  const rank = (kind) => {
    const index = repoKinds.indexOf(kind);
    return index === -1 ? repoKinds.length : index;
  };
  const repos = data.repos.slice().sort((a, b) => rank(a.kind) - rank(b.kind));
  for (const repo of repos) {
    const card = el("article", "card");
    const head = el("div", "repo-head");
    head.append(el("h3", "", repoLabel[repo.kind] || repo.kind));
    card.append(head, repoFolderSection(repo), repoIndexSection(repo));
    grid.append(card);
  }
  panel.append(grid);
  panel.append(saveButton());
  if (state.checkingIndexes.size) markIndexesChecking();
  if (state.busySetup) paintRepoSetupButtons();
}

function renderConfig(data) {
  document.querySelector("#settings-hint").textContent =
    "Press Save to write Workspace, Agent Orchestration, Generate text, and Team folder to " + data.setupFile + ". MCP credentials are saved in mcp.yaml beside that file. Tokens stay there and are not written into the setup file agents read. Tools are saved in tools.yaml beside that file. Appearance applies immediately and stays on this computer. Close discards other unsaved changes.";
  const settings = el("div", "setup-row");
  settings.append(renderAppearance());
  settings.append(field("Workspace", "text", "workspace-id", data.workspaceId));
  settings.append(renderOrchestration(data));
  settings.append(renderGenerate(data));
  settings.append(renderTeam(data.teamFolder || {}));
  settings.append(renderGuardrails(data.guardrails !== false));
  settings.append(renderMCP(data.atlassian || {}));
  settings.append(renderTools(data.toolChoices || []));
  document.querySelector("#settings-fields").replaceChildren(settings);
}

function renderAppearance() {
  const section = el("section", "appearance-setup");
  const wrap = el("label", "field");
  wrap.append(el("span", "", "Appearance"));
  const select = el("select");
  select.id = "appearance";
  const current = savedTheme();
  for (const [id, label] of [["system", "System"], ["light", "Light"], ["dark", "Dark"]]) {
    const option = el("option", "", label);
    option.value = id;
    if (id === current) option.selected = true;
    select.append(option);
  }
  wrap.append(select);
  wrap.append(el("p", "hint", "Light, dark, or match this computer."));
  section.append(wrap);
  return section;
}

function renderOrchestration(data) {
  const wrap = el("label", "field wide");
  wrap.append(el("span", "", "Agent Orchestration"));
  const select = el("select");
  select.id = "orchestrator";
  const items = Array.isArray(data.orchestrators) && data.orchestrators.length
    ? data.orchestrators
    : [{ id: "orca", label: "Orca" }];
  const selected = data.orchestrator || "orca";
  for (const item of items) {
    const option = el("option", "", item.label || item.id);
    option.value = item.id;
    if (item.id === selected) option.selected = true;
    select.append(option);
  }
  if (![...select.options].some((option) => option.selected) && select.options[0]) {
    select.options[0].selected = true;
  }
  wrap.append(select);
  return wrap;
}

function renderGenerate(data) {
  const section = el("section", "generate-setup");
  section.append(el("h3", "", "Generate text"));
  section.append(el("p", "hint", "AI Tool and model used when Generate rewrites a prompt, a repository note, an Agents Coordination note, an MCP note, or a tool note in Edit agent. KIKI runs that tool's CLI."));
  const grid = el("div", "agent-grid");
  const toolField = el("label", "field");
  toolField.append(el("span", "", "AI Tool"));
  const tool = el("select");
  tool.id = "generate-tool";
  const toolName = usableTool(data.generateTool);
  fillToolSelect(tool, toolName);
  tool.dataset.saved = tool.value;
  toolField.append(tool);
  const modelField = el("label", "field");
  modelField.append(el("span", "", "Model"));
  const model = el("select");
  model.id = "generate-model";
  const modelName = data.generateModel || defaultModel(toolName);
  fillModels(model, toolName, modelName);
  if (modelName && ![...model.options].some((option) => option.value === modelName)) {
    const option = el("option", "", modelName);
    option.value = modelName;
    option.selected = true;
    model.append(option);
  }
  model.dataset.saved = model.value;
  modelField.append(model);
  grid.append(toolField, modelField);
  section.append(grid);
  return section;
}

function teamNameMode() {
  const picked = document.querySelector("input[name='team-name-mode']:checked");
  return picked ? picked.value : "new";
}

function currentTeamUser() {
  if (teamNameMode() === "existing") {
    const select = document.querySelector("#team-member-name");
    return select ? select.value.trim() : "";
  }
  const user = document.querySelector("#team-user");
  return user ? user.value.trim() : "";
}

function teamPathReady(path) {
  const value = String(path || "").trim();
  return value !== "" && value === teamListedPath;
}

function teamChoicesApply(path) {
  return teamPathReady(path) && teamListedMembers.length > 0;
}

function savedTeamForMount(overrides) {
  const saved = (state.data && state.data.teamFolder) || {};
  const pathInput = document.querySelector("#team-path");
  return Object.assign({
    path: pathInput ? pathInput.value : (saved.path || ""),
    user: saved.user || "",
    memberName: saved.user || "",
  }, overrides || {});
}

function renderGuardrails(on) {
  const section = el("section", "tools-setup wide");
  section.append(el("h3", "", "Guardrails"));
  section.append(el("p", "hint", "Guardrails apply to every agent when they are on. Starting an agent adds the guardrail hook to that AI tool when it is missing. Turning guardrails off removes those hooks from Cursor, Claude, Codex, and Gemini. Save writes this choice to tools.yaml beside the setup file."));
  const card = el("div", "mcp-card");
  const head = el("div", "mcp-head");
  head.append(el("strong", "", "Guardrails"));
  const button = el("button", "switch");
  button.type = "button";
  button.id = "guardrails-toggle";
  button.dataset.guardrailsToggle = "1";
  button.setAttribute("aria-pressed", on ? "true" : "false");
  button.setAttribute("aria-label", "Use Guardrails");
  button.append(el("span", "switch-knob"));
  head.append(button);
  card.append(head);
  card.append(el("p", "hint", "The hook checks each command, file, MCP server, and agent before it runs."));
  section.append(card);
  return section;
}

function renderTools(choices) {
  const section = el("section", "tools-setup wide");
  section.append(el("h3", "", "Tools"));
  section.append(el("p", "hint", "GitHub CLI is chosen on each agent. RTK applies to every agent when it is on. Starting an agent adds the RTK hook to that AI tool when it is missing. Turning RTK off removes the RTK hook from Cursor, Claude, Codex, and Gemini. Save writes these choices to tools.yaml beside the setup file. Agents do not read that file."));
  for (const tool of choices) section.append(renderToolCard(tool));
  return section;
}

function renderToolCard(tool) {
  const card = el("div", "mcp-card");
  const head = el("div", "mcp-head");
  const title = el("div", "tool-title");
  title.append(el("strong", "", tool.name || tool.id));
  if (tool.global) title.append(el("span", "badge", "Every agent"));
  const badge = el("span", tool.installed ? "badge ok" : "badge wait", tool.installed ? "Installed" : "Not installed");
  title.append(badge);
  head.append(title, toolSwitch(tool));
  card.append(head);
  const hint = el("p", "hint");
  if (tool.detail) hint.append(document.createTextNode(tool.detail + " "));
  if (tool.command) {
    hint.append(document.createTextNode("Command: "));
    hint.append(el("code", "", tool.command));
    hint.append(document.createTextNode(". "));
  }
  if (tool.page) {
    const link = document.createElement("a");
    link.className = "index-tool-link";
    link.href = tool.page;
    link.dataset.url = tool.page;
    link.textContent = tool.page;
    hint.append(link);
  }
  card.append(hint);
  return card;
}

function toolSwitch(tool) {
  const button = el("button", "switch");
  button.type = "button";
  button.id = "tool-" + tool.id;
  button.dataset.toolToggle = tool.id;
  button.setAttribute("aria-pressed", tool.enabled ? "true" : "false");
  button.setAttribute("aria-label", "Use " + (tool.name || tool.id));
  button.append(el("span", "switch-knob"));
  return button;
}

function collectTools() {
  const buttons = document.querySelectorAll("#settings-fields [data-tool-toggle]");
  const guard = document.querySelector("#settings-fields [data-guardrails-toggle]");
  if (!buttons.length && !guard) return null;
  const tools = {};
  for (const button of buttons) tools[button.dataset.toolToggle] = button.getAttribute("aria-pressed") === "true";
  if (guard) tools.guardrails = guard.getAttribute("aria-pressed") === "true";
  return tools;
}

function toolsDirty() {
  const choices = (state.data && state.data.toolChoices) || [];
  for (const tool of choices) {
    const button = document.querySelector("#settings-fields [data-tool-toggle='" + CSS.escape(tool.id) + "']");
    if (!button) return false;
    if ((button.getAttribute("aria-pressed") === "true") !== !!tool.enabled) return true;
  }
  return guardrailsDirty();
}

function guardrailsDirty() {
  const button = document.querySelector("#settings-fields [data-guardrails-toggle]");
  if (!button || !state.data) return false;
  return (button.getAttribute("aria-pressed") === "true") !== (state.data.guardrails !== false);
}

function renderMCP(cfg) {
  const section = el("section", "mcp-setup wide");
  section.append(el("h3", "", "MCP Servers"));
  section.append(el("p", "hint", "Before a run, KIKI adds an MCP server to the agent's AI Tool when that tool does not already have one. A token left blank keeps the saved token."));
  section.append(renderAtlassianMCP(cfg));
  syncMCP(section);
  return section;
}

function renderAtlassianMCP(cfg) {
  const card = el("div", "mcp-card");
  const head = el("div", "mcp-head");
  head.append(el("strong", "", "Atlassian"));
  head.append(mcpSwitch("mcp-atlassian", "Use Atlassian", !!cfg.enabled));
  card.append(head);
  const body = el("div", "mcp-body");
  body.id = "mcp-atlassian-body";
  body.append(atlassianTokenHint());
  body.append(...mcpCredentialFields("mcp", cfg, "you@company.com"));
  rememberSettingsPlaces(cfg);
  body.append(mcpService("Jira", "jira", cfg.jira || {}, "https://your-company.atlassian.net"));
  body.append(mcpService("Confluence", "confluence", cfg.confluence || {}, "https://your-company.atlassian.net/wiki"));
  card.append(body);
  return card;
}

function mcpSwitch(id, label, on) {
  const button = el("button", "switch");
  button.type = "button";
  button.id = id;
  button.dataset.mcpToggle = id;
  button.setAttribute("aria-pressed", on ? "true" : "false");
  button.setAttribute("aria-label", label);
  button.append(el("span", "switch-knob"));
  return button;
}

function mcpService(title, name, product, placeholder) {
  const block = el("div", "mcp-service");
  const head = el("div", "mcp-head");
  head.append(el("strong", "", title));
  head.append(mcpSwitch("mcp-" + name, "Use " + title, !!product.enabled));
  block.append(head);
  const body = el("div", "mcp-service-body");
  body.id = "mcp-" + name + "-body";
  const url = field("Site URL", "text", "mcp-" + name + "-url", product.url || "");
  const urlInput = url.querySelector("input");
  urlInput.placeholder = placeholder;
  urlInput.spellcheck = false;
  urlInput.autocomplete = "off";
  const own = el("label", "mcp-own");
  const box = el("input");
  box.type = "checkbox";
  box.id = "mcp-" + name + "-own";
  box.checked = !!product.ownCredentials;
  own.append(box, document.createTextNode("Use a separate email and API token"));
  const creds = el("div", "mcp-own-fields");
  creds.id = "mcp-" + name + "-creds";
  creds.append(...mcpCredentialFields("mcp-" + name, product, "you@company.com"));
  body.append(url, own, creds, ...settingsPlaceFields(name));
  block.append(body);
  return block;
}

function rememberSettingsPlaces(cfg) {
  const jira = (cfg && cfg.jira) || {};
  const wiki = (cfg && cfg.confluence) || {};
  state.settingsPlaces = {
    jira: placeFields(jira),
    confluence: placeFields(wiki),
  };
}

function settingsPlaceFields(name) {
  const saved = () => state.settingsPlaces[name];
  if (name === "jira") {
    return [el("p", "hint", "Agents use this board unless an agent chooses another one."), searchSelect({
      label: "Board",
      placeholder: "Search Jira boards",
      value: () => saved().boardName || "",
      load: (query) => fetchPlaces("/api/atlassian/jira/boards?q=" + encodeURIComponent(query)),
      onChoose: (item) => {
        saved().boardId = item ? item.id : "";
        saved().boardName = item ? (item.name || "") : "";
        syncSave();
      },
    })];
  }
  let parentField;
  const space = searchSelect({
    label: "Space",
    placeholder: "Search Confluence spaces",
    value: () => saved().spaceName || "",
    load: (query) => fetchPlaces("/api/atlassian/confluence/spaces?q=" + encodeURIComponent(query)),
    onChoose: (item) => {
      const current = saved();
      current.spaceId = item ? item.id : "";
      current.spaceKey = item ? (item.key || "") : "";
      current.spaceName = item ? (item.name || "") : "";
      current.parentId = "";
      current.parentTitle = "";
      if (parentField) parentField.refresh();
      syncSave();
    },
  });
  parentField = searchSelect({
    label: "Parent page",
    placeholder: () => saved().spaceId ? "Search pages" : "Choose a space first.",
    value: () => saved().parentTitle || "",
    disabled: () => !saved().spaceId,
    load: (query) => {
      const current = saved();
      const params = new URLSearchParams({ space: current.spaceId || "", key: current.spaceKey || "", q: query });
      return fetchPlaces("/api/atlassian/confluence/pages?" + params.toString());
    },
    onChoose: (item) => {
      saved().parentId = item ? item.id : "";
      saved().parentTitle = item ? (item.name || "") : "";
      syncSave();
    },
  });
  return [el("p", "hint", "Agents use this space and parent page unless an agent chooses another one."), space, parentField];
}

function atlassianTokenHint() {
  const hint = el("div", "hint mcp-token-hint");
  const url = "https://id.atlassian.com/manage-profile/security/api-tokens";
  const link = document.createElement("a");
  link.className = "index-tool-link";
  link.href = url;
  link.dataset.url = url;
  link.textContent = url;
  const intro = el("p");
  intro.append(
    document.createTextNode("Sign in to Atlassian, open "),
    link,
    document.createTextNode(", choose Create API token with scopes, and paste that token here with the account email. Turn these scopes on:"),
  );
  const list = el("ul", "mcp-scopes");
  list.append(
    scopeLine("Jira", ["read:jira-work", "write:jira-work", "read:jira-user"]),
    scopeLine("Confluence", [
      "read:confluence-content.all",
      "read:confluence-space.summary",
      "write:confluence-content",
      "search:confluence",
      "read:confluence-user",
    ]),
  );
  hint.append(intro, list, el("p", "", "Jira and Confluence use this email and token unless a row has its own."));
  return hint;
}

function scopeLine(product, scopes) {
  const item = el("li");
  item.append(el("span", "", product + ": "));
  scopes.forEach((scope, index) => {
    if (index) item.append(document.createTextNode(", "));
    item.append(el("code", "", scope));
  });
  return item;
}

function mcpCredentialFields(prefix, product, emailPlaceholder) {
  const email = field("Email", "text", prefix + "-username", product.username || "");
  const emailInput = email.querySelector("input");
  emailInput.placeholder = emailPlaceholder;
  emailInput.spellcheck = false;
  emailInput.autocomplete = "off";
  const api = secretField("API token", prefix + "-api-token", product.apiToken || "");
  return [email, api];
}

function secretField(label, id, value) {
  const wrap = el("label", "field");
  wrap.append(el("span", "", label));
  const row = el("div", "secret-field");
  const input = el("input");
  input.type = "password";
  input.id = id;
  input.value = value || "";
  input.autocomplete = "new-password";
  input.spellcheck = false;
  input.placeholder = "API token";
  const button = el("button", "secret-toggle");
  button.type = "button";
  button.dataset.secretToggle = id;
  button.setAttribute("aria-pressed", "false");
  button.setAttribute("aria-label", "Show API token");
  button.append(eyeIcon(false));
  row.append(input, button);
  wrap.append(row);
  return wrap;
}

function eyeIcon(shown) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("fill", "none");
  path.setAttribute("stroke", "currentColor");
  path.setAttribute("stroke-width", "1.8");
  path.setAttribute("stroke-linecap", "round");
  path.setAttribute("stroke-linejoin", "round");
  path.setAttribute("d", shown
    ? "M3 3l18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.2A9.8 9.8 0 0 1 12 5c5 0 8.5 3.2 10 7a12 12 0 0 1-2.8 4M6.1 6.1C4.2 7.4 2.7 9.1 2 12c1.5 3.8 5 7 10 7 1.6 0 3.1-.3 4.4-.9"
    : "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z");
  svg.append(path);
  if (!shown) {
    const pupil = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    pupil.setAttribute("cx", "12");
    pupil.setAttribute("cy", "12");
    pupil.setAttribute("r", "2.5");
    pupil.setAttribute("fill", "none");
    pupil.setAttribute("stroke", "currentColor");
    pupil.setAttribute("stroke-width", "1.8");
    svg.append(pupil);
  }
  return svg;
}

function syncMCP(root) {
  root = root || document;
  const atlassian = root.querySelector("#mcp-atlassian");
  if (!atlassian) return;
  const body = root.querySelector("#mcp-atlassian-body");
  if (body) body.hidden = atlassian.getAttribute("aria-pressed") !== "true";
  for (const name of ["jira", "confluence"]) {
    const toggle = root.querySelector("#mcp-" + name);
    const detail = root.querySelector("#mcp-" + name + "-body");
    if (toggle && detail) detail.hidden = toggle.getAttribute("aria-pressed") !== "true";
    const own = root.querySelector("#mcp-" + name + "-own");
    const creds = root.querySelector("#mcp-" + name + "-creds");
    if (own && creds) creds.hidden = !own.checked;
  }
}

function fieldValue(selector) {
  const input = document.querySelector(selector);
  return input ? input.value.trim() : "";
}

function mcpPressed(id) {
  const button = document.querySelector(id);
  return !!button && button.getAttribute("aria-pressed") === "true";
}

function mcpChecked(id) {
  const input = document.querySelector(id);
  return !!input && input.checked;
}

function mcpDirty() {
  const toggle = document.querySelector("#mcp-atlassian");
  if (!toggle || !state.data) return false;
  const saved = state.data.atlassian || {};
  if (mcpPressed("#mcp-atlassian") !== !!saved.enabled) return true;
  if (fieldValue("#mcp-username") !== (saved.username || "")) return true;
  if (fieldValue("#mcp-api-token") !== (saved.apiToken || "")) return true;
  return mcpServiceDirty("jira", saved.jira || {}) || mcpServiceDirty("confluence", saved.confluence || {});
}

function mcpServiceDirty(name, saved) {
  if (mcpPressed("#mcp-" + name) !== !!saved.enabled) return true;
  if (fieldValue("#mcp-" + name + "-url") !== (saved.url || "")) return true;
  if (mcpChecked("#mcp-" + name + "-own") !== !!saved.ownCredentials) return true;
  if (fieldValue("#mcp-" + name + "-username") !== (saved.username || "")) return true;
  if (fieldValue("#mcp-" + name + "-api-token") !== (saved.apiToken || "")) return true;
  const picked = (state.settingsPlaces && state.settingsPlaces[name]) || {};
  const keys = name === "jira" ? ["boardId", "boardName"] : ["spaceId", "spaceKey", "spaceName", "parentId", "parentTitle"];
  return keys.some((key) => (picked[key] || "") !== (saved[key] || ""));
}

function collectAtlassian() {
  const toggle = document.querySelector("#mcp-atlassian");
  if (!toggle) return null;
  const picked = state.settingsPlaces || { jira: {}, confluence: {} };
  const read = (name) => Object.assign(placeFields(picked[name]), {
    enabled: mcpPressed("#mcp-" + name),
    url: fieldValue("#mcp-" + name + "-url"),
    ownCredentials: mcpChecked("#mcp-" + name + "-own"),
    username: fieldValue("#mcp-" + name + "-username"),
    apiToken: fieldValue("#mcp-" + name + "-api-token"),
  });
  return {
    enabled: mcpPressed("#mcp-atlassian"),
    username: fieldValue("#mcp-username"),
    apiToken: fieldValue("#mcp-api-token"),
    jira: read("jira"),
    confluence: read("confluence"),
  };
}

function renderTeam(team) {
  const section = el("section", "team-setup wide");
  section.append(el("h3", "", "Team folder"));
  section.append(el("p", "hint", "Choose a folder on this computer or type its full path. Your name appears after that folder is chosen. If it already has members, pick one as your name or enter a new name. Save copies this app's agents into your agents folder there. A name you type that already exists asks whether to enter a different name or use that folder."));
  const pathValue = (team.path || "").trim();
  const path = field("Team folder path", "text", "team-path", team.path || "");
  const pathInput = path.querySelector("input");
  pathInput.spellcheck = false;
  pathInput.autocomplete = "off";
  pathInput.placeholder = "/path/to/team";
  section.append(path);
  const actions = el("div", "team-actions");
  const browse = el("button", "", "Choose folder");
  browse.type = "button";
  browse.id = "team-browse";
  actions.append(browse);
  section.append(actions);
  if (!teamPathReady(pathValue)) return section;
  const members = teamChoicesApply(pathValue) ? teamListedMembers : [];
  const typedName = (team.user || "").trim();
  const pickedName = (team.memberName || "").trim();
  let mode = team.nameMode === "existing" || team.nameMode === "new" ? team.nameMode : "";
  if (!mode) mode = members.includes(typedName) || (members.length && !typedName) ? "existing" : "new";
  const selectedMember = members.includes(pickedName) ? pickedName : (members.includes(typedName) ? typedName : "");
  const hasPath = pathValue !== "";
  const activeName = mode === "existing" ? selectedMember : typedName;
  const missingName = hasPath && activeName === "";
  if (members.length) {
    const choices = el("fieldset", "choice-block team-name");
    choices.id = "team-name-choices";
    choices.classList.toggle("is-invalid", missingName);
    choices.append(el("legend", "", "Your name"));
    const existing = el("input");
    existing.type = "radio";
    existing.name = "team-name-mode";
    existing.value = "existing";
    existing.checked = mode === "existing";
    const existingLabel = el("label");
    existingLabel.append(existing, document.createTextNode("Use an existing folder"));
    const member = el("select");
    member.id = "team-member-name";
    member.disabled = mode !== "existing";
    member.hidden = mode !== "existing";
    const placeholder = el("option", "", "Choose a folder");
    placeholder.value = "";
    member.append(placeholder);
    for (const name of members) {
      const option = el("option", "", name);
      option.value = name;
      if (name === selectedMember) option.selected = true;
      member.append(option);
    }
    const existingRow = el("div", "choice-row");
    existingRow.append(existingLabel, member);
    const fresh = el("input");
    fresh.type = "radio";
    fresh.name = "team-name-mode";
    fresh.value = "new";
    fresh.checked = mode === "new";
    const freshLabel = el("label");
    freshLabel.append(fresh, document.createTextNode("Enter a new name"));
    const userInput = el("input");
    userInput.type = "text";
    userInput.id = "team-user";
    userInput.value = typedName;
    userInput.spellcheck = false;
    userInput.autocomplete = "off";
    userInput.placeholder = "Your folder name";
    userInput.disabled = mode !== "new";
    userInput.hidden = mode !== "new";
    userInput.required = hasPath && mode === "new";
    userInput.setAttribute("aria-required", hasPath && mode === "new" ? "true" : "false");
    userInput.setAttribute("aria-invalid", missingName && mode === "new" ? "true" : "false");
    const freshRow = el("div", "choice-row");
    freshRow.append(freshLabel, userInput);
    const nameError = el("p", "field-error", settingsFieldRequiredMsg);
    nameError.id = "team-user-error";
    nameError.hidden = !missingName;
    choices.append(existingRow, freshRow, nameError);
    section.append(choices);
  } else {
    const user = field("Your name", "text", "team-user", typedName);
    const userInput = user.querySelector("input");
    userInput.spellcheck = false;
    userInput.autocomplete = "off";
    userInput.placeholder = "Your folder name";
    userInput.required = hasPath;
    userInput.setAttribute("aria-required", hasPath ? "true" : "false");
    userInput.setAttribute("aria-invalid", missingName ? "true" : "false");
    user.classList.toggle("is-invalid", missingName);
    const nameError = el("p", "field-error", settingsFieldRequiredMsg);
    nameError.id = "team-user-error";
    nameError.hidden = !missingName;
    user.append(nameError);
    section.append(user);
  }
  return section;
}

function mountTeam(team) {
  const typed = document.querySelector("#team-user");
  const pathInput = document.querySelector("#team-path");
  const mode = document.querySelector("input[name='team-name-mode']:checked");
  const member = document.querySelector("#team-member-name");
  const chosenExisting = mode && mode.value === "existing";
  const next = renderTeam(Object.assign({}, team, {
    user: chosenExisting ? (member ? member.value : "") : (typed ? typed.value : (team.user || "")),
    path: team.path != null ? team.path : (pathInput ? pathInput.value : ""),
    nameMode: mode ? mode.value : (team.nameMode || ""),
    memberName: member ? member.value : (team.memberName || ""),
  }));
  const current = document.querySelector(".team-setup");
  if (current) current.replaceWith(next);
}

function refreshTeamNameChoices(pathInput) {
  const shown = !!document.querySelector("#team-user");
  if (shown === teamPathReady(pathInput.value)) return;
  const pos = pathInput.selectionStart;
  mountTeam(savedTeamForMount({ path: pathInput.value }));
  const again = document.querySelector("#team-path");
  if (!again) return;
  again.focus();
  if (pos != null) again.setSelectionRange(pos, pos);
  syncTeamName();
  syncSave();
}

async function browseTeamFolder() {
  const button = document.querySelector("#team-browse");
  button.disabled = true;
  button.classList.add("busy");
  button.textContent = "Choosing folder…";
  try {
    const picked = await api("/api/team/browse", { method: "POST" });
    if ((picked.path || "").trim()) await stageTeamPath(picked.path);
  } catch (error) {
    banner(error.message, true);
    if (button.isConnected) {
      button.disabled = false;
      button.classList.remove("busy");
      button.textContent = "Choose folder";
    }
  }
}

async function stageTeamPath(path) {
  const token = ++teamStage;
  const data = await api("/api/team/members?path=" + encodeURIComponent(path));
  if (token !== teamStage || !document.querySelector("#settings-dialog").open) return;
  teamListedPath = data.path || path;
  teamListedMembers = data.members || [];
  mountTeam(savedTeamForMount({ path: teamListedPath }));
  syncTeamName();
  const pathInput = document.querySelector("#team-path");
  if (document.activeElement !== pathInput) {
    if (teamNameMode() === "existing") {
      const member = document.querySelector("#team-member-name");
      if (member && !member.value) member.focus();
    } else {
      const user = document.querySelector("#team-user");
      if (user && !user.value.trim()) user.focus();
    }
  }
  syncSave();
}

function cancelTeamStage() {
  teamStage += 1;
  teamListedPath = "";
  teamListedMembers = [];
}

async function loadAgentMembers(selectedName) {
  const token = ++teamMemberLoad;
  const query = selectedName ? "?member=" + encodeURIComponent(selectedName) : "";
  try {
    const data = await api("/api/team/compare" + query);
    if (token !== teamMemberLoad || agentMemberBusy) return;
    teamShare = data;
    fillMemberSelect(data.members || [], selectedName);
    paintTeamShare();
  } catch (error) {
    if (token !== teamMemberLoad || agentMemberBusy) return;
    teamShare = null;
    paintShareBadge(false, false);
    const note = document.querySelector("#agent-member-note");
    if (note) note.textContent = error.message;
    const select = document.querySelector("#agent-member");
    if (!select) return;
    select.replaceChildren();
    const waiting = el("option", "", "Members unavailable");
    waiting.value = "";
    select.append(waiting);
    select.disabled = true;
  }
}

function refreshTeamShare() {
  const team = (state.data && state.data.teamFolder) || {};
  if (!String(team.path || "").trim() || !String(team.user || "").trim() || agentMemberBusy) return Promise.resolve();
  const select = document.querySelector("#agent-member");
  const selected = select && select.isConnected && !select.disabled ? select.value : chosenMember(team);
  return loadAgentMembers(selected);
}

function fillMemberSelect(members, selectedName) {
  const select = document.querySelector("#agent-member");
  if (!select || agentMemberBusy) return;
  select.replaceChildren();
  const placeholder = el("option", "", "Choose a member");
  placeholder.value = "";
  select.append(placeholder);
  let found = false;
  const user = ((state.data && state.data.teamFolder) || {}).user || "";
  for (const name of members) {
    const option = el("option", "", name === user ? name + " (You)" : name);
    option.value = name;
    if (name === selectedName) {
      option.selected = true;
      found = true;
    }
    select.append(option);
  }
  select.disabled = false;
  select.dataset.kept = select.value;
  const note = document.querySelector("#agent-member-note");
  if (note && !members.length) note.textContent = "The team folder has no member folders yet.";
  else if (note && selectedName && !found) note.textContent = selectedName + " is no longer in the team folder. Choose another.";
}

function agentsByID(agents) {
  const map = new Map();
  for (const agent of agents || []) map.set(agent.id, agent);
  return map;
}

function agentFileChanged(agent, otherByID) {
  if (!otherByID) return false;
  const other = otherByID.get(agent.id);
  if (!other) return true;
  if (String(other.category || "") !== String(agent.category || "")) return true;
  return String(other.hash || "") !== String(agent.hash || "");
}

function fillAgentList(list, agents, other, folder) {
  list.replaceChildren();
  if (!agents.length) {
    list.append(el("li", "hint", "No agents."));
    return;
  }
  const otherByID = other ? agentsByID(other) : null;
  for (const agent of agents) {
    const item = el("li");
    const name = el("span", "name", agent.name || agent.id);
    const meta = agent.category ? el("span", "meta", agent.category) : null;
    if (agentFileChanged(agent, otherByID)) {
      item.classList.add("is-diff");
      const button = el("button", "agent-diff");
      button.type = "button";
      button.dataset.agentDiff = agent.id;
      button.dataset.folder = folder || "";
      button.setAttribute("aria-label", "Show how " + (agent.name || agent.id) + " differs");
      button.append(name);
      if (meta) button.append(meta);
      item.append(button);
    } else {
      item.append(name);
      if (meta) item.append(meta);
    }
    list.append(item);
  }
}

function setArrow(button, enabled, label) {
  if (!button || button.classList.contains("busy")) return;
  button.disabled = !enabled;
  button.classList.toggle("warn", enabled);
  button.setAttribute("aria-label", label);
  button.title = label;
}

function paintPair(id, title, folderHeading, app, side) {
  const block = document.querySelector("#" + id);
  if (!block) return;
  block.querySelector("h3").textContent = title;
  const views = block.querySelectorAll(".team-view");
  views[0].querySelector("h4").textContent = "This app's agents";
  views[1].querySelector("h4").textContent = folderHeading;
  const badge = block.querySelector(".badge");
  const comparable = !!(side && side.name && !side.missing);
  const folderAgents = comparable ? (side.agents || []) : null;
  fillAgentList(views[0].querySelector("ul"), app, folderAgents, comparable ? side.name : "");
  if (!comparable) {
    views[1].querySelector("ul").replaceChildren(el("li", "hint", side && side.missing ? "This folder is not in the team folder." : "Choose a member."));
    badge.hidden = true;
  } else {
    fillAgentList(views[1].querySelector("ul"), side.agents || [], app, side.name);
    badge.hidden = false;
    badge.className = "badge " + (side.differs ? "wait" : "ok");
    badge.textContent = side.differs ? "Not matched" : "Matches";
  }
  const name = comparable ? side.name : "";
  const toFolder = block.querySelector("[data-sync='push']");
  const toApp = block.querySelector("[data-sync='pull']");
  if (toFolder) toFolder.dataset.folder = name;
  if (toApp) toApp.dataset.folder = name;
  setArrow(toFolder, comparable && !!side.differs && app.length > 0, name ? "Copy this app's agents to " + name : "Copy this app's agents to the folder");
  setArrow(toApp, comparable && !!side.differs && (side.agents || []).length > 0, name ? "Copy " + name + "'s agents to this app" : "Copy the folder's agents to this app");
}

function paintTeamShare() {
  const share = teamShare || {};
  const app = share.app || [];
  const user = share.user || { name: "", differs: false, agents: [] };
  const member = share.member || { name: "", differs: false, agents: [] };
  const teamState = (state.data && state.data.teamFolder) || {};
  paintMemberInUse(teamState.member || "");
  const yours = !!(user.name && member.name && member.name === user.name);
  const memberBlock = document.querySelector("#member-compare");
  if (memberBlock) memberBlock.hidden = yours;
  const memberOff = !!(member.name && member.name !== user.name && (member.missing || member.differs));
  paintShareBadge(!!user.name && !user.differs && !memberOff, true);
  paintPair("user-compare", "This app and " + (user.name || "your folder"), user.name ? user.name + "'s agents" : "Your agents", app, user);
  if (!yours) paintPair("member-compare", member.name ? "This app and " + member.name : "This app and a member", member.name ? member.name + "'s agents" : "Member agents", app, member);
  const note = document.querySelector("#agent-member-note");
  const select = document.querySelector("#agent-member");
  if (!note || (select && select.disabled)) return;
  if (!(share.members || []).length) return;
  if (member.missing) note.textContent = member.name + " is no longer in the team folder. Choose another.";
  else if (!member.name) note.textContent = "Choose a member to compare that folder with this app.";
  else if (member.differs) note.textContent = member.name + "'s agents differ from this app's agents.";
  else note.textContent = member.name + "'s agents match this app's agents.";
}

function paintShareBadge(matched, ready) {
  const badge = document.querySelector("#team-share-badge");
  if (!badge) return;
  badge.hidden = !ready;
  if (!ready) return;
  badge.className = "badge " + (matched ? "ok" : "wait");
  badge.textContent = matched ? "Matches" : "Not matched";
}

function saveButton() {
  const actions = el("div", "actions");
  actions.hidden = true;
  const save = el("button", "", "Save");
  save.type = "button";
  save.dataset.save = "1";
  actions.append(save);
  return actions;
}

function sameNames(left, right) {
  const a = [...left].sort();
  const b = [...right].sort();
  return a.length === b.length && a.every((name, index) => name === b[index]);
}

function reposDirty() {
  if (!state.data) return false;
  for (const repo of state.data.repos) {
    if (repo.kind === "document") continue;
    const tags = document.querySelector(`[data-services="${repo.kind}"]`);
    if (!tags) return false;
    const selected = [...tags.querySelectorAll("button[aria-pressed='true']")].map((item) => item.dataset.name);
    if (!sameNames(selected, repo.services || [])) return true;
  }
  return false;
}

function configDirty() {
  if (!state.data) return false;
  const workspace = document.querySelector("#workspace-id");
  const orchestrator = document.querySelector("#orchestrator");
  const tool = document.querySelector("#generate-tool");
  const model = document.querySelector("#generate-model");
  if (!workspace || !orchestrator || !tool || !model) return false;
  const savedTeam = state.data.teamFolder || {};
  const teamPath = document.querySelector("#team-path");
  const teamUser = document.querySelector("#team-user");
  const teamChanged = !!teamPath && teamPath.value.trim() !== (savedTeam.path || "");
  const userChanged = !!teamUser && currentTeamUser() !== (savedTeam.user || "");
  return teamChanged || userChanged
    || workspace.value !== (state.data.workspaceId || "")
    || orchestrator.value !== (state.data.orchestrator || "orca")
    || tool.value !== tool.dataset.saved
    || model.value !== model.dataset.saved
    || mcpDirty()
    || toolsDirty();
}

function syncSave() {
  const repos = document.querySelector("#panel-repos [data-save]");
  const config = document.querySelector("#settings-save");
  if (repos) repos.parentElement.hidden = !reposDirty();
  if (config && !config.classList.contains("busy")) config.disabled = !configDirty();
}

function paintGenerateIcon(button) {
  if (!button || button.classList.contains("busy")) return;
  button.textContent = "Rewrite by AI";
  button.setAttribute("aria-label", "Rewrite by AI");
}

function paintGenerateIcons() {
  for (const button of document.querySelectorAll(".field-generate")) paintGenerateIcon(button);
}

function generateButton(kind, subject) {
  const button = el("button", "field-generate", "Rewrite by AI");
  button.type = "button";
  button.dataset.generate = kind;
  button.setAttribute("aria-label", "Rewrite by AI");
  if (subject) button.dataset.generateSubject = subject;
  return button;
}

function placeGenerateButtons() {
  for (const area of document.querySelectorAll("#agent-dialog .compose textarea")) {
    const compose = area.closest(".compose");
    if (!compose) continue;
    compose.classList.toggle("has-scroll", area.scrollHeight > area.clientHeight + 1);
  }
}

let generateAbort = null;
let generatePreviewAbort = null;
let generatePending = null;
let generateDraft = null;
let generateSawThink = false;

function generateTitle(button) {
  const subject = button.dataset.generateSubject || "";
  let what = subject;
  if (button.dataset.generate === "prompt") what = "prompt";
  else if (!what && button.dataset.generate === "repo") what = "repository";
  else if (!what && button.dataset.generate === "mcp") what = "MCP Servers";
  else if (!what && button.dataset.generate === "tool") what = "tool";
  else if (!what) what = "Agents Coordination";
  return "Rewrite " + what + " by AI";
}

function compareCell(text, kind) {
  const cell = el("div", "diff-line " + (kind || "same"));
  cell.textContent = text === "" ? "\u00a0" : text;
  return cell;
}

function compareHead(title, keep) {
  const head = el("div", "compare-head");
  if (!keep) {
    head.textContent = title;
    return head;
  }
  head.append(el("span", "", title));
  const button = el("button", "primary", "Use");
  button.type = "button";
  button.dataset.keep = keep;
  button.setAttribute("aria-label", keep === "local" ? "Use this app" : "Use team folder");
  head.append(button);
  return head;
}

function renderCompareRows(rows, grid, titles, keeps) {
  grid = grid || document.querySelector("#generate-compare");
  grid.replaceChildren();
  const left = el("div", "compare-col");
  const right = el("div", "compare-col");
  left.append(compareHead((titles && titles[0]) || "Before", keeps && keeps[0]));
  right.append(compareHead((titles && titles[1]) || "After", keeps && keeps[1]));
  for (const row of rows) {
    left.append(compareCell(row.left, row.leftKind));
    right.append(compareCell(row.right, row.rightKind));
  }
  grid.append(left, right);
}

function showSideBySideDiff(lines, options) {
  const grid = options && options.grid;
  const titles = options && options.titles;
  if (!lines || !lines.length) {
    renderCompareRows([{ left: "No changes.", right: "No changes.", leftKind: "hunk", rightKind: "hunk" }], grid, titles);
    return;
  }
  const rows = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (line.kind === "hunk") {
      rows.push({ left: line.text, right: line.text, leftKind: "hunk", rightKind: "hunk" });
      index++;
      continue;
    }
    if (line.kind === "same") {
      rows.push({ left: line.text, right: line.text, leftKind: "same", rightKind: "same" });
      index++;
      continue;
    }
    const removed = [];
    const added = [];
    while (index < lines.length && lines[index].kind === "del") {
      removed.push(lines[index].text);
      index++;
    }
    while (index < lines.length && lines[index].kind === "add") {
      added.push(lines[index].text);
      index++;
    }
    const count = Math.max(removed.length, added.length);
    for (let row = 0; row < count; row++) {
      rows.push({
        left: removed[row] || "",
        right: added[row] || "",
        leftKind: removed[row] == null ? "empty" : "del",
        rightKind: added[row] == null ? "empty" : "add",
      });
    }
  }
  renderCompareRows(rows, grid, titles);
}

function showTeamShareList() {
  const dialog = document.querySelector("#team-share-dialog");
  if (dialog) dialog.classList.remove("diffing");
  const bar = document.querySelector("#team-share-bar");
  const diffBar = document.querySelector("#team-share-diff-bar");
  const body = document.querySelector("#team-share-body");
  const diff = document.querySelector("#team-share-diff");
  if (bar) bar.hidden = false;
  if (body) body.hidden = false;
  if (diffBar) diffBar.hidden = true;
  if (diff) diff.hidden = true;
}

async function openAgentDiff(id, folder) {
  const data = await api("/api/team/diff?folder=" + encodeURIComponent(folder) + "&id=" + encodeURIComponent(id));
  document.querySelector("#team-share-diff-title").textContent = data.name || id;
  showSideBySideDiff(data.diff || [], {
    grid: document.querySelector("#team-share-diff-grid"),
    titles: ["This app", (data.folder || folder) + "'s agents"],
  });
  document.querySelector("#team-share-bar").hidden = true;
  document.querySelector("#team-share-body").hidden = true;
  document.querySelector("#team-share-diff-bar").hidden = false;
  document.querySelector("#team-share-diff").hidden = false;
  document.querySelector("#team-share-dialog").classList.add("diffing");
}

function modelLabel(tool, id) {
  const found = modelsFor(tool).find((item) => item.id === id);
  if (found) return found.label;
  return id || "default";
}

function showGenerateCommand(command) {
  const node = document.querySelector("#generate-command");
  const text = document.querySelector("#generate-command-text");
  text.textContent = String(command || "");
  node.hidden = !command;
}

function showGenerateChoice(tool, model) {
  const node = document.querySelector("#generate-choice");
  if (!tool && !model) {
    node.textContent = "";
    return;
  }
  node.textContent = toolLabel(tool) + " · " + modelLabel(tool, model);
}

function savedGenerateChoice() {
  const data = state.data || {};
  const tool = data.generateTool || "";
  return { tool, model: data.generateModel || defaultModel(tool) };
}

function resetGenerateOutput() {
  generateSawThink = false;
  generateDraft = null;
  const thinking = document.querySelector("#generate-thinking");
  const answer = document.querySelector("#generate-answer");
  const spinner = document.querySelector("#generate-spinner");
  const use = document.querySelector("#generate-use");
  thinking.textContent = "";
  answer.textContent = "";
  answer.hidden = true;
  spinner.hidden = true;
  document.querySelector("#generate-thinking-label").hidden = true;
  const run = document.querySelector("#generate-run");
  run.hidden = false;
  run.disabled = true;
  use.disabled = true;
  const command = document.querySelector("#generate-command");
  command.hidden = false;
  document.querySelector("#generate-command-text").replaceChildren();
  const compare = document.querySelector(".generate-compare");
  compare.hidden = true;
  document.querySelector("#generate-compare").replaceChildren();
}

function showAgentForm() {
  document.querySelector("#agent-generate").hidden = true;
  document.querySelector("#agent-form").hidden = false;
  document.querySelector("#agent-dialog").classList.remove("generating");
}

function generateAgentLabel() {
  const name = (document.querySelector("#agent-name").value || "").trim();
  const id = (document.querySelector("#agent-id").value || "").trim();
  if (name && id && name !== id) return name + " · " + id;
  return name || id;
}

function openGenerateDialog(title) {
  document.querySelector("#generate-dialog-title").textContent = title;
  document.querySelector("#generate-dialog-agent").textContent = generateAgentLabel();
  const choice = savedGenerateChoice();
  showGenerateChoice(choice.tool, choice.model);
  resetGenerateOutput();
  const error = document.querySelector("#generate-error");
  error.hidden = true;
  error.textContent = "";
  document.querySelector("#agent-form").hidden = true;
  document.querySelector("#agent-generate").hidden = false;
  document.querySelector("#agent-dialog").classList.add("generating");
}

function appendGenerateLog(channel, text) {
  const log = document.querySelector("#generate-log");
  const thinking = document.querySelector("#generate-thinking");
  const answer = document.querySelector("#generate-answer");
  if (channel === "think") {
    if (!String(text).trim() && !generateSawThink) return;
    generateSawThink = true;
    thinking.textContent += text;
  } else {
    answer.hidden = false;
    answer.textContent += text;
  }
  log.scrollTop = log.scrollHeight;
}

function finishGenerateSpinner() {
  document.querySelector("#generate-spinner").hidden = true;
}

function storeGeneratedText(button, value) {
  if (button.dataset.generate === "repo") state.repoNotes[button.dataset.generateSubject] = value;
  if (button.dataset.generate === "teamwork") state.peerNotes[button.dataset.generateSubject] = value;
  const spec = useSections[button.dataset.generate];
  if (!spec) return;
  if (!state[spec.state]) state[spec.state] = {};
  const area = button.closest(".field") && button.closest(".field").querySelector("textarea");
  const id = (area && area.dataset.use) || button.dataset.generateSubject;
  state[spec.state][id] = value;
}

function prepareGenerate(button) {
  const block = button.closest(".field");
  const area = block.querySelector("textarea");
  const error = document.querySelector("#agent-error");
  const text = area.value.trim();
  if (!text) {
    error.hidden = false;
    error.textContent = "Enter some text first.";
    return;
  }
  error.hidden = true;
  error.textContent = "";
  generatePending = { button, area, text };
  openGenerateDialog(generateTitle(button));
  loadGeneratePreview();
}

function stopGeneratePreview() {
  if (!generatePreviewAbort) return;
  generatePreviewAbort.abort();
  generatePreviewAbort = null;
}

async function loadGeneratePreview() {
  const pending = generatePending;
  if (!pending) return;
  stopGeneratePreview();
  const controller = new AbortController();
  generatePreviewAbort = controller;
  try {
    const data = await api("/api/generate/preview", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      signal: controller.signal,
      body: JSON.stringify({
        kind: pending.button.dataset.generate,
        subject: pending.button.dataset.generateSubject || "",
        text: pending.text,
      }),
    });
    if (generatePreviewAbort !== controller) return;
    showGenerateChoice(data.tool, data.model);
    showGenerateCommand(data.command);
    document.querySelector("#generate-run").disabled = false;
  } catch (err) {
    if (err.name === "AbortError") return;
    const node = document.querySelector("#generate-error");
    if (!node) return;
    node.hidden = false;
    node.textContent = err.message;
  } finally {
    if (generatePreviewAbort === controller) generatePreviewAbort = null;
  }
}

async function rewriteText(button) {
  const pending = generatePending;
  const area = pending ? pending.area : button.closest(".field").querySelector("textarea");
  const error = document.querySelector("#agent-error");
  const text = pending ? pending.text : area.value.trim();
  if (generateAbort) generateAbort.abort();
  const controller = new AbortController();
  generateAbort = controller;
  document.querySelector("#generate-run").hidden = true;
  document.querySelector("#generate-thinking-label").hidden = false;
  document.querySelector("#generate-spinner").hidden = false;
  button.disabled = true;
  button.classList.add("busy");
  try {
    const response = await fetch("/api/generate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      signal: controller.signal,
      body: JSON.stringify({
        kind: button.dataset.generate,
        subject: button.dataset.generateSubject || "",
        text,
      }),
    });
    if (!response.ok) {
      const data = await response.json().catch(() => ({}));
      throw new Error(data.error || response.statusText);
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    let finished = false;
    while (!finished) {
      const chunk = await reader.read();
      if (chunk.done) break;
      buffer += decoder.decode(chunk.value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop();
      for (const line of lines) {
        if (!line.trim()) continue;
        const event = JSON.parse(line);
        if (event.type === "meta") showGenerateChoice(event.tool, event.model);
        if (event.type === "command") showGenerateCommand(event.command);
        if (event.type === "log") appendGenerateLog(event.channel, event.text || "");
        if (event.type === "error") throw new Error(event.error || "Generate failed.");
        if (event.type === "done") {
          const value = event.text || "";
          generateDraft = { area, button, text: value };
          if (!document.querySelector("#generate-answer").textContent) {
            appendGenerateLog("text", value);
          }
          document.querySelector("#generate-use").disabled = false;
          document.querySelector(".generate-compare").hidden = false;
          finishGenerateSpinner();
          showSideBySideDiff(event.diff || []);
          finished = true;
        }
      }
    }
  } catch (err) {
    if (err.name === "AbortError") return;
    finishGenerateSpinner();
    const generateError = document.querySelector("#generate-error");
    if (generateError && !document.querySelector("#agent-generate").hidden) {
      generateError.hidden = false;
      generateError.textContent = err.message;
    } else if (error.isConnected) {
      error.hidden = false;
      error.textContent = err.message;
    }
  } finally {
    if (generateAbort === controller) generateAbort = null;
    if (button.isConnected) {
      button.disabled = false;
      button.classList.remove("busy");
    }
  }
}

function field(label, type, id, value) {
  const wrap = el("label", "field");
  wrap.append(el("span", "", label));
  const input = el("input");
  input.type = type;
  input.id = id;
  input.value = value || "";
  wrap.append(input);
  return wrap;
}

function fillServiceTags(tags, note, available, selected) {
  if (!tags || !note) return;
  const chosen = new Set(selected);
  const names = [...available];
  for (const name of selected) {
    if (!names.includes(name)) names.push(name);
  }
  tags.replaceChildren();
  if (!names.length) {
    note.textContent = "No Nx projects in this folder.";
    return;
  }
  note.textContent = "Select the Nx projects agents should use.";
  for (const name of names) {
    const button = el("button", "tag", name);
    button.type = "button";
    button.dataset.name = name;
    button.setAttribute("aria-pressed", chosen.has(name) ? "true" : "false");
    if (!available.includes(name)) button.classList.add("missing");
    tags.append(button);
  }
}

function indexLabel(index) {
  if (index === "obsidian") return "Obsidian";
  if (index === "codegraph") return "CodeGraph";
  return "not set";
}

function indexItem(kind) {
  return ((state.data && state.data.indexes) || []).find((item) => item.kind === kind) || null;
}

function indexAssigned(item) {
  return !!item && (item.index === "obsidian" || item.index === "codegraph");
}

function repoIndexSection(repo) {
  const section = el("div", "repo-index");
  section.dataset.repoIndex = repo.kind;
  const title = el("div", "repo-index-title");
  title.append(el("p", "repo-region", "Index tool"));
  const status = el("div", "index-repo-status");
  status.dataset.repoIndexStatus = repo.kind;
  const toggle = el("button", "switch");
  toggle.type = "button";
  toggle.dataset.repoIndexUse = repo.kind;
  toggle.append(el("span", "switch-knob"));
  title.append(status, toggle);
  const head = el("div", "repo-index-head");
  const field = el("label", "field repo-index-field");
  const select = el("select");
  select.dataset.repoIndexChoice = repo.kind;
  select.setAttribute("aria-label", "Index for " + (repoLabel[repo.kind] || repo.kind));
  for (const [value, label] of [["obsidian", "Obsidian"], ["codegraph", "CodeGraph"]]) {
    const option = el("option", "", label);
    option.value = value;
    select.append(option);
  }
  field.append(select);
  const actions = el("div", "repo-index-actions");
  actions.dataset.repoIndexActions = repo.kind;
  head.append(field, actions);
  const detail = el("div", "repo-index-detail");
  detail.dataset.repoIndexDetail = repo.kind;
  section.append(title, head, detail);
  paintRepoIndex(section, repo);
  return section;
}

function repoFolderSection(repo) {
  const section = el("div", "repo-folder");
  section.dataset.repoFolderSection = repo.kind;
  section.append(el("p", "repo-region", "Folder path"));
  const row = el("div", "repo-folder-row");
  const input = el("input");
  input.type = "text";
  input.spellcheck = false;
  input.autocomplete = "off";
  input.placeholder = "Full path of the folder";
  input.value = repo.folder || "";
  input.dataset.repoFolder = repo.kind;
  input.dataset.saved = repo.folder || "";
  input.setAttribute("aria-label", "Folder for " + (repoLabel[repo.kind] || repo.kind));
  const browse = el("button", "", "Browse");
  browse.type = "button";
  browse.dataset.browseFolder = repo.kind;
  row.append(input, browse);
  section.append(row);
  if (repo.folder && !repo.exists) section.append(el("p", "hint", "Folder is missing."));
  if (repo.empty) section.append(el("p", "hint", "This folder is empty."));
  if (repo.sparse) section.append(el("p", "hint", "Checkout: " + repo.sparse + ". Other folders are not in this working tree."));
  if (repo.empty) {
    const clone = el("button", "", "Clone repository from git");
    clone.type = "button";
    clone.dataset.clone = repo.kind;
    section.append(clone);
  }
  if (repo.kind !== "document") {
    const services = el("div", "services");
    services.append(el("span", "hint", "Services"));
    const tags = el("div", "service-tags");
    tags.dataset.services = repo.kind;
    services.append(tags);
    const note = el("p", "hint", "");
    note.dataset.serviceNote = repo.kind;
    services.append(note);
    fillServiceTags(tags, note, repo.available || [], repo.services || []);
    section.append(services);
  }
  return section;
}

function paintRepoIndexes() {
  if (!state.data) return;
  for (const repo of state.data.repos || []) {
    const section = document.querySelector(`#panel-repos [data-repo-index="${CSS.escape(repo.kind)}"]`);
    if (section) paintRepoIndex(section, repo);
  }
  if (state.checkingIndexes.size) markIndexesChecking();
}

function paintRepoIndex(section, repo) {
  const select = section.querySelector("[data-repo-index-choice]");
  const saved = repo.index || "";
  const dialog = document.querySelector("#repo-index-dialog");
  const confirming = dialog && dialog.open && dialog.dataset.kind === repo.kind;
  const settingUp = !!state.busySetup && state.busyKind === repo.kind;
  const pending = state.pendingRepoIndex && state.pendingRepoIndex.kind === repo.kind;
  const enabled = saved === "obsidian" || saved === "codegraph";
  const using = enabled || settingUp || pending || (confirming && !enabled);
  const locked = settingUp || pending;
  section.classList.toggle("is-off", !using);
  if (select) {
    select.dataset.saved = saved;
    if (!settingUp && !confirming && !pending) select.value = shownIndexTool(repo);
    select.disabled = locked;
    if (settingUp) select.setAttribute("aria-busy", "true");
    else select.removeAttribute("aria-busy");
  }
  const toggle = section.querySelector("[data-repo-index-use]");
  if (toggle) {
    toggle.setAttribute("aria-pressed", using ? "true" : "false");
    toggle.disabled = locked;
    const name = repoLabel[repo.kind] || repo.kind;
    toggle.setAttribute("aria-label", (using ? "Stop using the index on " : "Use the index on ") + name);
  }
  const item = indexItem(repo.kind);
  const assigned = indexAssigned(item) && item.index === saved;
  const status = section.querySelector("[data-repo-index-status]");
  if (status) {
    status.replaceChildren();
    if (assigned && !settingUp) status.append(el("span", "badge " + badgeClass(item), badgeText(item)));
  }
  const actions = section.querySelector("[data-repo-index-actions]");
  if (actions) {
    actions.replaceChildren();
    actions.removeAttribute("role");
    actions.removeAttribute("aria-label");
    if (settingUp) {
      const spin = el("span", "spinner");
      spin.setAttribute("aria-hidden", "true");
      actions.setAttribute("role", "status");
      actions.setAttribute("aria-label", "Setting up");
      actions.append(spin, el("span", "repo-index-loading", "Setting up…"));
    } else if (assigned) {
      const refresh = el("button", "icon-button");
      refresh.type = "button";
      refresh.dataset.refreshRepo = repo.kind;
      const busy = state.checkingIndexes.has(repo.kind);
      refresh.disabled = busy;
      refresh.classList.toggle("spin", busy);
      refresh.setAttribute("aria-busy", busy ? "true" : "false");
      refresh.setAttribute("aria-label", "Check " + indexLabel(item.index) + " on " + (repoLabel[repo.kind] || repo.kind));
      refresh.append(refreshIcon());
      actions.append(refresh, indexRemoveButton(item));
    }
  }
  const detail = section.querySelector("[data-repo-index-detail]");
  if (detail) {
    detail.replaceChildren();
    if (!assigned && !settingUp) {
      detail.append(el("p", "hint", "Turn the index on to choose Obsidian or CodeGraph. A tool already in the folder is used as it is."));
    }
    if (assigned) {
      const url = indexToolURL[item.index];
      if (url) {
        const link = document.createElement("a");
        link.className = "index-tool-link";
        link.href = url;
        link.dataset.url = url;
        link.textContent = url;
        detail.append(link);
      }
      if (item.command) detail.append(el("p", "command", item.command));
      if (item.summary) detail.append(el("p", "hint", item.summary));
    }
  }
}

function paintRepoSetupButtons() {
  for (const select of document.querySelectorAll("#panel-repos [data-repo-index-choice]")) {
    const kind = select.dataset.repoIndexChoice;
    const settingUp = !!state.busySetup && state.busyKind === kind;
    const pending = state.pendingRepoIndex && state.pendingRepoIndex.kind === kind;
    select.disabled = settingUp || pending;
    if (settingUp) select.setAttribute("aria-busy", "true");
    else select.removeAttribute("aria-busy");
  }
  for (const toggle of document.querySelectorAll("#panel-repos [data-repo-index-use]")) {
    const kind = toggle.dataset.repoIndexUse;
    const settingUp = !!state.busySetup && state.busyKind === kind;
    const pending = state.pendingRepoIndex && state.pendingRepoIndex.kind === kind;
    toggle.disabled = settingUp || pending;
  }
}

function rememberIndexChoice(kind, tool) {
  if (tool !== "obsidian" && tool !== "codegraph") return;
  state.indexChoice[kind] = tool;
}

function preferredIndexTool(kind) {
  const held = state.indexChoice[kind];
  if (held === "obsidian" || held === "codegraph") return held;
  const item = indexItem(kind);
  const present = item && Array.isArray(item.present) ? item.present : [];
  if (present.includes("codegraph")) return "codegraph";
  if (present.includes("obsidian")) return "obsidian";
  return "obsidian";
}

function shownIndexTool(repo) {
  const saved = repo && (repo.index || "");
  if (saved === "obsidian" || saved === "codegraph") return saved;
  return preferredIndexTool(repo ? repo.kind : "");
}

function indexIsSetup(kind, tool) {
  const item = indexItem(kind);
  return !!item && item.index === tool && !!item.dirExists;
}

function toolIsPresent(kind, tool) {
  const item = indexItem(kind);
  return !!item && Array.isArray(item.present) && item.present.includes(tool);
}

function requestRepoIndex(select) {
  const kind = select.dataset.repoIndexChoice;
  const chosen = select.value;
  const repo = ((state.data && state.data.repos) || []).find((item) => item.kind === kind);
  const saved = repo ? (repo.index || "") : (select.dataset.saved || "");
  const shown = shownIndexTool(repo || { kind, index: saved });
  if (state.busySetup || (state.pendingRepoIndex && state.pendingRepoIndex.kind === kind)) {
    select.value = state.pendingRepoIndex && state.pendingRepoIndex.kind === kind ? state.pendingRepoIndex.tool : shown;
    return;
  }
  if (chosen !== "obsidian" && chosen !== "codegraph") {
    select.value = shown;
    return;
  }
  rememberIndexChoice(kind, chosen);
  const section = select.closest("[data-repo-index]");
  const toggle = section && section.querySelector("[data-repo-index-use]");
  if (!toggle || toggle.getAttribute("aria-pressed") !== "true") return;
  if (chosen === saved && (indexIsSetup(kind, chosen) || toolIsPresent(kind, chosen))) return;
  const request = { kind, tool: chosen };
  state.pendingRepoIndex = request;
  adoptRepoIndex(kind, chosen, saved, request);
}

function toggleRepoIndex(button) {
  const kind = button.dataset.repoIndexUse;
  if (!kind || state.busySetup || state.checkingIndexes.has(kind)) return;
  if (state.pendingRepoIndex && state.pendingRepoIndex.kind === kind) return;
  const section = button.closest("[data-repo-index]");
  const select = section && section.querySelector("[data-repo-index-choice]");
  const tool = select && (select.value === "obsidian" || select.value === "codegraph")
    ? select.value
    : preferredIndexTool(kind);
  const turningOn = button.getAttribute("aria-pressed") !== "true";
  if (!turningOn) {
    button.setAttribute("aria-pressed", "false");
    if (section) section.classList.add("is-off");
    disableRepoIndex(kind);
    return;
  }
  openRepoIndexChoice(kind, tool);
}

function openRepoIndexChoice(kind, tool) {
  const dialog = document.querySelector("#repo-index-choose-dialog");
  dialog.dataset.kind = kind;
  const name = repoLabel[kind] || kind;
  document.querySelector("#repo-index-choose-title").textContent = "Use an index";
  document.querySelector("#repo-index-choose-text").textContent = "Choose the index for " + name + ".";
  const select = document.querySelector("#repo-index-choose-tool");
  select.value = tool === "codegraph" ? "codegraph" : "obsidian";
  syncRepoIndexChoiceNote(kind, select.value);
  dialog.showModal();
}

function syncRepoIndexChoiceNote(kind, tool) {
  const note = document.querySelector("#repo-index-choose-note");
  if (!note) return;
  if (toolIsPresent(kind, tool)) {
    const folder = tool === "obsidian" ? ".obsidian" : ".codegraph";
    note.textContent = folder + " is already in this folder. KIKI will use it.";
    return;
  }
  note.textContent = "KIKI will set " + indexLabel(tool) + " up for this repository.";
}

function applyRepoIndexChoice(kind, tool) {
  if (tool !== "obsidian" && tool !== "codegraph") return;
  rememberIndexChoice(kind, tool);
  const section = document.querySelector(`#panel-repos [data-repo-index="${CSS.escape(kind)}"]`);
  const select = section && section.querySelector("[data-repo-index-choice]");
  if (select) select.value = tool;
  const toggle = section && section.querySelector("[data-repo-index-use]");
  if (toggle) toggle.setAttribute("aria-pressed", "true");
  if (section) section.classList.remove("is-off");
  const repo = ((state.data && state.data.repos) || []).find((item) => item.kind === kind);
  const saved = repo ? (repo.index || "") : "";
  if (tool === saved && (indexIsSetup(kind, tool) || toolIsPresent(kind, tool))) return;
  if (!toolIsPresent(kind, tool)) {
    setupRepoIndex(kind, tool);
    return;
  }
  const request = { kind, tool };
  state.pendingRepoIndex = request;
  adoptRepoIndex(kind, tool, saved, request);
}

async function disableRepoIndex(kind) {
  if (!kind || state.busySetup || state.checkingIndexes.has(kind)) return;
  const repo = ((state.data && state.data.repos) || []).find((item) => item.kind === kind);
  const saved = repo ? (repo.index || "") : "";
  if (saved !== "obsidian" && saved !== "codegraph") {
    paintRepoIndexes();
    return;
  }
  rememberIndexChoice(kind, saved);
  state.pendingRepoIndex = null;
  const toggle = document.querySelector(`#panel-repos [data-repo-index-use="${CSS.escape(kind)}"]`);
  if (toggle) toggle.disabled = true;
  try {
    const data = await api("/api/indexes/" + encodeURIComponent(kind), {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ deleteFolder: false }),
    });
    state.data = data;
    paintRepoIndexes();
    banner("Stopped using " + indexLabel(saved) + " on " + (repoLabel[kind] || kind) + ".");
  } catch (error) {
    banner(error.message, true);
    paintRepoIndexes();
  }
}

function missingIndexFolder(error) {
  return /is not in this repository/.test((error && error.message) || "");
}

async function adoptRepoIndex(kind, tool, saved, request) {
  try {
    const data = await api("/api/indexes/" + encodeURIComponent(kind), {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ index: tool }),
    });
    if (request && state.pendingRepoIndex !== request) return;
    state.pendingRepoIndex = null;
    state.data = data;
    const selected = document.querySelector("[data-tab][aria-selected='true']");
    const tab = (selected && selected.dataset.tab) || "repos";
    render(data);
    showTab(tab);
    banner("Using " + indexLabel(tool) + " already in " + (repoLabel[kind] || kind) + ".");
  } catch (error) {
    if (request && state.pendingRepoIndex !== request) return;
    state.pendingRepoIndex = null;
    if (missingIndexFolder(error)) {
      openRepoIndexConfirm(kind, tool, saved || "");
      return;
    }
    banner(error.message, true);
    paintRepoIndexes();
  }
}

function openRepoIndexConfirm(kind, tool, saved) {
  const dialog = document.querySelector("#repo-index-dialog");
  dialog.dataset.kind = kind;
  dialog.dataset.tool = tool;
  dialog.dataset.previous = saved || "";
  const name = repoLabel[kind] || kind;
  document.querySelector("#repo-index-dialog-title").textContent = "Set up " + indexLabel(tool);
  const text = "Set up " + indexLabel(tool) + " for " + name + ".";
  document.querySelector("#repo-index-dialog-text").textContent = saved && saved !== tool
    ? text + " This replaces " + indexLabel(saved) + "."
    : text;
  dialog.showModal();
}

function revertRepoIndexChoice(kind, saved) {
  if (saved === "obsidian" || saved === "codegraph") rememberIndexChoice(kind, saved);
  const select = document.querySelector(`#panel-repos [data-repo-index-choice="${CSS.escape(kind)}"]`);
  const section = select && select.closest("[data-repo-index]");
  const repo = ((state.data && state.data.repos) || []).find((item) => item.kind === kind);
  if (section && repo) paintRepoIndex(section, repo);
}

const savingFolders = new Set();

async function applyRepoFolder(kind, folder) {
  const input = document.querySelector(`#panel-repos [data-repo-folder="${CSS.escape(kind)}"]`);
  folder = (folder || "").trim();
  const saved = input ? (input.dataset.saved || "") : "";
  if (folder === saved) return;
  if (!folder) {
    if (input) input.value = saved;
    return;
  }
  if (savingFolders.has(kind)) return;
  savingFolders.add(kind);
  if (input) {
    input.value = folder;
    input.dataset.saved = folder;
  }
  try {
    const data = await api("/api/repos/" + encodeURIComponent(kind) + "/folder", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ folder }),
    });
    const selected = document.querySelector("[data-tab][aria-selected='true']");
    const tab = (selected && selected.dataset.tab) || "repos";
    render(data);
    showTab(tab);
    banner("Pointed " + (repoLabel[kind] || kind) + " at " + folder + ".");
  } catch (error) {
    banner(error.message, true);
    if (input && input.isConnected) {
      input.dataset.saved = saved;
      input.value = saved;
    }
  } finally {
    savingFolders.delete(kind);
  }
}

async function browseRepoFolder(kind) {
  const button = document.querySelector(`#panel-repos [data-browse-folder="${CSS.escape(kind)}"]`);
  if (button) button.disabled = true;
  try {
    const picked = await api("/api/repos/browse", { method: "POST" });
    if (picked.path) await applyRepoFolder(kind, picked.path);
  } catch (error) {
    if (error.message !== "Folder selection was cancelled.") banner(error.message, true);
  } finally {
    const current = document.querySelector(`#panel-repos [data-browse-folder="${CSS.escape(kind)}"]`);
    if (current) current.disabled = false;
  }
}

function refreshIcon() {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const add = (d) => {
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", d);
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "currentColor");
    path.setAttribute("stroke-width", "1.7");
    path.setAttribute("stroke-linecap", "round");
    path.setAttribute("stroke-linejoin", "round");
    svg.append(path);
  };
  add("M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8");
  add("M3 3v5h5");
  add("M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16");
  add("M16 16h5v5");
  return svg;
}

function indexRemoveButton(item) {
  const remove = el("button", "icon-button danger");
  remove.type = "button";
  remove.dataset.clearIndex = item.kind;
  const label = "Remove " + (repoLabel[item.kind] || item.kind);
  remove.setAttribute("aria-label", label);
  remove.title = label;
  remove.append(trashIcon());
  return remove;
}

function trashIcon() {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  for (const d of [
    "M3 6h18",
    "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2",
    "M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6",
    "M10 11v6",
    "M14 11v6",
  ]) {
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", d);
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "currentColor");
    path.setAttribute("stroke-width", "1.7");
    path.setAttribute("stroke-linecap", "round");
    path.setAttribute("stroke-linejoin", "round");
    svg.append(path);
  }
  return svg;
}

function badgeClass(item) {
  if (isFolderCheck(item)) return item.dirExists ? "ok" : "bad";
  if (!item.checked) return item.dirExists ? "wait" : "bad";
  return item.ready ? "ok" : "bad";
}

function badgeText(item) {
  const present = item.index === "obsidian" ? "Vault exists" : "Index folder exists";
  const missing = item.index === "obsidian" ? "Vault missing" : "Index folder missing";
  if (isFolderCheck(item)) return item.dirExists ? present : missing;
  if (!item.checked) return item.dirExists ? present : "Not set up";
  return item.ready ? "Ready" : "Not ready";
}

function isFolderCheck(item) {
  const summary = item.summary || "";
  return summary === ".obsidian exists" || summary === ".obsidian is missing" || summary === ".codegraph exists" || summary === ".codegraph is missing";
}

let workflowEdges = [];
let workflowLoad = 0;
const workflowObserver = new ResizeObserver(() => layoutWorkflow());

function agentTeamwork(agent) {
  if (Array.isArray(agent.teamwork)) return agent.teamwork;
  return (agent.worksWith || []).map((id) => ({ id, collaboration: "" }));
}

function agentRepositoryUses(agent) {
  if (Array.isArray(agent.repositoryUses)) return agent.repositoryUses;
  return (agent.repos || []).map((kind) => ({ kind, description: "" }));
}

function workflowRepo(kind) {
  return (state.data.repos || []).find((repo) => repo.kind === kind) || null;
}

function workflowChoice(agent) {
  const tool = toolLabel(agent.tool);
  if (!tool && !agent.model) return "";
  return tool + " · " + modelLabel(agent.tool, agent.model);
}

function workflowIndexLink(url, className) {
  const link = document.createElement("a");
  link.className = className;
  link.href = url;
  link.dataset.url = url;
  const parts = String(url).split("/");
  parts.forEach((part, index) => {
    if (index) link.append("/");
    if (part) link.append(part);
    if (index < parts.length - 1) link.append(document.createElement("wbr"));
  });
  return link;
}

function workflowRemote(url) {
  if (!url) return null;
  const node = el("span", "workflow-path");
  node.append(el("span", "workflow-path-label", "Git"));
  const parts = String(url).split("/");
  parts.forEach((part, index) => {
    if (index) node.append("/");
    if (part) node.append(part);
    if (index < parts.length - 1) node.append(document.createElement("wbr"));
  });
  return node;
}

function workflowPath(folder) {
  const node = el("span", "workflow-path");
  if (!folder) {
    node.textContent = "Folder is not set";
    return node;
  }
  const parts = String(folder).split("/");
  parts.forEach((part, index) => {
    if (index) node.append("/");
    if (part) node.append(part);
    if (index < parts.length - 1) node.append(document.createElement("wbr"));
  });
  return node;
}

function workflowRepoOrder(data) {
  const kinds = workflowRepoKinds(data);
  const sum = new Map();
  const count = new Map();
  (data.agents || []).forEach((agent, index) => {
    for (const use of agentRepositoryUses(agent)) {
      if (!kinds.includes(use.kind)) continue;
      sum.set(use.kind, (sum.get(use.kind) || 0) + index);
      count.set(use.kind, (count.get(use.kind) || 0) + 1);
    }
  });
  return kinds.slice().sort((a, b) => {
    const left = count.has(a) ? sum.get(a) / count.get(a) : kinds.length + kinds.indexOf(a);
    const right = count.has(b) ? sum.get(b) / count.get(b) : kinds.length + kinds.indexOf(b);
    return left - right || kinds.indexOf(a) - kinds.indexOf(b);
  });
}

function workflowRepoKinds(data) {
  const kinds = [];
  const seen = new Set();
  const add = (kind) => {
    if (!kind || seen.has(kind)) return;
    seen.add(kind);
    kinds.push(kind);
  };
  for (const repo of data.repos || []) add(repo.kind);
  for (const agent of data.agents || []) {
    for (const use of agentRepositoryUses(agent)) add(use.kind);
  }
  return kinds;
}

function agentTitle(id) {
  const agent = (state.data.agents || []).find((item) => item.id === id);
  return agent ? agent.title : id;
}

function renderWorkflow(data) {
  const panel = document.querySelector("#panel-workflow");
  state.workflowSelection = null;
  workflowEdges = [];
  panel.replaceChildren();
  panel.append(el("p", "hint", "Select an agent. Lines go through the guardrail to the MCP servers, tools, repositories, and agents that agent may use."));
  const legend = el("div", "workflow-legend");
  legend.append(
    workflowSwatch("guardrail", "Guardrail"),
    workflowSwatch("mcp", "MCP Servers"),
    workflowSwatch("tool", "Tool"),
    workflowSwatch("repository", "Repository"),
    workflowSwatch("teamwork", "Agents Coordination"),
    workflowSwatch("index", "Index"),
  );
  panel.append(legend);
  const scroll = el("div", "workflow-scroll");
  const stage = el("div", "workflow-stage");
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("workflow-lines");
  svg.setAttribute("aria-hidden", "true");
  const cols = el("div", "workflow-cols");
  cols.append(
    workflowAgentColumn(data),
    workflowGuardrailColumn(),
    workflowUseColumn(data, "mcp", "MCP Servers", "No agent lists an MCP server."),
    workflowUseColumn(data, "tool", "Tools", "No agent lists a tool."),
    workflowRepoColumn(data),
    workflowIndexColumn(data),
  );
  stage.append(svg, cols);
  scroll.append(stage);
  panel.append(scroll);
  const detail = el("div", "card workflow-detail");
  detail.id = "workflow-detail";
  panel.append(detail);
  workflowObserver.disconnect();
  workflowObserver.observe(stage);
  paintWorkflowDetail();
  if (!panel.hidden) requestAnimationFrame(layoutWorkflow);
}

function workflowSwatch(kind, label) {
  const item = el("span", "workflow-swatch");
  const mark = el("i", "workflow-swatch-mark " + kind);
  mark.setAttribute("aria-hidden", "true");
  item.append(mark, document.createTextNode(label));
  return item;
}

function workflowAgentColumn(data) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", "Agents"));
  const agents = data.agents || [];
  if (!agents.length) {
    col.append(el("p", "hint", "No agents yet."));
    return col;
  }
  for (const group of groups(agents)) {
    col.append(el("p", "workflow-group", categoryLabel(group.name)));
    for (const agent of group.agents) {
      const card = el("div", "workflow-node" + (agent.enabled === false ? " is-off" : ""));
      card.dataset.flow = "agent";
      card.dataset.id = agent.id;
      const open = el("button", "workflow-open");
      open.type = "button";
      open.setAttribute("aria-pressed", "false");
      const row = el("span", "workflow-name-row");
      const title = el("span", "workflow-title-line");
      title.append(el("span", "workflow-name", agent.title));
      if (agent.enabled === false) title.append(el("span", "badge bad", "Off"));
      row.append(title);
      open.append(row, el("span", "id", agent.id));
      const choice = workflowChoice(agent);
      if (choice) open.append(el("span", "workflow-meta", choice));
      const prompt = el("button", "icon-button workflow-prompt");
      prompt.type = "button";
      prompt.dataset.prompt = agent.id;
      prompt.setAttribute("aria-label", "Show the prompt for " + agent.title);
      prompt.append(promptIcon());
      card.append(open, prompt);
      col.append(card);
      for (const peer of agentTeamwork(agent)) {
        if (!peer.id || peer.id === agent.id) continue;
        workflowEdges.push({
          kind: "teamwork",
          fromType: "agent",
          fromId: agent.id,
          toType: "agent",
          toId: peer.id,
          fromKey: "agent:" + agent.id,
          toKey: "agent:" + peer.id,
          fromSide: "left",
          toSide: "left",
          kicker: "Agents Coordination",
          title: agent.title + " → " + agentTitle(peer.id),
          note: peer.collaboration || "",
        });
      }
      for (const use of agentRepositoryUses(agent)) {
        if (!use.kind) continue;
        workflowEdges.push({
          kind: "repository",
          fromType: "agent",
          fromId: agent.id,
          toType: "repo",
          toId: use.kind,
          fromKey: "agent:" + agent.id,
          toKey: "repo:" + use.kind,
          fromSide: "right",
          toSide: "left",
          kicker: "Repository",
          title: agent.title + " → " + (repoLabel[use.kind] || use.kind),
          note: use.description || "",
        });
      }
      pushWorkflowUses(agent, "mcp", configuredUses("mcp", agent.mcps || []));
      pushWorkflowUses(agent, "tool", agent.tools || []);
    }
  }
  return col;
}

function pushWorkflowUses(agent, kind, items) {
  const kicker = kind === "mcp" ? "MCP Servers" : "Tool";
  for (const item of items) {
    if (!item.id) continue;
    workflowEdges.push({
      kind,
      fromType: "agent",
      fromId: agent.id,
      toType: kind,
      toId: item.id,
      fromKey: "agent:" + agent.id,
      toKey: kind + ":" + item.id,
      fromSide: "right",
      toSide: "left",
      kicker,
      title: agent.title + " → " + workflowUseName(kind, item.id),
      note: kind === "tool" ? toolDetail(item.id) : (item.description || ""),
    });
  }
}

function toolDetail(id) {
  const choice = ((state.data && state.data.toolChoices) || []).find((item) => item.id === id);
  return (choice && choice.detail) || "";
}

function workflowCard(kind, id) {
  const card = el("div", "workflow-node");
  card.dataset.flow = kind;
  card.dataset.id = id;
  const open = el("button", "workflow-open");
  open.type = "button";
  open.setAttribute("aria-pressed", "false");
  card.append(open);
  return card;
}

function workflowAboutButton(kind, id, name) {
  const view = el("button", "icon-button workflow-prompt");
  view.type = "button";
  view.dataset.flowAbout = kind;
  view.dataset.flowId = id;
  view.setAttribute("aria-label", "Show what " + name + " is for");
  view.append(promptIcon());
  return view;
}

function workflowUseColumn(data, kind, title, empty) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", title));
  const ids = workflowUseOrder(data, kind);
  if (!ids.length) {
    col.append(el("p", "hint", empty));
    return col;
  }
  for (const id of ids) {
    const name = workflowUseName(kind, id);
    const card = workflowCard(kind, id);
    const open = card.querySelector(".workflow-open");
    open.append(el("span", "workflow-name", name));
    open.append(el("span", "id", id));
    card.append(workflowAboutButton(kind, id, name));
    col.append(card);
  }
  return col;
}

function workflowUseOrder(data, kind) {
  const ids = [];
  const seen = new Set();
  const sum = new Map();
  const count = new Map();
  (data.agents || []).forEach((agent, index) => {
    const items = kind === "mcp" ? configuredUses("mcp", agent.mcps) : agent.tools;
    for (const item of items || []) {
      if (!item.id || seen.has(item.id)) {
        if (item.id) {
          sum.set(item.id, (sum.get(item.id) || 0) + index);
          count.set(item.id, (count.get(item.id) || 0) + 1);
        }
        continue;
      }
      seen.add(item.id);
      ids.push(item.id);
      sum.set(item.id, (sum.get(item.id) || 0) + index);
      count.set(item.id, (count.get(item.id) || 0) + 1);
    }
  });
  return ids.sort((a, b) => sum.get(a) / count.get(a) - sum.get(b) / count.get(b));
}

function workflowGuardrailColumn() {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", "Guardrail"));
  const slot = el("div", "workflow-guardrail-slot");
  const card = el("div", "workflow-node workflow-guardrail");
  card.dataset.flow = "guardrail";
  card.dataset.id = "guardrail";
  const open = el("button", "workflow-open");
  open.type = "button";
  open.setAttribute("aria-pressed", "false");
  open.append(el("span", "workflow-name", "KIKI guardrail"));
  open.append(el("span", "workflow-meta", "Before input and after output"));
  const view = el("button", "icon-button workflow-prompt");
  view.type = "button";
  view.dataset.guardrailView = "1";
  view.setAttribute("aria-label", "Show what the guardrail checks");
  view.append(promptIcon());
  card.append(open, view);
  slot.append(card);
  col.append(slot);
  return col;
}

function configuredUses(kind, items) {
  const list = items || [];
  if (kind !== "mcp") return list;
  const offered = new Set(((state.data && state.data.mcps) || []).map((item) => item.id));
  return list.filter((item) => item && offered.has(item.id));
}

function workflowUseName(kind, id) {
  const catalog = kind === "mcp" ? (state.data.mcps || []) : (state.data.tools || []);
  const found = catalog.find((item) => item.id === id);
  return (found && found.name) || id;
}

function workflowRepoColumn(data) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", "Repositories"));
  const kinds = workflowRepoOrder(data);
  if (!kinds.length) {
    col.append(el("p", "hint", "No repositories yet."));
    return col;
  }
  for (const kind of kinds) {
    const repo = workflowRepo(kind);
    const name = repoLabel[kind] || kind;
    const card = workflowCard("repo", kind);
    const open = card.querySelector(".workflow-open");
    open.append(el("span", "workflow-name", name));
    const indexName = repo && repo.index ? indexLabel(repo.index) : "No index";
    open.append(el("span", "workflow-meta", indexName));
    card.append(workflowAboutButton("repo", kind, name));
    col.append(card);
    const index = repo && repo.index ? repo.index : "none";
    workflowEdges.push({
      kind: "index",
      fromType: "repo",
      fromId: kind,
      toType: "index",
      toId: index,
      fromKey: "repo:" + kind,
      toKey: "index:" + index,
      fromSide: "right",
      toSide: "left",
      kicker: "Index",
      title: (repoLabel[kind] || kind) + " → " + (index === "none" ? "Not set" : indexLabel(index)),
      note: index === "none"
        ? "No index is saved for this repository."
        : "The launch prompt tells the model to query " + indexLabel(index) + " for this repository.",
    });
  }
  return col;
}

function workflowIndexColumn(data) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", "Indexes"));
  const ids = [];
  const seen = new Set();
  const sum = new Map();
  const count = new Map();
  workflowRepoOrder(data).forEach((kind, index) => {
    const repo = workflowRepo(kind);
    const id = repo && repo.index ? repo.index : "none";
    sum.set(id, (sum.get(id) || 0) + index);
    count.set(id, (count.get(id) || 0) + 1);
    if (seen.has(id)) return;
    seen.add(id);
    ids.push(id);
  });
  ids.sort((a, b) => sum.get(a) / count.get(a) - sum.get(b) / count.get(b));
  if (!ids.length) {
    col.append(el("p", "hint", "No index is saved."));
    return col;
  }
  for (const id of ids) {
    const card = el("div", "workflow-node");
    card.dataset.flow = "index";
    card.dataset.id = id;
    const name = id === "none" ? "Not set" : indexLabel(id);
    const open = el("button", "workflow-open");
    open.type = "button";
    open.setAttribute("aria-pressed", "false");
    open.append(el("span", "workflow-name", name));
    card.append(open, workflowAboutButton("index", id, name));
    const url = indexToolURL[id];
    if (url) card.append(workflowIndexLink(url, "index-tool-link workflow-index-link"));
    col.append(card);
  }
  return col;
}

function layoutWorkflow() {
  const stage = document.querySelector(".workflow-stage");
  const svg = stage && stage.querySelector(".workflow-lines");
  if (!svg || stage.getClientRects().length === 0) return;
  placeGuardrail(stage);
  svg.replaceChildren(workflowMarkers());
  if (state.workflowSelection) {
    const show = workflowRelated(state.workflowSelection);
    const linked = new Set();
    const fan = {};
    workflowEdges.forEach((edge, index) => {
      if (!show.edges.has(index)) return;
      if (edge.kind === "repository" || edge.kind === "teamwork" || edge.kind === "mcp" || edge.kind === "tool") {
        drawThroughGuardrail(stage, svg, edge, index, linked, fan);
        return;
      }
      drawWorkflowEdge(stage, svg, edge, index, true);
    });
  }
  paintWorkflowHighlight();
}

function placeGuardrail(stage) {
  const card = stage.querySelector(".workflow-guardrail");
  if (!card) return;
  const sel = state.workflowSelection;
  let anchorId = "";
  if (sel && sel.kind === "agent") anchorId = sel.id;
  if (sel && sel.kind === "edge") {
    const edge = workflowEdges[sel.id];
    if (edge && edge.fromType === "agent") anchorId = edge.fromId;
  }
  const anchor = anchorId
    ? stage.querySelector(`.workflow-node[data-flow="agent"][data-id="${CSS.escape(anchorId)}"]`)
    : null;
  if (!anchor) {
    card.style.marginTop = "0px";
    return;
  }
  const slotTop = card.parentElement.getBoundingClientRect().top;
  const anchorBox = anchor.getBoundingClientRect();
  const cardBox = card.getBoundingClientRect();
  const margin = anchorBox.top + anchorBox.height / 2 - slotTop - cardBox.height / 2;
  card.style.marginTop = Math.max(0, margin) + "px";
}

function drawThroughGuardrail(stage, svg, edge, index, linked, fan) {
  const guard = stage.querySelector('.workflow-node[data-flow="guardrail"]');
  const from = stage.querySelector(`.workflow-node[data-flow="${edge.fromType}"][data-id="${CSS.escape(edge.fromId)}"]`);
  const to = stage.querySelector(`.workflow-node[data-flow="${edge.toType}"][data-id="${CSS.escape(edge.toId)}"]`);
  if (from && guard && !linked.has(edge.fromId)) {
    linked.add(edge.fromId);
    drawCurve(svg, workflowAnchor(stage, from, "right"), workflowAnchor(stage, guard, "left"), "guardrail", -1, false);
  }
  if (!guard || !to) return;
  const back = edge.kind === "teamwork";
  const start = workflowAnchor(stage, guard, back ? "left" : "right");
  const end = workflowAnchor(stage, to, back ? "right" : "left");
  const n = fan[edge.fromId] || 0;
  fan[edge.fromId] = n + 1;
  drawCurve(svg, start, end, edge.kind, index, true, (n - 1) * 14);
}

function drawWorkflowEdge(stage, svg, edge, index, hittable) {
  const from = stage.querySelector(`.workflow-node[data-flow="${edge.fromType}"][data-id="${CSS.escape(edge.fromId)}"]`);
  const to = stage.querySelector(`.workflow-node[data-flow="${edge.toType}"][data-id="${CSS.escape(edge.toId)}"]`);
  if (!from || !to) return;
  drawCurve(svg, workflowAnchor(stage, from, edge.fromSide), workflowAnchor(stage, to, edge.toSide), edge.kind, index, hittable, 0);
}

function drawCurve(svg, start, end, kind, index, hittable, bend = 0) {
  const svgNS = "http://www.w3.org/2000/svg";
  const c1 = start.x + (end.x - start.x) * 0.45;
  const c2 = end.x - (end.x - start.x) * 0.45;
  const d = `M ${start.x} ${start.y} C ${c1} ${start.y + bend}, ${c2} ${end.y + bend}, ${end.x} ${end.y}`;
  const path = document.createElementNS(svgNS, "path");
  path.setAttribute("d", d);
  path.setAttribute("class", "workflow-edge " + kind);
  if (index >= 0) path.dataset.edge = String(index);
  path.setAttribute("marker-end", "url(#workflow-arrow-" + kind + ")");
  svg.append(path);
  if (!hittable || index < 0) return;
  const hit = document.createElementNS(svgNS, "path");
  hit.setAttribute("d", d);
  hit.setAttribute("class", "workflow-hit");
  hit.dataset.edge = String(index);
  svg.append(hit);
}

function workflowAnchor(stage, node, side) {
  const origin = stage.getBoundingClientRect();
  const rect = node.getBoundingClientRect();
  return {
    x: (side === "left" ? rect.left : rect.right) - origin.left,
    y: rect.top + rect.height / 2 - origin.top,
  };
}

function workflowMarkers() {
  const svgNS = "http://www.w3.org/2000/svg";
  const defs = document.createElementNS(svgNS, "defs");
  for (const [kind, color] of [
    ["guardrail", "var(--accent)"],
    ["teamwork", "var(--accent)"],
    ["mcp", "var(--mcp)"],
    ["tool", "var(--tool)"],
    ["repository", "var(--ink)"],
    ["index", "var(--warn)"],
  ]) {
    const marker = document.createElementNS(svgNS, "marker");
    marker.id = "workflow-arrow-" + kind;
    marker.setAttribute("viewBox", "0 0 10 10");
    marker.setAttribute("refX", "8");
    marker.setAttribute("refY", "5");
    marker.setAttribute("markerWidth", "7");
    marker.setAttribute("markerHeight", "7");
    marker.setAttribute("orient", "auto");
    const path = document.createElementNS(svgNS, "path");
    path.setAttribute("d", "M 0 1 L 9 5 L 0 9 z");
    path.setAttribute("fill", color);
    marker.append(path);
    defs.append(marker);
  }
  return defs;
}

function selectWorkflow(next) {
  const current = state.workflowSelection;
  const same = current && next && current.kind === next.kind && String(current.id) === String(next.id);
  state.workflowSelection = same ? null : next;
  applyWorkflowSelection();
}

function workflowRelated(sel) {
  const nodes = new Set();
  const edges = new Set();
  if (!sel) return { nodes, edges };
  if (sel.kind === "edge") {
    const edge = workflowEdges[sel.id];
    if (!edge) return { nodes, edges };
    edges.add(sel.id);
    nodes.add(edge.fromKey);
    nodes.add(edge.toKey);
    if (edge.kind === "repository" || edge.kind === "teamwork" || edge.kind === "mcp" || edge.kind === "tool") nodes.add("guardrail:guardrail");
    return { nodes, edges };
  }
  if (sel.kind === "guardrail") {
    nodes.add("guardrail:guardrail");
    return { nodes, edges };
  }
  if (sel.kind === "agent") {
    const key = "agent:" + sel.id;
    nodes.add(key);
    nodes.add("guardrail:guardrail");
    const repos = new Set();
    workflowEdges.forEach((edge, index) => {
      if (edge.kind === "teamwork" && edge.fromKey === key) {
        edges.add(index);
        nodes.add(edge.fromKey);
        nodes.add(edge.toKey);
      }
      if (edge.kind === "repository" && edge.fromKey === key) {
        edges.add(index);
        nodes.add(edge.toKey);
        repos.add(edge.toKey);
      }
      if ((edge.kind === "mcp" || edge.kind === "tool") && edge.fromKey === key) {
        edges.add(index);
        nodes.add(edge.toKey);
      }
    });
    workflowEdges.forEach((edge, index) => {
      if (edge.kind === "index" && repos.has(edge.fromKey)) {
        edges.add(index);
        nodes.add(edge.toKey);
      }
    });
    return { nodes, edges };
  }
  if (sel.kind === "repo") {
    const key = "repo:" + sel.id;
    nodes.add(key);
    nodes.add("guardrail:guardrail");
    workflowEdges.forEach((edge, index) => {
      if (edge.fromKey === key || edge.toKey === key) {
        edges.add(index);
        nodes.add(edge.fromKey);
        nodes.add(edge.toKey);
      }
    });
    return { nodes, edges };
  }
  if (sel.kind === "mcp" || sel.kind === "tool") {
    const key = sel.kind + ":" + sel.id;
    nodes.add(key);
    nodes.add("guardrail:guardrail");
    workflowEdges.forEach((edge, index) => {
      if (edge.fromKey === key || edge.toKey === key) {
        edges.add(index);
        nodes.add(edge.fromKey);
        nodes.add(edge.toKey);
      }
    });
    return { nodes, edges };
  }
  if (sel.kind === "index") {
    const key = "index:" + sel.id;
    nodes.add(key);
    nodes.add("guardrail:guardrail");
    const repos = new Set();
    workflowEdges.forEach((edge, index) => {
      if (edge.kind === "index" && edge.toKey === key) {
        edges.add(index);
        nodes.add(edge.fromKey);
        repos.add(edge.fromKey);
      }
    });
    workflowEdges.forEach((edge, index) => {
      if (edge.kind === "repository" && repos.has(edge.toKey)) {
        edges.add(index);
        nodes.add(edge.fromKey);
      }
    });
  }
  return { nodes, edges };
}

function applyWorkflowSelection() {
  layoutWorkflow();
  paintWorkflowDetail();
}

function paintWorkflowHighlight() {
  const panel = document.querySelector("#panel-workflow");
  if (!panel) return;
  const related = workflowRelated(state.workflowSelection);
  const active = state.workflowSelection;
  for (const node of panel.querySelectorAll(".workflow-node")) {
    const key = node.dataset.flow + ":" + node.dataset.id;
    const on = related.nodes.has(key);
    const clicked = !!active && active.kind === node.dataset.flow && String(active.id) === node.dataset.id;
    const endpoint = !!active && active.kind === "edge" && on;
    node.classList.toggle("is-selected", clicked || endpoint);
    node.classList.toggle("is-dim", !!active && !on);
    const press = node.querySelector(".workflow-open") || node;
    press.setAttribute("aria-pressed", clicked ? "true" : "false");
  }
  for (const path of panel.querySelectorAll(".workflow-edge")) {
    const on = path.classList.contains("guardrail") || related.edges.has(Number(path.dataset.edge));
    path.classList.toggle("is-selected", !!active && active.kind === "edge" && Number(path.dataset.edge) === active.id);
    path.classList.toggle("is-dim", !!active && !on);
  }
  paintGuardrailCard();
}

function paintGuardrailCard() {
  const card = document.querySelector('.workflow-node[data-flow="guardrail"] .workflow-meta');
  if (!card) return;
  card.textContent = "Before input and after output";
}

function paintWorkflowDetail() {
  const box = document.querySelector("#workflow-detail");
  if (!box) return;
  box.replaceChildren();
  const sel = state.workflowSelection;
  if (!sel) {
    box.append(el("p", "hint", "Select an agent. The guardrail is what KIKI checks before input and after output."));
    return;
  }
  if (sel.kind === "guardrail") {
    box.append(el("p", "workflow-kicker", "Guardrail"));
    box.append(el("h3", "", "KIKI guardrail"));
    box.append(el("p", "hint", "Before a prompt is sent to the model, and after the model output, KIKI checks the agent. It allows only the MCP servers, tools, repositories, and agents set for that agent. A repository marked Readonly may be read and must not be changed. The check is code in the app. When KIKI is not running, the AI tool is not blocked."));
    return;
  }
  if (sel.kind === "edge") {
    const edge = workflowEdges[sel.id];
    if (!edge) {
      box.append(el("p", "hint", "Select an agent, a repository, an index, or a line."));
      return;
    }
    box.append(el("p", "workflow-kicker", edge.kicker));
    box.append(el("h3", "", edge.title));
    box.append(el("p", "workflow-note", edge.note || "No note is saved."));
    return;
  }
  if (sel.kind === "agent") paintAgentWorkflow(box, sel.id);
  else if (sel.kind === "repo") paintRepoWorkflow(box, sel.id);
  else if (sel.kind === "mcp" || sel.kind === "tool") paintUseWorkflow(box, sel.kind, sel.id);
  else if (sel.kind === "index") paintIndexWorkflow(box, sel.id);
}

function paintAgentWorkflow(box, id) {
  const agent = (state.data.agents || []).find((item) => item.id === id);
  if (!agent) return;
  box.append(el("p", "workflow-kicker", categoryLabel(agent.group)));
  const head = el("div", "workflow-name-row");
  head.append(el("h3", "", agent.title));
  if (agent.enabled === false) head.append(el("span", "badge bad", "Off"));
  box.append(head);
  box.append(el("p", "id", agent.id));
  const choice = workflowChoice(agent);
  if (choice) box.append(el("p", "workflow-meta", choice));
  if (agent.summary) box.append(el("p", "hint", agent.summary));
  box.append(el("h4", "", "Guardrail"));
  box.append(el("p", "hint", "Before input and after output, KIKI allows only the MCP servers, tools, repositories, and agents listed here."));
  paintUseList(box, "MCP", configuredUses("mcp", agent.mcps || []), "mcp");
  paintUseList(box, "Tools", agent.tools || [], "tool");
  const teamwork = agentTeamwork(agent);
  const uses = agentRepositoryUses(agent);
  box.append(el("h4", "", "Agents Coordination"));
  if (!teamwork.length) box.append(el("p", "hint", "This agent does not list another agent."));
  for (const peer of teamwork) {
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", agent.title + " → " + agentTitle(peer.id)));
    block.append(el("p", "workflow-note", peer.collaboration || "No note is saved."));
    box.append(block);
  }
  box.append(el("h4", "", "Repositories"));
  if (!uses.length) box.append(el("p", "hint", "This agent does not list a repository."));
  for (const use of uses) {
    const repo = workflowRepo(use.kind);
    const index = repo && repo.index ? indexLabel(repo.index) : "not set";
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", (repoLabel[use.kind] || use.kind) + " · " + index));
    block.append(el("p", "workflow-note", use.description || "No note is saved."));
    box.append(block);
  }
}

function paintUseList(box, title, items, kind) {
  box.append(el("h4", "", title));
  if (!items.length) {
    box.append(el("p", "hint", "None set."));
    return;
  }
  for (const item of items) {
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", workflowUseName(kind, item.id)));
    block.append(el("p", "workflow-note", kind === "tool" ? (toolDetail(item.id) || "KIKI writes this tool into the launch prompt.") : (item.description || "No note is saved.")));
    box.append(block);
  }
}

function paintUseWorkflow(box, kind, id) {
  box.append(el("p", "workflow-kicker", kind === "mcp" ? "MCP Servers" : "Tool"));
  box.append(el("h3", "", workflowUseName(kind, id)));
  box.append(el("p", "id", id));
  box.append(el("h4", "", "Used by"));
  const users = (state.data.agents || []).filter((agent) => {
    const items = kind === "mcp" ? configuredUses("mcp", agent.mcps) : agent.tools;
    return (items || []).some((item) => item.id === id);
  });
  if (!users.length) {
    box.append(el("p", "hint", "No agent lists this."));
    return;
  }
  for (const agent of users) {
    const items = kind === "mcp" ? configuredUses("mcp", agent.mcps) : agent.tools;
    const item = (items || []).find((entry) => entry.id === id);
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", agent.title));
    const note = kind === "tool" ? toolDetail(id) : (item && item.description);
    block.append(el("p", "workflow-note", note || (kind === "tool" ? "KIKI writes this tool into the launch prompt." : "No note is saved.")));
    box.append(block);
  }
}

function paintRepoWorkflow(box, kind) {
  const repo = workflowRepo(kind);
  box.append(el("p", "workflow-kicker", "Repository"));
  box.append(el("h3", "", repoLabel[kind] || kind));
  box.append(workflowPath(repo && repo.folder));
  const remote = workflowRemote(repo && repo.git);
  if (remote) box.append(remote);
  box.append(el("p", "hint", "Index: " + (repo && repo.index ? indexLabel(repo.index) : "not set")));
  box.append(el("h4", "", "Used by"));
  const users = (state.data.agents || []).filter((agent) => agentRepositoryUses(agent).some((use) => use.kind === kind));
  if (!users.length) box.append(el("p", "hint", "No agent lists this repository."));
  for (const agent of users) {
    const use = agentRepositoryUses(agent).find((item) => item.kind === kind);
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", agent.title));
    block.append(el("p", "workflow-note", (use && use.description) || "No note is saved."));
    box.append(block);
  }
}

function paintIndexWorkflow(box, id) {
  box.append(el("p", "workflow-kicker", "Index"));
  box.append(el("h3", "", id === "none" ? "Not set" : indexLabel(id)));
  const url = indexToolURL[id];
  if (url) box.append(workflowIndexLink(url, "index-tool-link"));
  box.append(el("h4", "", "Repositories"));
  const kinds = workflowRepoKinds(state.data).filter((kind) => {
    const repo = workflowRepo(kind);
    return (repo && repo.index ? repo.index : "none") === id;
  });
  if (!kinds.length) box.append(el("p", "hint", "No repository uses this index."));
  for (const kind of kinds) {
    const repo = workflowRepo(kind);
    const users = (state.data.agents || []).filter((agent) => agentRepositoryUses(agent).some((use) => use.kind === kind));
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", repoLabel[kind] || kind));
    block.append(workflowPath(repo && repo.folder));
    if (users.length) block.append(el("p", "hint", "Agents: " + users.map((agent) => agent.title).join(", ")));
    box.append(block);
  }
}

document.querySelector("#panel-workflow").addEventListener("click", (event) => {
  if (event.target.closest("[data-prompt], [data-guardrail-view], [data-flow-about], .index-tool-link")) return;
  const edge = event.target.closest(".workflow-hit");
  if (edge) {
    selectWorkflow({ kind: "edge", id: Number(edge.dataset.edge) });
    return;
  }
  const node = event.target.closest(".workflow-node");
  if (node) {
    selectWorkflow({ kind: node.dataset.flow, id: node.dataset.id });
    return;
  }
  if (event.target.closest("#workflow-detail")) return;
  selectWorkflow(null);
});

function unlockNavigation() {
  for (const button of document.querySelectorAll("[data-tab='repos'], [data-tab='workflow'], #open-settings, #start-feature")) {
    button.disabled = false;
  }
}

function showTab(name) {
  if (!state.data && name !== "agents") return;
  const enterRepos = name === "repos" && state.tab !== "repos";
  const enterWorkflow = name === "workflow" && state.tab !== "workflow";
  const enterAgents = name === "agents" && state.tab !== "agents";
  state.tab = name;
  for (const button of document.querySelectorAll("[data-tab]")) {
    button.setAttribute("aria-selected", button.dataset.tab === name ? "true" : "false");
  }
  document.querySelector("#panel-agents").hidden = name !== "agents";
  document.querySelector("#panel-repos").hidden = name !== "repos";
  document.querySelector("#panel-workflow").hidden = name !== "workflow";
  if (enterRepos) checkIndexes();
  if (enterAgents) refreshTeamShare();
  if (name === "workflow") requestAnimationFrame(layoutWorkflow);
  if (enterWorkflow) refreshWorkflow();
}

async function refreshWorkflow() {
  const token = ++workflowLoad;
  try {
    const data = await api("/api/state");
    if (token !== workflowLoad || state.tab !== "workflow") return;
    render(data);
  } catch (error) {
    if (token !== workflowLoad || state.tab !== "workflow") return;
    banner(error.message, true);
  }
}

function collect() {
  const body = {
    workspaceId: document.querySelector("#workspace-id").value,
    orchestrator: document.querySelector("#orchestrator").value,
    generateTool: document.querySelector("#generate-tool").value,
    generateModel: document.querySelector("#generate-model").value,
    repos: state.data.repos.map((repo) => ({
      kind: repo.kind,
      folder: repo.folder,
      services: repo.kind === "document"
        ? []
        : [...document.querySelectorAll(`[data-services="${repo.kind}"] button[aria-pressed="true"]`)].map((item) => item.dataset.name),
    })),
  };
  const dialog = document.querySelector("#settings-dialog");
  if (dialog && dialog.open) {
    const atlassian = collectAtlassian();
    if (atlassian) body.atlassian = atlassian;
    const tools = collectTools();
    if (tools) body.tools = tools;
  }
  return body;
}

function agentById(id) {
  return state.data.agents.find((agent) => agent.id === id);
}

function modelsFor(tool) {
  return (state.data && state.data.models && state.data.models[tool]) || [];
}

function defaultModel(tool) {
  const models = modelsFor(tool);
  const found = models.find((item) => item.default) || models[0];
  return found ? found.id : "";
}

function promptIcon() {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const add = (d) => {
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", d);
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "currentColor");
    path.setAttribute("stroke-width", "1.7");
    path.setAttribute("stroke-linejoin", "round");
    path.setAttribute("stroke-linecap", "round");
    svg.append(path);
  };
  add("M7 3.5h7.5L19 8v12.5H7z");
  add("M14.5 3.5V8H19");
  add("M9.5 12.5h5M9.5 16h5");
  return svg;
}

function installedTools() {
  const data = state.data || {};
  if (Array.isArray(data.installedAgents)) return data.installedAgents;
  return data.orcaAgents || [];
}

function usableTool(tool) {
  const tools = installedTools();
  if (tools.includes(tool)) return tool;
  return tools[0] || "";
}

function fillToolSelect(select, selected) {
  select.replaceChildren();
  const installed = new Set(installedTools());
  const names = (state.data && state.data.orcaAgents) || [];
  const ready = names.filter((name) => installed.has(name));
  const missing = names.filter((name) => !installed.has(name));
  if (ready.length) select.append(toolGroup("Installed", ready, selected, false));
  if (missing.length) select.append(toolGroup("Not installed", missing, selected, true));
}

function toolGroup(label, names, selected, disabled) {
  const group = document.createElement("optgroup");
  group.label = label;
  for (const name of names) {
    const option = el("option", "", toolLabel(name));
    option.value = name;
    option.disabled = disabled;
    if (!disabled && name === selected) option.selected = true;
    group.append(option);
  }
  return group;
}

function choiceFor(agentId) {
  const agent = agentById(agentId);
  const tool = usableTool(agent && agent.tool);
  let model = agent && agent.model ? agent.model : "";
  if (!modelsFor(tool).some((item) => item.id === model)) model = defaultModel(tool);
  return { tool, model };
}

function fillModels(select, tool, model) {
  select.replaceChildren();
  const items = modelsFor(tool);
  const order = [];
  const groups = new Map();
  for (const item of items) {
    const name = item.group || "";
    if (!groups.has(name)) {
      groups.set(name, []);
      order.push(name);
    }
    groups.get(name).push(item);
  }
  order.sort((a, b) => {
    const rank = (name) => (name === "Cursor Models" ? 0 : name === "Other Models" ? 1 : name === "" ? 2 : 3);
    return rank(a) - rank(b);
  });
  const add = (parent, item) => {
    const option = el("option", "", item.label);
    option.value = item.id;
    if (item.id === model) option.selected = true;
    parent.append(option);
  };
  for (const name of order) {
    const parent = name ? el("optgroup") : select;
    if (name) {
      parent.label = name;
      select.append(parent);
    }
    for (const item of groups.get(name)) add(parent, item);
  }
}

async function refreshModels(select, tool) {
  const token = String(Date.now()) + Math.random();
  select.dataset.modelsToken = token;
  const title = select.closest(".field").querySelector("span");
  let spin = title.querySelector(".spinner");
  if (!spin) {
    spin = el("span", "spinner");
    spin.setAttribute("aria-hidden", "true");
    title.append(spin);
  }
  select.disabled = true;
  select.setAttribute("aria-busy", "true");
  select.replaceChildren();
  const waiting = el("option", "", "Loading…");
  waiting.value = "";
  waiting.selected = true;
  select.append(waiting);
  try {
    const data = await api("/api/tools/" + encodeURIComponent(tool) + "/models");
    if (select.dataset.modelsToken !== token) return false;
    if (!state.data.models) state.data.models = {};
    state.data.models[tool] = data.models || [];
    const keep = (data.models || []).some((item) => item.id === select.dataset.previousModel)
      ? select.dataset.previousModel
      : defaultModel(tool);
    fillModels(select, tool, keep);
    return true;
  } catch (error) {
    if (select.dataset.modelsToken !== token) return false;
    fillModels(select, tool, defaultModel(tool));
    banner(error.message, true);
    return false;
  } finally {
    if (select.dataset.modelsToken === token) {
      select.disabled = false;
      select.removeAttribute("aria-busy");
      spin.remove();
    }
  }
}

function agentPicks(agentId) {
  const choice = choiceFor(agentId);
  const picks = el("div", "agent-picks");
  const toolField = el("label", "field");
  toolField.append(el("span", "", "AI Tool"));
  const toolSelect = el("select");
  toolSelect.dataset.tool = agentId;
  toolSelect.setAttribute("aria-label", "AI Tool");
  fillToolSelect(toolSelect, choice.tool);
  toolField.append(toolSelect);
  const modelField = el("label", "field");
  modelField.append(el("span", "", "Model"));
  const modelSelect = el("select");
  modelSelect.dataset.model = agentId;
  modelSelect.setAttribute("aria-label", "Model");
  fillModels(modelSelect, choice.tool, choice.model);
  modelField.append(modelSelect);
  picks.append(toolField, modelField);
  return picks;
}

const cardChoiceSave = {};

async function saveCardChoice(agentId, toolSelect, modelSelect) {
  const token = (cardChoiceSave[agentId] || 0) + 1;
  cardChoiceSave[agentId] = token;
  const previous = choiceFor(agentId);
  const tool = toolSelect.value;
  const model = modelSelect.value;
  toolSelect.disabled = true;
  modelSelect.disabled = true;
  try {
    const current = await api("/api/agents/" + encodeURIComponent(agentId));
    if (cardChoiceSave[agentId] !== token) return;
    const data = await api("/api/agents/" + encodeURIComponent(agentId), {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        id: current.id,
        name: current.name,
        description: current.description,
        category: current.category,
        enabled: current.enabled !== false,
        tool,
        model,
        repos: current.repos,
        worksWith: current.worksWith,
        prompt: current.prompt,
      }),
    });
    if (cardChoiceSave[agentId] !== token) return;
    const selected = document.querySelector("[data-tab][aria-selected='true']");
    render(data);
    showTab((selected && selected.dataset.tab) || "agents");
  } catch (error) {
    if (cardChoiceSave[agentId] !== token) return;
    fillToolSelect(toolSelect, previous.tool);
    fillModels(modelSelect, previous.tool, previous.model);
    toolSelect.disabled = false;
    modelSelect.disabled = false;
    banner(error.message, true);
  }
}

function launchPayload() {
  const choice = choiceFor(state.agentId);
  return JSON.stringify({
    note: document.querySelector("#launch-note").value,
    repo: selectedRepo(),
    agent: choice.tool,
    model: choice.model,
  });
}

function openLaunch(id) {
  state.agentId = id;
  const agent = agentById(id);
  document.querySelector("#launch-title").textContent = agent.title;
  const choice = choiceFor(id);
  const model = modelsFor(choice.tool).find((item) => item.id === choice.model);
  document.querySelector("#launch-meta").textContent =
    agent.id + " · " + toolLabel(choice.tool) + " · " + (model ? model.label : choice.model);
  const repos = document.querySelector("#launch-repos");
  repos.replaceChildren();
  const select = document.querySelector("#launch-repo");
  select.replaceChildren();
  for (const kind of agent.repos) {
    const repo = state.data.repos.find((item) => item.kind === kind);
    const option = el("option", "", repoLabel[kind] || kind);
    option.value = kind;
    select.append(option);
    const line = el("p", "");
    const folder = repo && repo.folder ? repo.folder : "(no folder)";
    const services = repo && repo.services.length ? repo.services.join(", ") : "(none)";
    line.textContent = kind === "document"
      ? (repoLabel[kind] || kind) + ": " + folder
      : (repoLabel[kind] || kind) + ": " + folder + " · services: " + services;
    repos.append(line);
  }
  showWorktree();
  document.querySelector("#launch-note").value = "";
  const prompt = document.querySelector("#launch-prompt");
  prompt.hidden = true;
  prompt.textContent = "Open this to load the prompt.";
  document.querySelector("#prompt-toggle").setAttribute("aria-expanded", "false");
  document.querySelector("#prompt-toggle").textContent = "Show the prompt";
  state.promptHtml = "";
  document.querySelector("#launch-result").hidden = true;
  const go = document.querySelector("#launch-go");
  go.disabled = false;
  go.textContent = "Start on " + orchestratorLabel();
  document.querySelector("#launch-dialog").showModal();
}

function selectedRepo() {
  const select = document.querySelector("#launch-repo");
  return select ? select.value : "";
}

function showWorktree() {
  const kind = selectedRepo();
  const repo = state.data.repos.find((item) => item.kind === kind);
  const node = document.querySelector("#launch-worktree");
  if (!repo || !repo.folder) {
    node.textContent = "This repository has no folder. Set it on the Repositories tab before starting.";
    return;
  }
  node.textContent = orchestratorLabel() + " starts in " + repo.folder;
}

async function showPrompt(reveal = true) {
  const view = document.querySelector("#launch-prompt");
  if (reveal) {
    view.hidden = false;
    view.textContent = "Loading…";
  }
  try {
    const data = await api("/api/agents/" + state.agentId + "/preview", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: launchPayload(),
    });
    state.promptHtml = renderPrompt(data.command, data.prompt);
    if (!view.hidden) view.innerHTML = state.promptHtml;
    const reader = document.querySelector("#prompt-reader-body");
    if (document.querySelector("#prompt-reader").open) reader.innerHTML = state.promptHtml;
  } catch (error) {
    state.promptHtml = "";
    if (reveal || !view.hidden) {
      view.hidden = false;
      view.textContent = error.message;
    } else {
      banner(error.message, true);
    }
  }
}

function escapeHtml(text) {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function inlineMarkdown(text) {
  let html = escapeHtml(text);
  html = html.replace(/`([^`]+)`/g, "<code>$1</code>");
  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  html = html.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2">$1</a>');
  return html;
}

function isTableStart(lines, index) {
  if (!lines[index].includes("|") || index + 1 >= lines.length) return false;
  return /^\s*\|?\s*:?-{3,}/.test(lines[index + 1]);
}

function renderTable(rows) {
  const cells = (row) => row.split("|").map((cell) => cell.trim()).filter((cell, index, all) => !(cell === "" && (index === 0 || index === all.length - 1)));
  const head = cells(rows[0]);
  const body = rows.slice(2).map(cells);
  const html = ["<table><thead><tr>"];
  for (const cell of head) html.push("<th>" + inlineMarkdown(cell) + "</th>");
  html.push("</tr></thead><tbody>");
  for (const row of body) {
    html.push("<tr>");
    for (const cell of row) html.push("<td>" + inlineMarkdown(cell) + "</td>");
    html.push("</tr>");
  }
  html.push("</tbody></table>");
  return html.join("");
}

function isBlockLine(lines, index) {
  const line = lines[index];
  if (line.trim().startsWith("```")) return true;
  if (/^#{1,6}\s/.test(line)) return true;
  if (/^\s*([-*]|\d+\.)\s+/.test(line)) return true;
  if (/^---+$/.test(line.trim())) return true;
  return isTableStart(lines, index);
}

function renderMarkdown(source) {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const out = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (line.trim().startsWith("```")) {
      const buf = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith("```")) {
        buf.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) index += 1;
      out.push("<pre><code>" + escapeHtml(buf.join("\n")) + "</code></pre>");
      continue;
    }
    if (isTableStart(lines, index)) {
      const rows = [];
      while (index < lines.length && lines[index].includes("|")) {
        rows.push(lines[index]);
        index += 1;
      }
      out.push(renderTable(rows));
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      out.push("<h" + level + ">" + inlineMarkdown(heading[2]) + "</h" + level + ">");
      index += 1;
      continue;
    }
    if (/^---+$/.test(line.trim())) {
      out.push("<hr>");
      index += 1;
      continue;
    }
    if (/^\s*([-*]|\d+\.)\s+/.test(line)) {
      const ordered = /^\s*\d+\.\s+/.test(line);
      const items = [];
      while (index < lines.length && /^\s*([-*]|\d+\.)\s+/.test(lines[index])) {
        items.push(lines[index].replace(/^\s*([-*]|\d+\.)\s+/, ""));
        index += 1;
      }
      const tag = ordered ? "ol" : "ul";
      out.push("<" + tag + ">" + items.map((item) => "<li>" + inlineMarkdown(item) + "</li>").join("") + "</" + tag + ">");
      continue;
    }
    if (line.trim() === "") {
      index += 1;
      continue;
    }
    const para = [line];
    index += 1;
    while (index < lines.length && lines[index].trim() !== "" && !isBlockLine(lines, index)) {
      para.push(lines[index]);
      index += 1;
    }
    out.push("<p>" + inlineMarkdown(para.join(" ")) + "</p>");
  }
  return out.join("");
}

function renderPrompt(command, prompt) {
  return '<p class="prompt-label">Command</p><pre class="command">' + escapeHtml(command) + "</pre>" + renderMarkdown(prompt);
}

function guardrailText() {
  return [
    "The hook runs before a prompt is sent to the model and after the model output. That covers a tool, an MCP server, a shell command, a file read or edit, and another agent.",
    "",
    "KIKI decides while it is running. A folder outside this workspace's repositories is allowed. Turning Guardrails off in Settings removes the hooks from Cursor, Claude, Codex, and Gemini.",
    "",
    "## Which agent",
    "",
    "KIKI uses the agent named for the hook when that agent may use the folder. Otherwise it uses the agent it started in this worktree. Otherwise it uses the only enabled agent whose repositories include the folder.",
    "",
    "A KIKI repository with no agent set up for it is blocked. When more than one agent includes the folder and KIKI cannot tell which one is running, the action stays blocked until that agent is started from KIKI.",
    "",
    "## What it allows",
    "",
    "For that agent, each interaction has to match the agent setup.",
    "",
    "- **MCP.** Only the MCP servers set on that agent. Jira and Confluence are checked on their own.",
    "- **Tools.** Only the tools set on that agent and turned on in Settings. `gh` is GitHub CLI. `rtk` is allowed for every agent while RTK is on, and blocked while RTK is off.",
    "- **Agents.** The agent may continue as itself, and may start an agent listed under Agents Coordination.",
    "- **Repositories.** A path stays inside a repository set for that agent, including a worktree KIKI started for it. A repository marked Readonly may be read and must not be changed. File edits, commits, and other writes in that folder or its worktree are blocked. Reads stay allowed. Another repository on the same agent stays writable when it is not marked Readonly.",
    "- **Prompt and reply.** A prompt or model message with no MCP server, tool, repository path, or other agent is allowed.",
  ].join("\n");
}

function openGuardrail() {
  document.querySelector("#prompt-reader-title").textContent = "KIKI guardrail";
  document.querySelector("#prompt-reader-body").innerHTML = renderMarkdown(guardrailText());
  document.querySelector("#prompt-reader").showModal();
}

function flowAbout(kind, id) {
  if (kind === "mcp") return mcpAbout(id);
  if (kind === "tool") return toolAbout(id);
  if (kind === "repo") return repoAbout(id);
  return indexAbout(id);
}

function mcpAbout(id) {
  const name = workflowUseName("mcp", id);
  const text = {
    atlassian_jira: "This MCP server is how an agent works with Jira. The agent searches, reads, and updates issues on the site saved in Settings. KIKI adds the Atlassian server to the agent's AI tool before a run when that tool does not already have one. An agent can call Jira only when Jira is turned on for that agent.",
    atlassian_confluence: "This MCP server is how an agent works with Confluence. The agent reads pages in the space saved in Settings. KIKI adds the Atlassian server to the agent's AI tool before a run when that tool does not already have one. An agent can call Confluence only when Confluence is turned on for that agent.",
  }[id] || "This is an MCP server an agent can call. KIKI adds it to the agent's AI tool before a run when that tool does not already have one.";
  return { title: name, text };
}

function toolAbout(id) {
  const choice = ((state.data && state.data.toolChoices) || []).find((item) => item.id === id);
  const name = (choice && choice.name) || workflowUseName("tool", id);
  const lines = [];
  if (choice && choice.detail) lines.push(choice.detail);
  if (choice && choice.command) lines.push("", "Command: `" + choice.command + "`.");
  if (choice && choice.global) lines.push("", "This tool applies to every agent when it is on in Settings.");
  else lines.push("", "An agent uses this tool when it is chosen for that agent and turned on in Settings. The launch prompt includes this description.");
  return { title: name, text: lines.join("\n"), url: choice && choice.page };
}

function repoAbout(kind) {
  const name = repoLabel[kind] || kind;
  const repo = workflowRepo(kind);
  const folder = repo && repo.folder ? repo.folder : "The folder is not set.";
  const index = repo && repo.index ? indexLabel(repo.index) : "not set";
  let purpose = "This repository is a folder agents can open. Each agent that lists it receives the folder in the launch prompt.";
  if (kind === "document") purpose = "The Document repository is the folder for notes and requirement files. Agents that list it may read and write here. When the index is Obsidian, the launch prompt tells the agent to search that vault before opening files.";
  else if (kind === "backend" || kind === "website" || kind === "mobile") purpose = "The " + name + " repository is the folder for that part of the app. Agents that list it may read and change code here. When the index is CodeGraph, the launch prompt tells the agent to query that index before reading the repository.";
  return { title: name, text: [purpose, "", "Folder: `" + folder + "`", "", "Index: " + index].join("\n") };
}

function indexAbout(id) {
  if (id === "obsidian") {
    return {
      title: "Obsidian",
      text: "Obsidian indexes a document repository. The launch prompt tells the agent to search the vault and to read a note when search, backlinks, or links name it. The agent does not create the vault.",
      url: indexToolURL.obsidian,
    };
  }
  if (id === "codegraph") {
    return {
      title: "CodeGraph",
      text: "CodeGraph indexes a code repository. The launch prompt tells the agent to query this index before grep, find, or reading the repository. The agent does not run codegraph init.",
      url: indexToolURL.codegraph,
    };
  }
  return { title: "Not set", text: "No index is saved. The launch prompt does not tell the agent to query Obsidian or CodeGraph for the repositories on this card." };
}

function openFlowAbout(kind, id) {
  const about = flowAbout(kind, id);
  document.querySelector("#prompt-reader-title").textContent = about.title;
  const body = document.querySelector("#prompt-reader-body");
  body.innerHTML = renderMarkdown(about.text);
  if (about.url) body.append(workflowIndexLink(about.url, "index-tool-link"));
  document.querySelector("#prompt-reader").showModal();
}

async function openAgentPrompt(id) {
  const agent = agentById(id);
  const choice = choiceFor(id);
  const dialog = document.querySelector("#prompt-reader");
  document.querySelector("#prompt-reader-title").textContent = agent.title + " — prompt sent to " + orchestratorLabel();
  document.querySelector("#prompt-reader-body").textContent = "Loading…";
  dialog.showModal();
  try {
    const data = await api("/api/agents/" + encodeURIComponent(id) + "/preview", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        note: "",
        repo: agent.repos[0],
        agent: choice.tool,
        model: choice.model,
      }),
    });
    document.querySelector("#prompt-reader-body").innerHTML = renderPrompt(data.command, data.prompt);
  } catch (error) {
    dialog.close();
    banner(error.message, true);
  }
}

async function openPromptReader() {
  await showPrompt(false);
  if (!state.promptHtml) return;
  const agent = agentById(state.agentId);
  document.querySelector("#prompt-reader-title").textContent = (agent ? agent.title : "Prompt") + " — prompt sent to " + orchestratorLabel();
  document.querySelector("#prompt-reader-body").innerHTML = state.promptHtml;
  document.querySelector("#prompt-reader").showModal();
}

async function launch() {
  const button = document.querySelector("#launch-go");
  button.disabled = true;
  const result = document.querySelector("#launch-result");
  result.hidden = false;
  result.classList.remove("error");
  const label = orchestratorLabel();
  result.textContent = "Starting on " + label + "…";
  try {
    const data = await api("/api/agents/" + state.agentId + "/launch", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: launchPayload(),
    });
    const launch = data.launch;
    result.classList.toggle("error", launch.status !== "ok");
    result.textContent = launch.status === "ok"
      ? "Started on " + label + ". Detail:\n" + launch.detail
      : launch.detail;
    const fresh = await api("/api/state");
    state.data = fresh;
    renderHistory(fresh);
  } catch (error) {
    result.classList.add("error");
    result.textContent = error.message;
  } finally {
    button.disabled = false;
  }
}

async function save() {
  try {
    const data = await api("/api/setup", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(collect()),
    });
    const selected = document.querySelector("[data-tab][aria-selected='true']");
    const tab = (selected && selected.dataset.tab) || "agents";
    render(data);
    showTab(tab);
    banner("Saved " + data.setupFile + ".");
  } catch (error) {
    banner(error.message, true);
  }
}

function syncTeamName() {
  const path = document.querySelector("#team-path");
  const user = document.querySelector("#team-user");
  const note = document.querySelector("#team-user-error");
  if (!path || !user || !note) return;
  const hasPath = path.value.trim() !== "";
  const choices = document.querySelector("#team-name-choices");
  const mode = teamNameMode();
  const member = document.querySelector("#team-member-name");
  const typing = !choices || mode === "new";
  user.disabled = !!choices && !typing;
  user.required = hasPath && typing;
  user.setAttribute("aria-required", hasPath && typing ? "true" : "false");
  if (member) {
    member.disabled = mode !== "existing";
    member.hidden = mode !== "existing";
  }
  user.hidden = !!choices && mode !== "new";
  const missing = hasPath && currentTeamUser() === "";
  user.setAttribute("aria-invalid", missing && typing ? "true" : "false");
  if (member) member.setAttribute("aria-invalid", missing && mode === "existing" ? "true" : "false");
  const target = choices || user.closest(".field");
  if (target) target.classList.toggle("is-invalid", missing);
  note.textContent = settingsFieldRequiredMsg;
  note.hidden = !missing;
}

function settingsFieldWrap(input) {
  if (!input) return null;
  return input.closest(".field") || input.closest(".team-name") || input.closest(".choice-block");
}

function ensureFieldError(wrap) {
  if (!wrap) return null;
  let note = wrap.querySelector(":scope > .field-error");
  if (!note && wrap.classList.contains("team-name")) note = wrap.querySelector(".field-error");
  if (!note) {
    note = el("p", "field-error", "");
    wrap.append(note);
  }
  return note;
}

function markFieldInvalid(input, message) {
  if (!input) return;
  const wrap = settingsFieldWrap(input);
  if (wrap) wrap.classList.add("is-invalid");
  input.setAttribute("aria-invalid", "true");
  const note = ensureFieldError(wrap);
  if (note) {
    note.textContent = message || settingsFieldRequiredMsg;
    note.hidden = false;
  }
}

function clearFieldValidation(input) {
  if (!input || !input.isConnected) return;
  input.setAttribute("aria-invalid", "false");
  const wrap = settingsFieldWrap(input);
  if (!wrap || !wrap.classList.contains("is-invalid")) return;
  wrap.classList.remove("is-invalid");
  const note = wrap.querySelector(":scope > .field-error") || (wrap.classList.contains("team-name") ? wrap.querySelector(".field-error") : null);
  if (note) note.hidden = true;
}

function clearSettingsValidation() {
  const root = document.querySelector("#settings-fields");
  if (!root) return;
  for (const wrap of root.querySelectorAll(".is-invalid")) wrap.classList.remove("is-invalid");
  for (const note of root.querySelectorAll(".field-error")) note.hidden = true;
  for (const input of root.querySelectorAll("[aria-invalid='true']")) input.setAttribute("aria-invalid", "false");
}

function mcpCredentialReady(username, token, tokenSaved) {
  return username !== "" && (token !== "" || tokenSaved);
}

function validHttpUrl(raw) {
  try {
    const parsed = new URL(raw);
    if (parsed.username || parsed.password) return false;
    return (parsed.protocol === "http:" || parsed.protocol === "https:") && parsed.host !== "";
  } catch {
    return false;
  }
}

function validateSettingsForm() {
  clearSettingsValidation();
  let ok = true;
  let focusEl = null;

  const workspace = document.querySelector("#workspace-id");
  const workspaceValue = workspace ? workspace.value.trim() : "";
  if (!workspacePattern.test(workspaceValue)) {
    markFieldInvalid(workspace, workspaceValue ? "Use lowercase letters, digits, hyphens, or underscores." : settingsFieldRequiredMsg);
    ok = false;
    focusEl = workspace;
  }

  const teamPathInput = document.querySelector("#team-path");
  const teamPath = teamPathInput ? teamPathInput.value.trim() : "";
  if (teamPath && !currentTeamUser()) {
    syncTeamName();
    ok = false;
    const choosingExisting = teamNameMode() === "existing";
    focusEl = focusEl || document.querySelector(choosingExisting ? "#team-member-name" : "#team-user");
  }

  if (mcpPressed("#mcp-atlassian")) {
    const saved = (state.data && state.data.atlassian) || {};
    const jiraOn = mcpPressed("#mcp-jira");
    const confluenceOn = mcpPressed("#mcp-confluence");
    if (!jiraOn && !confluenceOn) {
      const card = document.querySelector("#settings-fields .mcp-card");
      if (card) {
        const note = ensureFieldError(card);
        note.textContent = "Turn on Jira or Confluence.";
        note.hidden = false;
      }
      ok = false;
    }
    const sharedNeeded = (jiraOn && !mcpChecked("#mcp-jira-own")) || (confluenceOn && !mcpChecked("#mcp-confluence-own"));
    const sharedUser = fieldValue("#mcp-username");
    const sharedToken = fieldValue("#mcp-api-token");
    if (sharedNeeded && !mcpCredentialReady(sharedUser, sharedToken, !!saved.apiTokenSet)) {
      if (!sharedUser) {
        markFieldInvalid(document.querySelector("#mcp-username"));
        ok = false;
        focusEl = focusEl || document.querySelector("#mcp-username");
      }
      if (!sharedToken && !saved.apiTokenSet) {
        markFieldInvalid(document.querySelector("#mcp-api-token"));
        ok = false;
        focusEl = focusEl || document.querySelector("#mcp-api-token");
      }
    }
    for (const name of ["jira", "confluence"]) {
      if (!mcpPressed("#mcp-" + name)) continue;
      const savedService = saved[name] || {};
      const urlInput = document.querySelector("#mcp-" + name + "-url");
      const url = urlInput ? urlInput.value.trim() : "";
      if (!url) {
        markFieldInvalid(urlInput);
        ok = false;
        focusEl = focusEl || urlInput;
      } else if (!validHttpUrl(url)) {
        markFieldInvalid(urlInput, "Use an http or https URL.");
        ok = false;
        focusEl = focusEl || urlInput;
      }
      if (!mcpChecked("#mcp-" + name + "-own")) continue;
      const userInput = document.querySelector("#mcp-" + name + "-username");
      const tokenInput = document.querySelector("#mcp-" + name + "-api-token");
      const user = userInput ? userInput.value.trim() : "";
      const token = tokenInput ? tokenInput.value.trim() : "";
      if (!mcpCredentialReady(user, token, !!savedService.apiTokenSet)) {
        if (!user) {
          markFieldInvalid(userInput);
          ok = false;
          focusEl = focusEl || userInput;
        }
        if (!token && !savedService.apiTokenSet) {
          markFieldInvalid(tokenInput);
          ok = false;
          focusEl = focusEl || tokenInput;
        }
      }
    }
  }

  if (!ok && focusEl) focusEl.focus();
  return ok;
}

async function saveSettings() {
  const button = document.querySelector("#settings-save");
  let teamPath = document.querySelector("#team-path").value.trim();
  if (teamPath && teamPath !== teamListedPath) {
    button.disabled = true;
    button.classList.add("busy");
    try {
      await stageTeamPath(teamPath);
    } catch (error) {
      banner(error.message, true);
      button.classList.remove("busy");
      syncSave();
      return;
    }
    button.classList.remove("busy");
    syncSave();
    const pathInput = document.querySelector("#team-path");
    teamPath = pathInput ? pathInput.value.trim() : "";
  }
  if (!validateSettingsForm()) {
    syncSave();
    return;
  }
  const choosingExisting = teamNameMode() === "existing";
  const teamUser = currentTeamUser();
  const savedTeam = (state.data && state.data.teamFolder) || {};
  const payload = collect();
  const selected = document.querySelector("[data-tab][aria-selected='true']");
  const tab = (selected && selected.dataset.tab) || "agents";
  const pathChanged = teamPath !== (savedTeam.path || "");
  const userChanged = teamUser !== (savedTeam.user || "");
  button.disabled = true;
  button.classList.add("busy");
  let data;
  try {
    if (teamPath && teamPath !== (savedTeam.path || "")) {
      await api("/api/team/members?path=" + encodeURIComponent(teamPath));
    }
    data = await api("/api/setup", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
  } catch (error) {
    banner(error.message, true);
    button.classList.remove("busy");
    syncSave();
    return;
  }
  try {
    if (pathChanged || userChanged) {
      const teamBody = { path: teamPath, user: teamPath ? teamUser : "" };
      if (choosingExisting && teamUser) teamBody.useExisting = true;
      const next = await putTeam(teamBody);
      if (next && next.rename) {
        button.classList.remove("busy");
        const typed = document.querySelector("input[name='team-name-mode'][value='new']");
        if (typed) typed.checked = true;
        syncTeamName();
        syncSave();
        const user = document.querySelector("#team-user");
        if (user) {
          user.focus();
          user.select();
        }
        return;
      }
      if (!next) throw new Error("Team folder was not saved.");
      data = next;
    }
  } catch (error) {
    button.classList.remove("busy");
    render(data);
    showTab(tab);
    banner(error.message, true);
    return;
  }
  button.classList.remove("busy");
  render(data);
  showTab(tab);
  banner("Saved " + data.setupFile + ".");
  const settings = document.querySelector("#settings-dialog");
  if (settings && settings.open) settings.close();
}

async function putTeam(body) {
  try {
    return await api("/api/team", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch (error) {
    if (error.payload && error.payload.conflict === "name") {
      const useExisting = await askExistingName(error.payload.user || body.user);
      if (!useExisting) return { rename: true };
      return putTeam(Object.assign({}, body, { useExisting: true }));
    }
    if (error.payload && error.payload.conflict === "agents") {
      const direction = await askSyncDirection(body.user);
      if (!direction) return null;
      return putTeam(Object.assign({}, body, { direction }));
    }
    if (error.payload && Array.isArray(error.payload.unknown) && error.payload.unknown.length) {
      const repos = await askRepoChoices(error.payload.unknown, error.payload.repos || []);
      if (!repos) return null;
      return putTeam(Object.assign({}, body, { repos }));
    }
    throw error;
  }
}

async function postMember(name, repos, direction) {
  const body = { name, direction: direction || "pull" };
  if (repos) body.repos = repos;
  try {
    return await api("/api/team/member", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch (error) {
    if (direction !== "push" && error.payload && Array.isArray(error.payload.unknown) && error.payload.unknown.length) {
      const choices = await askRepoChoices(error.payload.unknown, error.payload.repos || []);
      if (!choices) return null;
      return postMember(name, choices, direction);
    }
    throw error;
  }
}

function confirmMemberSync(name, direction) {
  const dialog = document.querySelector("#team-member-dialog");
  const push = direction === "push";
  dialog.querySelector("h2").textContent = push ? "Copy this app to " + name : "Copy " + name + " to this app";
  document.querySelector("#team-member-text").textContent = push
    ? "Replace " + name + "'s agents with this app's agents? This overwrites every agent file in that folder."
    : "Replace this app's agents with " + name + "'s agents? This overwrites every agent in this app.";
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      dialog.removeEventListener("close", onClose);
      resolve(value);
    };
    const onClose = () => finish(false);
    dialog.addEventListener("close", onClose);
    document.querySelector("#team-member-replace").onclick = () => {
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish(true);
    };
    dialog.showModal();
  });
}

async function syncFolder(name, direction) {
  if (!name || agentMemberBusy) return;
  const team = (state.data && state.data.teamFolder) || {};
  if (direction === "push" && name !== (team.user || "")) return;
  const confirmed = await confirmMemberSync(name, direction);
  if (!confirmed) return;
  await runAgentCopy(name, direction);
}

async function runAgentCopy(name, direction) {
  if (!name || agentMemberBusy) return;
  const select = document.querySelector("#agent-member");
  const arrows = [...document.querySelectorAll(".team-arrows button")];
  const selectWasDisabled = !!(select && select.disabled);
  agentMemberBusy = true;
  if (select) select.disabled = true;
  for (const button of arrows) {
    button.disabled = true;
    if (button.dataset.folder === name && button.dataset.sync === direction) button.classList.add("busy");
  }
  let copied = null;
  try {
    const team = (state.data && state.data.teamFolder) || {};
    copied = direction === "push" && name === (team.user || "")
      ? await api("/api/team/agents", { method: "POST" })
      : await postMember(name, null, direction);
  } catch (error) {
    banner(error.message, true);
  } finally {
    agentMemberBusy = false;
    if (select && select.isConnected) select.disabled = selectWasDisabled;
    for (const button of arrows) {
      if (button.isConnected) button.classList.remove("busy");
    }
  }
  if (copied) {
    render(copied);
    banner(direction === "push"
      ? "Copied this app's agents into " + name + "'s agents folder."
      : "Copied " + name + "'s agents into this app.");
    return;
  }
  if (select && select.isConnected) loadAgentMembers(select.value).catch((error) => banner(error.message, true));
}

async function chooseMember(select) {
  const name = select.value;
  const previous = select.dataset.kept || "";
  if (name === previous) return;
  if (!name) {
    agentMemberChoice = "";
    select.dataset.kept = "";
    loadAgentMembers("").catch((error) => banner(error.message, true));
    return;
  }
  const confirmed = await confirmMemberSync(name, "pull");
  if (!select.isConnected) return;
  if (!confirmed) {
    select.value = previous;
    return;
  }
  agentMemberChoice = name;
  select.dataset.kept = name;
  await runAgentCopy(name, "pull");
}

function askExistingName(name) {
  const dialog = document.querySelector("#team-name-dialog");
  document.querySelector("#team-name-text").textContent = name + " is already a folder in the team folder. Enter a different name, or use this folder.";
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      dialog.removeEventListener("close", onClose);
      resolve(value);
    };
    const onClose = () => finish(false);
    dialog.addEventListener("close", onClose);
    document.querySelector("#team-name-use").onclick = () => {
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish(true);
    };
    document.querySelector("#team-name-rename").onclick = () => {
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish(false);
    };
    dialog.showModal();
  });
}

function askSyncDirection(user) {
  const dialog = document.querySelector("#team-sync-dialog");
  document.querySelector("#team-sync-text").textContent = user + " already has an agents folder in the team folder. Copy this app's agents into that folder, or copy that folder into this app.";
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      dialog.removeEventListener("close", onClose);
      resolve(value);
    };
    const onClose = () => finish(null);
    dialog.addEventListener("close", onClose);
    document.querySelector("#team-sync-push").onclick = () => {
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish("push");
    };
    document.querySelector("#team-sync-pull").onclick = () => {
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish("pull");
    };
    dialog.showModal();
  });
}

function askRepoChoices(unknown, repos) {
  const dialog = document.querySelector("#team-repo-dialog");
  const box = document.querySelector("#team-repo-choices");
  box.replaceChildren();
  for (const id of unknown) {
    const block = el("fieldset", "choice-block");
    block.append(el("legend", "", id));
    const create = el("input");
    create.type = "radio";
    create.name = "repo-choice-" + id;
    create.value = "create";
    create.checked = true;
    const createRow = el("label", "choice-row");
    createRow.append(create, document.createTextNode("Create repository " + id));
    const use = el("input");
    use.type = "radio";
    use.name = "repo-choice-" + id;
    use.value = "use";
    const select = el("select");
    select.dataset.repoFor = id;
    for (const kind of repos) {
      const option = el("option", "", repoLabel[kind] || kind);
      option.value = kind;
      select.append(option);
    }
    const useRow = el("label", "choice-row");
    useRow.append(use, document.createTextNode("Use"), select);
    block.append(createRow, useRow);
    box.append(block);
  }
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      dialog.removeEventListener("close", onClose);
      resolve(value);
    };
    const onClose = () => finish(null);
    dialog.addEventListener("close", onClose);
    document.querySelector("#team-repo-continue").onclick = () => {
      const choices = {};
      for (const id of unknown) {
        const picked = box.querySelector('input[name="repo-choice-' + id + '"]:checked');
        const select = box.querySelector('select[data-repo-for="' + id + '"]');
        if (picked && picked.value === "use") choices[id] = { action: "use", id: select ? select.value : "" };
        else choices[id] = { action: "create" };
      }
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish(choices);
    };
    dialog.showModal();
  });
}

let timer = 0;
let watchGeneration = 0;

function watchJob(id, which) {
  if (timer) clearInterval(timer);
  const generation = ++watchGeneration;
  let settled = false;
  const tick = async () => {
    if (settled || generation !== watchGeneration) return;
    try {
      const job = await api("/api/jobs/" + id);
      if (settled || generation !== watchGeneration) return;
      if (job.status === "running") return;
      settled = true;
      clearInterval(timer);
      timer = 0;
      const failed = job.status !== "ok";
      try {
        const data = await api("/api/indexes/check", { method: "POST" });
        if (generation !== watchGeneration) return;
        state.data = data;
        paintRepoIndexes();
      } catch (error) {
        if (generation !== watchGeneration) return;
        banner(error.message, true);
      }
      if (generation !== watchGeneration) return;
      markSetupBusy(which, false);
      const message = job.status === "cancelled" ? "Setup cancelled." : failed ? "Setup failed." : "Setup finished.";
      banner(message, failed);
    } catch (error) {
      if (generation !== watchGeneration) return;
      settled = true;
      clearInterval(timer);
      timer = 0;
      markSetupBusy(which, false);
      banner(error.message, true);
    }
  };
  tick();
  timer = setInterval(tick, 1000);
}

function markSetupBusy(which, busy) {
  if (busy) state.busySetup = which;
  else if (state.busySetup === which) {
    state.busySetup = "";
    state.busyKind = "";
  }
  paintRepoSetupButtons();
  paintRepoIndexes();
}

const indexCheckGeneration = {};

function allIndexKeys() {
  return ((state.data && state.data.indexes) || [])
    .filter((item) => item.index === "obsidian" || item.index === "codegraph")
    .map((item) => item.kind);
}

function markIndexCardChecking(card) {
  for (const button of card.querySelectorAll("[data-clear-index]")) button.disabled = true;
  const badge = card.querySelector(".badge");
  if (!badge) return;
  badge.className = "badge checking";
  badge.replaceChildren();
  badge.setAttribute("aria-label", "Checking");
  const spin = el("span", "spinner");
  spin.setAttribute("aria-hidden", "true");
  badge.append(spin);
}

function markIndexesChecking() {
  const repos = document.querySelector("#panel-repos");
  if (!repos) return;
  for (const card of repos.querySelectorAll("[data-repo-index]")) {
    if (!state.checkingIndexes.has(card.dataset.repoIndex)) continue;
    markIndexCardChecking(card);
    const refresh = card.querySelector("[data-refresh-repo]");
    if (!refresh) continue;
    refresh.disabled = true;
    refresh.classList.add("spin");
    refresh.setAttribute("aria-busy", "true");
  }
}

function checkPath(keys) {
  if (keys.length !== 1) return "/api/indexes/check";
  return "/api/indexes/check/" + encodeURIComponent(keys[0]);
}

function applyCheckedIndexes(data, keys) {
  if (!state.data || !Array.isArray(state.data.indexes) || keys.length !== 1) {
    state.data = data;
    paintRepoIndexes();
    return;
  }
  const kind = keys[0];
  const next = (data.indexes || []).find((item) => item.kind === kind);
  if (!next) return;
  const list = state.data.indexes;
  const index = list.findIndex((item) => item.kind === kind);
  if (index >= 0) list[index] = next;
  else list.push(next);
  const repo = (state.data.repos || []).find((item) => item.kind === kind);
  if (repo) repo.index = next.index;
  paintRepoIndexes();
}

function finishIndexCheck(keys, generation) {
  if (!keys.every((key) => indexCheckGeneration[key] === generation[key])) return false;
  for (const key of keys) state.checkingIndexes.delete(key);
  return true;
}

async function checkIndexKeys(keys, options = {}) {
  if (!keys.length) return false;
  const generation = {};
  for (const key of keys) {
    indexCheckGeneration[key] = (indexCheckGeneration[key] || 0) + 1;
    generation[key] = indexCheckGeneration[key];
    state.checkingIndexes.add(key);
  }
  markIndexesChecking();
  try {
    const data = await api(checkPath(keys), { method: "POST" });
    if (!finishIndexCheck(keys, generation)) return false;
    applyCheckedIndexes(data, keys);
    if (options.announce) banner(keys.length === 1 ? "Index checked." : "Indexes checked.");
    return true;
  } catch (error) {
    if (!finishIndexCheck(keys, generation)) return false;
    if (state.data) paintRepoIndexes();
    banner(error.message, true);
    return false;
  }
}

function checkIndexes(options) {
  return checkAllIndexes(options);
}

async function checkAllIndexes(options = {}) {
  const keys = allIndexKeys();
  const generation = {};
  for (const key of keys) {
    indexCheckGeneration[key] = (indexCheckGeneration[key] || 0) + 1;
    generation[key] = indexCheckGeneration[key];
    state.checkingIndexes.add(key);
  }
  markIndexesChecking();
  try {
    const data = await api("/api/indexes/check", { method: "POST" });
    if (keys.length && !finishIndexCheck(keys, generation)) return false;
    state.data = data;
    paintRepoIndexes();
    if (options.announce) banner("Indexes checked.");
    return true;
  } catch (error) {
    if (keys.length && !finishIndexCheck(keys, generation)) return false;
    if (state.data) paintRepoIndexes();
    banner(error.message, true);
    return false;
  }
}

async function openIndexPage(url) {
  try {
    await api("/api/open", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url }),
    });
  } catch (error) {
    banner(error.message, true);
  }
}

function openRemoveIndex(kind) {
  if (!kind || state.busySetup || state.checkingIndexes.has(kind)) return;
  const item = ((state.data && state.data.indexes) || []).find((row) => row.kind === kind);
  if (!item || (item.index !== "obsidian" && item.index !== "codegraph")) return;
  const dialog = document.querySelector("#index-remove-dialog");
  dialog.dataset.kind = kind;
  const folderName = item.index === "obsidian" ? ".obsidian" : ".codegraph";
  const label = repoLabel[kind] || kind;
  document.querySelector("#index-remove-title").textContent = "Remove " + indexLabel(item.index);
  document.querySelector("#index-remove-text").textContent =
    "Remove " + indexLabel(item.index) + " from " + label + ". The launch prompt will no longer tell the model to query this index.";
  const box = document.querySelector("#index-remove-folder");
  box.checked = false;
  const place = item.folder ? item.folder + "/" + folderName : folderName;
  document.querySelector("#index-remove-folder-label").textContent = "Also delete " + place;
  const error = document.querySelector("#index-remove-error");
  error.hidden = true;
  error.textContent = "";
  dialog.showModal();
}

async function confirmRemoveIndex() {
  const dialog = document.querySelector("#index-remove-dialog");
  const kind = dialog.dataset.kind;
  const deleteFolder = document.querySelector("#index-remove-folder").checked;
  const error = document.querySelector("#index-remove-error");
  if (!kind) {
    error.hidden = false;
    error.textContent = "Choose a repository.";
    return;
  }
  error.hidden = true;
  dialog.close();
  await clearIndex(kind, deleteFolder);
}

async function clearIndex(kind, deleteFolder) {
  if (!kind || state.busySetup || state.checkingIndexes.has(kind)) return;
  const buttons = [...document.querySelectorAll(`[data-clear-index="${CSS.escape(kind)}"]`)];
  for (const button of buttons) {
    button.disabled = true;
    button.classList.add("busy");
  }
  try {
    const data = await api("/api/indexes/" + encodeURIComponent(kind), {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ deleteFolder: !!deleteFolder }),
    });
    state.data = data;
    paintRepoIndexes();
    const label = repoLabel[kind] || kind;
    banner(deleteFolder ? "Removed the index from " + label + " and deleted its index folder." : "Removed the index from " + label + ".");
  } catch (error) {
    banner(error.message, true);
    for (const button of buttons) {
      if (!button.isConnected) continue;
      button.disabled = false;
      button.classList.remove("busy");
    }
  }
}

function setupRepoIndex(kind, tool) {
  if (!kind || state.busySetup) return;
  if (tool !== "obsidian" && tool !== "codegraph") return;
  return startSetup(tool, kind);
}

function refreshRepoIndex(kind, options = {}) {
  if (!kind || state.checkingIndexes.has(kind)) return;
  const item = indexItem(kind);
  if (!indexAssigned(item)) return;
  return checkIndexKeys([kind], { announce: options.announce !== false });
}

function refreshAssignedRepoIndexes() {
  for (const kind of allIndexKeys()) refreshRepoIndex(kind, { announce: false });
}

async function startSetup(tool, kind) {
  if (state.busySetup || !kind) return;
  state.busyKind = kind;
  markSetupBusy(tool, true);
  try {
    const path = tool === "obsidian" ? "/api/indexes/obsidian" : "/api/indexes/codegraph";
    const result = await api(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ repo: kind }),
    });
    if (!result || !result.id) {
      if (result && Array.isArray(result.repos)) state.data = result;
      markSetupBusy(tool, false);
      if (result && Array.isArray(result.repos)) {
        render(result);
        banner("Using " + indexLabel(tool) + " already in " + (repoLabel[kind] || kind) + ".");
      }
      return;
    }
    watchJob(result.id, tool);
  } catch (error) {
    markSetupBusy(tool, false);
    banner(error.message, true);
    try {
      const data = await api("/api/state");
      state.data = data;
      paintRepoIndexes();
    } catch (reloadError) {
      banner(reloadError.message, true);
    }
  }
}

function repoList(value) {
  if (!Array.isArray(value)) {
    if (!value) return [];
    return [{ kind: value, description: "", readonly: false }];
  }
  return value.map((item) => {
    if (typeof item === "string") return { kind: item, description: "", readonly: false };
    return {
      kind: item && (item.kind || item.id),
      description: (item && item.description) || "",
      readonly: !!(item && item.readonly),
    };
  }).filter((item) => item.kind);
}

function repoChoices(repos) {
  const kinds = repoKinds.slice();
  for (const kind of repos || []) {
    if (!kinds.includes(kind)) kinds.push(kind);
  }
  return kinds;
}

function rememberRepoNotes() {
  if (!state.repoNotes) state.repoNotes = {};
  if (!state.repoReadonly) state.repoReadonly = {};
  for (const area of document.querySelectorAll("#agent-repo-notes textarea")) {
    state.repoNotes[area.dataset.repo] = area.value;
  }
  for (const box of document.querySelectorAll("#agent-repo-notes [data-repo-readonly]")) {
    state.repoReadonly[box.dataset.repoReadonly] = box.checked;
  }
}

function paintScopeCount(selector, countId) {
  const node = document.querySelector(countId);
  if (!node) return;
  const count = document.querySelectorAll(selector + " button[aria-pressed='true']").length;
  node.textContent = count === 0 ? "None selected" : count === 1 ? "1 selected" : count + " selected";
  node.dataset.empty = count === 0 ? "true" : "false";
}

function renderRepoNotes() {
  rememberRepoNotes();
  const root = document.querySelector("#agent-repo-notes");
  root.replaceChildren();
  const selected = [...document.querySelectorAll("#agent-repos button[aria-pressed='true']")];
  for (const button of selected) {
    const kind = button.dataset.repo;
    const block = el("div", "field");
    const bar = el("div", "prompt-bar");
    const names = el("span", "collab-names");
    names.append(el("span", "collab-title", button.textContent));
    names.append(el("span", "peer-id", kind));
    bar.append(names);
    const toggleWrap = el("span", "preview-toggle");
    toggleWrap.append(el("span", "", "Preview"));
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "switch";
    toggle.dataset.repoPreview = kind;
    toggle.setAttribute("aria-pressed", "false");
    toggle.setAttribute("aria-label", "Preview");
    toggle.append(el("span", "switch-knob"));
    toggleWrap.append(toggle);
    bar.append(toggleWrap);
    block.append(bar);
    const area = document.createElement("textarea");
    area.dataset.repo = kind;
    area.rows = 4;
    area.placeholder = "What this agent does in the " + kind + " repository. The AI Tool receives this with that repository.";
    area.value = state.repoNotes[kind] || "";
    const compose = el("div", "compose");
    compose.append(area, generateButton("repo", kind));
    const preview = el("div", "markdown repo-preview");
    preview.hidden = true;
    const readonly = el("label", "repo-readonly");
    readonly.title = "This agent can read this repository and cannot change it.";
    const box = document.createElement("input");
    box.type = "checkbox";
    box.dataset.repoReadonly = kind;
    box.checked = !!(state.repoReadonly && state.repoReadonly[kind]);
    readonly.append(box, el("span", "", "Readonly"));
    block.append(compose, preview, readonly);
    root.append(block);
  }
  paintScopeCount("#agent-repos", "#agent-repo-count");
}

function fillAgentCategories(selected) {
  const select = document.querySelector("#agent-category");
  select.replaceChildren();
  const names = [];
  for (const agent of state.data.agents) {
    if (agent.group && !names.includes(agent.group)) names.push(agent.group);
  }
  if (selected && !names.includes(selected)) names.push(selected);
  for (const name of names) {
    const option = el("option", "", categoryLabel(name));
    option.value = name;
    if (name === selected) option.selected = true;
    select.append(option);
  }
}

function peerList(value) {
  if (!Array.isArray(value)) {
    if (!value) return [];
    return [{ id: value, collaboration: "" }];
  }
  return value.map((item) => {
    if (typeof item === "string") return { id: item, collaboration: "" };
    return { id: item && item.id, collaboration: (item && item.collaboration) || "" };
  }).filter((item) => item.id);
}

function rememberCollaborations() {
  if (!state.peerNotes) state.peerNotes = {};
  for (const area of document.querySelectorAll("#agent-collaborations textarea")) {
    state.peerNotes[area.dataset.peer] = area.value;
  }
}

function renderCollaborations() {
  rememberCollaborations();
  const root = document.querySelector("#agent-collaborations");
  root.replaceChildren();
  const selected = [...document.querySelectorAll("#agent-works-with button[aria-pressed='true']")];
  for (const button of selected) {
    const id = button.dataset.peer;
    const block = el("div", "field");
    const bar = el("div", "prompt-bar");
    const names = el("span", "collab-names");
    names.append(el("span", "collab-title", button.textContent));
    names.append(el("span", "peer-id", id));
    bar.append(names);
    const toggleWrap = el("span", "preview-toggle");
    toggleWrap.append(el("span", "", "Preview"));
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "switch";
    toggle.dataset.collabPreview = id;
    toggle.setAttribute("aria-pressed", "false");
    toggle.setAttribute("aria-label", "Preview");
    toggle.append(el("span", "switch-knob"));
    toggleWrap.append(toggle);
    bar.append(toggleWrap);
    block.append(bar);
    const area = document.createElement("textarea");
    area.dataset.peer = id;
    area.rows = 4;
    area.placeholder = "How this agent communicates and coordinates with " + id + ". The AI Tool receives this with that agent id.";
    area.value = state.peerNotes[id] || "";
    const compose = el("div", "compose");
    compose.append(area, generateButton("teamwork", id));
    const preview = el("div", "markdown collab-preview");
    preview.hidden = true;
    block.append(compose, preview);
    root.append(block);
  }
  paintScopeCount("#agent-works-with", "#agent-peer-count");
}

function fillAgentPeers(currentId, selectedIds) {
  const root = document.querySelector("#agent-works-with");
  root.replaceChildren();
  document.querySelector("#agent-collaborations").replaceChildren();
  const peers = peerList(selectedIds);
  state.peerNotes = {};
  for (const peer of peers) state.peerNotes[peer.id] = peer.collaboration;
  const chosen = new Set(peers.map((peer) => peer.id));
  const others = state.data.agents.filter((agent) => agent.id !== currentId);
  for (const group of groups(others)) {
    const block = el("div", "peer-group");
    block.append(el("span", "peer-label", categoryLabel(group.name)));
    const tags = el("div", "service-tags");
    for (const agent of group.agents) {
      const button = el("button", "tag", agent.title);
      button.type = "button";
      button.dataset.peer = agent.id;
      button.setAttribute("aria-pressed", chosen.has(agent.id) ? "true" : "false");
      tags.append(button);
    }
    block.append(tags);
    root.append(block);
  }
  for (const id of chosen) {
    if (others.some((agent) => agent.id === id)) continue;
    const button = el("button", "tag", id);
    button.type = "button";
    button.dataset.peer = id;
    button.setAttribute("aria-pressed", "true");
    root.append(button);
  }
}

const useSections = {
  mcp: {
    root: "#agent-mcps",
    notes: "#agent-mcp-notes",
    state: "mcpNotes",
    generate: "mcp",
    count: "#agent-mcp-count",
    empty: "No MCP servers are set up in Settings.",
    placeholder: (name) => "What this agent does with " + name + ". The AI Tool receives this with that MCP server.",
  },
  tool: {
    root: "#agent-tools",
    state: "toolNotes",
    count: "#agent-tool-count",
    empty: "No tools are enabled in Settings.",
  },
};

function useList(value) {
  if (!Array.isArray(value)) {
    if (!value) return [];
    return [{ id: value, description: "" }];
  }
  return value.map((item) => {
    if (typeof item === "string") return { id: item, description: "" };
    return Object.assign(placeFields(item), {
      id: item && item.id,
      description: (item && item.description) || "",
    });
  }).filter((item) => item.id);
}

function globalTool(id) {
  const choice = ((state.data && state.data.toolChoices) || []).find((item) => item.id === id);
  return !!(choice && choice.global) || id === "rtk";
}

function configuredChoices(key) {
  const list = (state.data && state.data[key]) || [];
  return list
    .filter((item) => item && item.id && item.enabled !== false && !item.global)
    .map((item) => ({ id: item.id, name: item.name || item.id }));
}

function fillUses(kind, selected) {
  const spec = useSections[kind];
  const root = document.querySelector(spec.root);
  const notes = spec.notes ? document.querySelector(spec.notes) : null;
  root.replaceChildren();
  if (notes) notes.replaceChildren();
  const choices = configuredChoices(kind === "mcp" ? "mcps" : "toolChoices");
  const items = (kind === "mcp" ? expandLegacyAtlassian(useList(selected), choices) : useList(selected))
    .filter((item) => kind !== "tool" || !globalTool(item.id));
  state[spec.state] = {};
  for (const item of items) state[spec.state][item.id] = item.description;
  const chosen = new Set(items.map((item) => item.id));
  const offered = new Set(choices.map((item) => item.id));
  if (kind === "mcp") {
    state.mcpPlaces = {};
    state.hiddenMcpNotes = {};
    for (const item of items) {
      state.mcpPlaces[item.id] = placeFields(item);
      if (!offered.has(item.id)) state.hiddenMcpNotes[item.id] = Object.assign({ description: item.description || "" }, placeFields(item));
    }
  }
  for (const id of chosen) {
    if (offered.has(id) || kind === "mcp") continue;
    choices.push({ id, name: id, missing: true });
  }
  if (!choices.length) {
    root.classList.remove("tool-switches");
    root.append(el("p", "hint", spec.empty));
    return;
  }
  if (kind === "tool") {
    root.classList.add("tool-switches");
    for (const choice of choices) root.append(toolSwitchRow(choice, chosen.has(choice.id)));
    return;
  }
  root.classList.remove("tool-switches");
  for (const choice of choices) {
    const button = el("button", choice.missing ? "tag missing" : "tag", choice.name);
    button.type = "button";
    button.dataset.use = choice.id;
    button.setAttribute("aria-pressed", chosen.has(choice.id) ? "true" : "false");
    root.append(button);
  }
}

function toolSwitchRow(choice, on) {
  const row = el("div", "agent-tool" + (choice.missing ? " missing" : ""));
  const copy = el("div", "agent-tool-copy");
  copy.append(el("strong", "", choice.name));
  const meta = ((state.data && state.data.toolChoices) || []).find((item) => item.id === choice.id) || {};
  if (meta.detail) copy.append(el("span", "hint", meta.detail));
  if (choice.missing) copy.append(el("span", "hint", "Not enabled in Settings."));
  const button = el("button", "switch");
  button.type = "button";
  button.dataset.use = choice.id;
  button.setAttribute("aria-pressed", on ? "true" : "false");
  button.setAttribute("aria-label", choice.name);
  button.append(el("span", "switch-knob"));
  row.append(copy, button);
  return row;
}

function rememberUseNotes(kind) {
  const spec = useSections[kind];
  if (!state[spec.state]) state[spec.state] = {};
  for (const area of document.querySelectorAll(spec.notes + " textarea")) {
    state[spec.state][area.dataset.use] = area.value;
  }
}

function renderUseNotes(kind) {
  const spec = useSections[kind];
  if (!spec.notes) {
    paintScopeCount(spec.root, spec.count);
    return;
  }
  rememberUseNotes(kind);
  const root = document.querySelector(spec.notes);
  root.replaceChildren();
  const selected = [...document.querySelectorAll(spec.root + " button[aria-pressed='true']")];
  for (const button of selected) {
    const id = button.dataset.use;
    const block = el("div", "field");
    const bar = el("div", "prompt-bar");
    const names = el("span", "collab-names");
    names.append(el("span", "collab-title", button.textContent));
    names.append(el("span", "peer-id", id));
    bar.append(names);
    const toggleWrap = el("span", "preview-toggle");
    toggleWrap.append(el("span", "", "Preview"));
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "switch";
    toggle.dataset.usePreview = kind;
    toggle.setAttribute("aria-pressed", "false");
    toggle.setAttribute("aria-label", "Preview");
    toggle.append(el("span", "switch-knob"));
    toggleWrap.append(toggle);
    bar.append(toggleWrap);
    block.append(bar);
    const fields = kind === "mcp" ? mcpPlaceFields(id) : [];
    for (const field of fields) block.append(field);
    const area = document.createElement("textarea");
    area.dataset.use = id;
    area.rows = 4;
    area.placeholder = spec.placeholder(button.textContent);
    area.value = state[spec.state][id] || "";
    const compose = el("div", "compose");
    compose.append(area, generateButton(spec.generate, button.textContent));
    const preview = el("div", "markdown repo-preview");
    preview.hidden = true;
    block.append(compose, preview);
    if (fields.length) {
      const row = useSettingsRow(id, fields);
      for (const field of fields) field.syncSettings = () => row.sync();
      block.append(row);
    }
    root.append(block);
  }
  paintScopeCount(spec.root, spec.count);
}

function expandLegacyAtlassian(items, choices) {
  const legacy = items.find((item) => item.id === "atlassian");
  if (!legacy) return items;
  const rest = items.filter((item) => item.id !== "atlassian");
  const have = new Set(rest.map((item) => item.id));
  let absorbed = false;
  for (const choice of choices) {
    if (choice.id !== "atlassian_jira" && choice.id !== "atlassian_confluence") continue;
    absorbed = true;
    if (have.has(choice.id)) continue;
    rest.push({ id: choice.id, description: legacy.description || "" });
    have.add(choice.id);
  }
  return absorbed ? rest : items;
}

function selectedUses(kind) {
  const spec = useSections[kind];
  if (spec.notes) rememberUseNotes(kind);
  const picked = [...document.querySelectorAll(spec.root + " button[aria-pressed='true']")].map((button) => {
    const id = button.dataset.use;
    const description = spec.notes ? (state[spec.state][id] || "").trim() : "";
    if (kind !== "mcp") return { id, description };
    return mcpPayload(id, description, state.mcpPlaces && state.mcpPlaces[id]);
  });
  if (kind !== "mcp") return picked;
  const seen = new Set(picked.map((item) => item.id));
  for (const [id, saved] of Object.entries(state.hiddenMcpNotes || {})) {
    if (!id || seen.has(id)) continue;
    const record = saved && typeof saved === "object" ? saved : { description: saved };
    picked.push(mcpPayload(id, record.description, record));
  }
  return picked;
}

const placeKeys = ["boardId", "boardName", "spaceId", "spaceKey", "spaceName", "parentId", "parentTitle"];

function placeFields(item) {
  const source = item || {};
  const place = {};
  for (const key of placeKeys) place[key] = source[key] || "";
  return place;
}

function mcpPayload(id, description, place) {
  return Object.assign({ id, description: String(description || "").trim() }, placeFields(place));
}

async function fetchPlaces(url) {
  const data = await api(url);
  return data.items || [];
}

function sharedPlace(id) {
  const cfg = (state.data && state.data.atlassian) || {};
  if (id === "atlassian_jira") return placeFields(cfg.jira);
  if (id === "atlassian_confluence") return placeFields(cfg.confluence);
  return placeFields();
}

function mcpOverrideLabels(agent) {
  const labels = [];
  for (const item of (agent && agent.mcps) || []) labels.push(...placeOverrideLabels(item));
  return labels;
}

function placeOverrideLabels(item) {
  if (!item) return [];
  const own = placeFields(item);
  const shared = sharedPlace(item.id);
  if (item.id === "atlassian_jira") {
    if (!jiraDiffers(own, shared)) return [];
    return [{ text: "Diff Jira", title: "This agent's Jira board differs from Settings." }];
  }
  if (item.id !== "atlassian_confluence" || !confluenceDiffers(own, shared)) return [];
  return [{ text: "Diff Confluence", title: "This agent's Confluence space or parent page differs from Settings." }];
}

function jiraDiffers(own, shared) {
  if (!own.boardId && !own.boardName) return false;
  return !samePlaceValue(own.boardId, own.boardName, shared.boardId, shared.boardName);
}

function confluenceSpaceDiffers(own, shared) {
  if (!own.spaceId && !own.spaceName) return false;
  return !samePlaceValue(own.spaceId, own.spaceName, shared.spaceId, shared.spaceName);
}

function confluenceParentDiffers(own, shared) {
  if (!own.spaceId && !own.spaceName) return false;
  return !samePlaceValue(own.parentId, own.parentTitle, shared.parentId, shared.parentTitle);
}

function confluenceDiffers(own, shared) {
  return confluenceSpaceDiffers(own, shared) || confluenceParentDiffers(own, shared);
}

function samePlaceValue(id, name, sharedId, sharedName) {
  const left = String(id || "").trim();
  const right = String(sharedId || "").trim();
  if (left || right) return left === right;
  return String(name || "").trim() === String(sharedName || "").trim();
}

function shownPlace(id) {
  const own = placeFields(state.mcpPlaces && state.mcpPlaces[id]);
  const shared = sharedPlace(id);
  if (id === "atlassian_jira") {
    if (own.boardId || own.boardName) return Object.assign(placeFields(), own, { source: "agent" });
    return Object.assign(placeFields(), { boardId: shared.boardId, boardName: shared.boardName, source: shared.boardId || shared.boardName ? "settings" : "" });
  }
  if (own.spaceId || own.spaceName) return Object.assign(placeFields(), own, { source: "agent" });
  return Object.assign(placeFields(), {
    spaceId: shared.spaceId,
    spaceKey: shared.spaceKey,
    spaceName: shared.spaceName,
    parentId: shared.parentId,
    parentTitle: shared.parentTitle,
    source: shared.spaceId || shared.spaceName ? "settings" : "",
  });
}

function placeDiffers(id) {
  const own = placeFields(state.mcpPlaces && state.mcpPlaces[id]);
  const shared = sharedPlace(id);
  if (id === "atlassian_jira") return jiraDiffers(own, shared);
  if (id === "atlassian_confluence") return confluenceDiffers(own, shared);
  return false;
}

function useSettingsRow(id, fields) {
  const row = el("div", "use-settings-row");
  const button = el("button", "text-button", "Use from settings");
  button.type = "button";
  button.addEventListener("click", () => {
    if (!state.mcpPlaces) state.mcpPlaces = {};
    state.mcpPlaces[id] = placeFields();
    for (const field of fields) if (field.refresh) field.refresh();
    sync();
  });
  row.append(button);
  const sync = () => { row.hidden = !placeDiffers(id); };
  sync();
  row.sync = sync;
  return row;
}

function placeNote(source, label) {
  if (source === "agent") return "This agent uses its own " + label + ".";
  if (source === "settings") return "From Settings. Choose another " + label + " for this agent.";
  return "";
}

function mcpPlaceFields(id) {
  if (!state.mcpPlaces) state.mcpPlaces = {};
  if (!state.mcpPlaces[id]) state.mcpPlaces[id] = placeFields();
  const place = () => state.mcpPlaces[id];
  if (id === "atlassian_jira") {
    return [searchSelect({
      label: "Board",
      placeholder: "Search Jira boards",
      value: () => shownPlace(id).boardName || "",
      warn: () => jiraDiffers(placeFields(place()), sharedPlace(id)),
      warnText: "Diff Jira",
      note: () => placeNote(shownPlace(id).source, "board"),
      clearLabel: () => sharedPlace(id).boardId || sharedPlace(id).boardName ? "Use Settings" : "Clear",
      load: (query) => fetchPlaces("/api/atlassian/jira/boards?q=" + encodeURIComponent(query)),
      onChoose: (item) => {
        const shared = sharedPlace(id);
        const same = item && item.id === shared.boardId && (item.name || "") === shared.boardName;
        place().boardId = !item || same ? "" : item.id;
        place().boardName = !item || same ? "" : (item.name || "");
      },
    })];
  }
  if (id !== "atlassian_confluence") return [];
  let parentField;
  const space = searchSelect({
    label: "Space",
    placeholder: "Search Confluence spaces",
    value: () => shownPlace(id).spaceName || "",
    warn: () => confluenceSpaceDiffers(placeFields(place()), sharedPlace(id)),
    warnText: "Diff Confluence",
    note: () => placeNote(shownPlace(id).source, "space"),
    clearLabel: () => sharedPlace(id).spaceId || sharedPlace(id).spaceName ? "Use Settings" : "Clear",
    load: (query) => fetchPlaces("/api/atlassian/confluence/spaces?q=" + encodeURIComponent(query)),
    onChoose: (item) => {
      const current = place();
      const shared = sharedPlace(id);
      const same = item && item.id === shared.spaceId;
      current.spaceId = !item || same ? "" : item.id;
      current.spaceKey = !item || same ? "" : (item.key || "");
      current.spaceName = !item || same ? "" : (item.name || "");
      current.parentId = "";
      current.parentTitle = "";
      if (parentField) parentField.refresh();
    },
  });
  parentField = searchSelect({
    label: "Parent page",
    placeholder: () => shownPlace(id).spaceId ? "Search pages" : "Choose a space first.",
    value: () => shownPlace(id).parentTitle || "",
    warn: () => confluenceParentDiffers(placeFields(place()), sharedPlace(id)),
    warnText: "Diff Confluence",
    note: () => placeNote(place().parentId || place().parentTitle ? "agent" : (shownPlace(id).parentTitle ? shownPlace(id).source : ""), "parent page"),
    clearLabel: () => {
      const shared = sharedPlace(id);
      const shown = shownPlace(id);
      return shared.parentId && shown.spaceId === shared.spaceId ? "Use Settings" : "Clear";
    },
    disabled: () => !shownPlace(id).spaceId,
    load: (query) => {
      const current = shownPlace(id);
      const params = new URLSearchParams({ space: current.spaceId || "", key: current.spaceKey || "", q: query });
      return fetchPlaces("/api/atlassian/confluence/pages?" + params.toString());
    },
    onChoose: (item) => {
      const current = place();
      const shared = sharedPlace(id);
      const shown = shownPlace(id);
      if (!current.spaceId && !current.spaceName && shown.spaceId) {
        current.spaceId = shown.spaceId;
        current.spaceKey = shown.spaceKey;
        current.spaceName = shown.spaceName;
      }
      current.parentId = item ? item.id : "";
      current.parentTitle = item ? (item.name || "") : "";
      const backToSettings = (!item && !current.spaceId) || (current.spaceId === shared.spaceId && current.parentId === shared.parentId && current.parentTitle === shared.parentTitle);
      if (backToSettings || (!item && current.spaceId === shared.spaceId && current.spaceName === shared.spaceName)) {
        current.spaceId = "";
        current.spaceKey = "";
        current.spaceName = "";
        current.parentId = "";
        current.parentTitle = "";
      }
      space.refresh();
    },
  });
  return [space, parentField];
}

function searchSelect(options) {
  const field = el("label", "place-field");
  const heading = el("span", "place-label");
  heading.append(document.createTextNode(options.label));
  const warn = options.warn ? el("span", "badge warn", options.warnText || "Diff") : null;
  if (warn) {
    warn.hidden = true;
    warn.title = "Different from Settings.";
    heading.append(warn);
  }
  field.append(heading);
  const box = el("div", "place-box");
  const input = document.createElement("input");
  input.type = "search";
  input.autocomplete = "off";
  input.spellcheck = false;
  const menu = el("div", "place-menu");
  menu.hidden = true;
  menu.addEventListener("mousedown", (event) => event.preventDefault());
  box.append(input, menu);
  field.append(box);
  const note = options.note ? el("p", "place-note", "") : null;
  if (note) field.append(note);
  const refresh = () => {
    input.value = options.value() || "";
    input.placeholder = typeof options.placeholder === "function" ? options.placeholder() : options.placeholder;
    input.disabled = options.disabled ? options.disabled() : false;
    if (warn) warn.hidden = !options.warn();
    if (!note) return;
    const text = options.note() || "";
    note.textContent = text;
    note.hidden = !text;
  };
  refresh();
  let timer = 0;
  let request = 0;
  const choose = (item) => {
    menu.hidden = true;
    options.onChoose(item);
    refresh();
    if (field.syncSettings) field.syncSettings();
  };
  const open = async () => {
    if (input.disabled) return;
    const token = ++request;
    menu.hidden = false;
    menu.replaceChildren(el("p", "place-empty", "Loading…"));
    let items = [];
    try {
      items = await options.load(input.value.trim());
    } catch (error) {
      if (token !== request) return;
      menu.replaceChildren(el("p", "place-empty", error.message || "Could not load the list."));
      return;
    }
    if (token !== request) return;
    menu.replaceChildren();
    if (options.value()) {
      const label = options.clearLabel ? options.clearLabel() : "Clear";
      if (label !== "Use Settings") {
        const clear = el("button", "place-option", label);
        clear.type = "button";
        clear.addEventListener("click", () => choose(null));
        menu.append(clear);
      }
    }
    if (!items.length) menu.append(el("p", "place-empty", "No matches."));
    for (const item of items) {
      const option = el("button", "place-option", item.key ? item.name + " (" + item.key + ")" : item.name);
      option.type = "button";
      option.addEventListener("click", () => choose(item));
      menu.append(option);
    }
  };
  input.addEventListener("focus", () => open());
  input.addEventListener("input", () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(open, 250);
  });
  input.addEventListener("blur", () => {
    window.setTimeout(() => {
      menu.hidden = true;
      refresh();
    }, 120);
  });
  field.input = input;
  field.refresh = refresh;
  return field;
}

function fillAgentRepos(selected) {
  const repos = document.querySelector("#agent-repos");
  repos.replaceChildren();
  document.querySelector("#agent-repo-notes").replaceChildren();
  const items = repoList(selected);
  state.repoNotes = {};
  state.repoReadonly = {};
  for (const repo of items) {
    state.repoNotes[repo.kind] = repo.description;
    state.repoReadonly[repo.kind] = !!repo.readonly;
  }
  const chosen = new Set(items.map((repo) => repo.kind));
  for (const kind of repoChoices([...chosen])) {
    const button = el("button", "tag", repoLabel[kind] || kind);
    button.type = "button";
    button.dataset.repo = kind;
    button.setAttribute("aria-pressed", chosen.has(kind) ? "true" : "false");
    repos.append(button);
  }
}

function showMarkdownPreview(area, preview, on) {
  const compose = area.closest(".compose") || area;
  if (on) {
    preview.innerHTML = renderMarkdown(area.value);
    preview.hidden = false;
    compose.hidden = true;
  } else {
    preview.hidden = true;
    preview.replaceChildren();
    compose.hidden = false;
    placeGenerateButtons();
  }
}

function showAgentPromptPreview(on) {
  showMarkdownPreview(
    document.querySelector("#agent-prompt"),
    document.querySelector("#agent-prompt-preview"),
    on,
  );
  document.querySelector("#agent-prompt-toggle").setAttribute("aria-pressed", on ? "true" : "false");
}

async function openAgentEditor(id) {
  const data = await api("/api/agents/" + encodeURIComponent(id));
  state.editingAgentId = data.id;
  document.querySelector("#agent-dialog-title").textContent = "Agent: " + (data.name || data.id);
  document.querySelector("#agent-enabled").setAttribute("aria-pressed", data.enabled !== false ? "true" : "false");
  document.querySelector("#agent-name").value = data.name || "";
  document.querySelector("#agent-id").value = data.id || "";
  fillAgentCategories(data.category || "");
  fillAgentPeers(data.id, data.worksWith);
  renderCollaborations();
  document.querySelector("#agent-description").value = data.description || "";
  document.querySelector("#agent-prompt").value = data.prompt || "";
  const promptGenerate = document.querySelector("#agent-prompt").closest(".field").querySelector("[data-generate]");
  if (promptGenerate) promptGenerate.dataset.generateSubject = data.id || "";
  showAgentPromptPreview(false);
  fillAgentRepos(data.repos);
  renderRepoNotes();
  fillUses("mcp", data.mcps);
  renderUseNotes("mcp");
  fillUses("tool", data.tools);
  renderUseNotes("tool");
  const tool = document.querySelector("#agent-tool");
  const toolName = usableTool(data.tool);
  fillToolSelect(tool, toolName);
  fillModels(document.querySelector("#agent-model"), toolName, data.model);
  const error = document.querySelector("#agent-error");
  error.hidden = true;
  error.textContent = "";
  const save = document.querySelector("#agent-save");
  save.disabled = false;
  save.textContent = "Save";
  document.querySelector("#agent-dialog").showModal();
  document.querySelector("#agent-name").focus();
  requestAnimationFrame(placeGenerateButtons);
}

async function saveAgent() {
  const button = document.querySelector("#agent-save");
  const error = document.querySelector("#agent-error");
  error.hidden = true;
  button.disabled = true;
  button.textContent = "Saving…";
  rememberRepoNotes();
  const repos = [...document.querySelectorAll("#agent-repos button[aria-pressed='true']")].map((button) => ({
    kind: button.dataset.repo,
    description: (state.repoNotes[button.dataset.repo] || "").trim(),
    readonly: !!(state.repoReadonly && state.repoReadonly[button.dataset.repo]),
  }));
  rememberCollaborations();
  const worksWith = [...document.querySelectorAll("#agent-works-with button[aria-pressed='true']")].map((button) => ({
    id: button.dataset.peer,
    collaboration: (state.peerNotes[button.dataset.peer] || "").trim(),
  }));
  const mcps = selectedUses("mcp");
  const tools = selectedUses("tool");
  const name = document.querySelector("#agent-name").value.trim();
  try {
    const data = await api("/api/agents/" + encodeURIComponent(state.editingAgentId), {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        id: state.editingAgentId,
        name,
        description: document.querySelector("#agent-description").value.trim(),
        category: document.querySelector("#agent-category").value.trim(),
        enabled: document.querySelector("#agent-enabled").getAttribute("aria-pressed") === "true",
        tool: document.querySelector("#agent-tool").value,
        model: document.querySelector("#agent-model").value,
        repos,
        worksWith,
        mcps,
        tools,
        prompt: document.querySelector("#agent-prompt").value,
      }),
    });
    const selected = document.querySelector("[data-tab][aria-selected='true']");
    const tab = (selected && selected.dataset.tab) || "agents";
    render(data);
    showTab(tab);
    document.querySelector("#agent-dialog").close();
    banner("Saved " + name + ".");
  } catch (err) {
    error.hidden = false;
    error.textContent = err.message;
    button.disabled = false;
    button.textContent = "Save";
  }
}

document.body.addEventListener("click", (event) => {
  const reveal = event.target.closest("[data-secret-toggle]");
  if (reveal) {
    event.preventDefault();
    const input = document.getElementById(reveal.dataset.secretToggle);
    if (!input) return;
    const show = input.type === "password";
    input.type = show ? "text" : "password";
    reveal.setAttribute("aria-pressed", show ? "true" : "false");
    reveal.setAttribute("aria-label", show ? "Hide API token" : "Show API token");
    reveal.replaceChildren(eyeIcon(show));
    return;
  }
  const indexLink = event.target.closest(".index-tool-link");
  if (indexLink) {
    event.preventDefault();
    openIndexPage(indexLink.dataset.url);
    return;
  }
  const tab = event.target.closest("[data-tab]");
  if (tab) {
    showTab(tab.dataset.tab);
    return;
  }
  const editAgent = event.target.closest("[data-edit-agent]");
  if (editAgent) {
    openAgentEditor(editAgent.dataset.editAgent).catch((error) => banner(error.message, true));
    return;
  }
  const promptButton = event.target.closest("[data-prompt]");
  if (promptButton) {
    openAgentPrompt(promptButton.dataset.prompt).catch((error) => banner(error.message, true));
    return;
  }
  const guardrailView = event.target.closest("[data-guardrail-view]");
  if (guardrailView) {
    openGuardrail();
    return;
  }
  const flowAboutButton = event.target.closest("[data-flow-about]");
  if (flowAboutButton) {
    openFlowAbout(flowAboutButton.dataset.flowAbout, flowAboutButton.dataset.flowId);
    return;
  }
  const run = event.target.closest("[data-run]");
  if (run) {
    openLaunch(run.dataset.run);
    return;
  }
  const clone = event.target.closest("[data-clone]");
  if (clone) {
    openClone(clone.dataset.clone);
    return;
  }
  const featurePrompt = event.target.closest("[data-feature-prompt]");
  if (featurePrompt) {
    openFeaturePrompt(featurePrompt.dataset.featurePrompt).catch((error) => featureError(error.message));
    return;
  }
  const toolToggle = event.target.closest("[data-tool-toggle]");
  if (toolToggle) {
    const on = toolToggle.getAttribute("aria-pressed") !== "true";
    toolToggle.setAttribute("aria-pressed", on ? "true" : "false");
    syncSave();
    return;
  }
  const guardrailsToggle = event.target.closest("[data-guardrails-toggle]");
  if (guardrailsToggle) {
    const on = guardrailsToggle.getAttribute("aria-pressed") !== "true";
    guardrailsToggle.setAttribute("aria-pressed", on ? "true" : "false");
    syncSave();
    return;
  }
  const mcpToggle = event.target.closest("[data-mcp-toggle]");
  if (mcpToggle) {
    const on = mcpToggle.getAttribute("aria-pressed") !== "true";
    mcpToggle.setAttribute("aria-pressed", on ? "true" : "false");
    const cardNote = document.querySelector("#settings-fields .mcp-card > .field-error");
    if (cardNote) cardNote.hidden = true;
    syncMCP();
    syncSave();
    return;
  }
  const featureAgent = event.target.closest("[data-feature-agent]");
  if (featureAgent) {
    const on = featureAgent.getAttribute("aria-pressed") !== "true";
    featureAgent.setAttribute("aria-pressed", on ? "true" : "false");
    const picks = document.querySelector(`[data-feature-picks="${CSS.escape(featureAgent.dataset.featureAgent)}"]`);
    if (picks) picks.hidden = !on;
    const prompt = document.querySelector(`[data-feature-prompt="${CSS.escape(featureAgent.dataset.featureAgent)}"]`);
    if (prompt) prompt.hidden = !on;
    return;
  }
  const agentSwitch = event.target.closest(".agent-switch");
  if (agentSwitch) {
    const button = agentSwitch.querySelector("#agent-enabled");
    button.setAttribute("aria-pressed", button.getAttribute("aria-pressed") === "true" ? "false" : "true");
    return;
  }
  const repoTag = event.target.closest("#agent-repos button, #agent-works-with button");
  if (repoTag) {
    repoTag.setAttribute("aria-pressed", repoTag.getAttribute("aria-pressed") === "true" ? "false" : "true");
    if (repoTag.closest("#agent-works-with")) renderCollaborations();
    if (repoTag.closest("#agent-repos")) renderRepoNotes();
    return;
  }
  const useTag = event.target.closest("#agent-mcps button, #agent-tools button, #agent-tools .agent-tool");
  if (useTag) {
    const button = useTag.matches("button") ? useTag : useTag.querySelector("button");
    if (!button) return;
    button.setAttribute("aria-pressed", button.getAttribute("aria-pressed") === "true" ? "false" : "true");
    renderUseNotes(button.closest("#agent-mcps") ? "mcp" : "tool");
    return;
  }
  const generate = event.target.closest("[data-generate]");
  if (generate) {
    prepareGenerate(generate);
    return;
  }
  if (event.target.closest("#agent-prompt-toggle")) {
    const button = document.querySelector("#agent-prompt-toggle");
    showAgentPromptPreview(button.getAttribute("aria-pressed") !== "true");
    return;
  }
  const repoToggle = event.target.closest("[data-repo-preview]");
  if (repoToggle) {
    const block = repoToggle.closest(".field");
    const area = block.querySelector("textarea");
    const preview = block.querySelector(".repo-preview");
    const on = repoToggle.getAttribute("aria-pressed") !== "true";
    if (on) state.repoNotes[area.dataset.repo] = area.value;
    showMarkdownPreview(area, preview, on);
    repoToggle.setAttribute("aria-pressed", on ? "true" : "false");
    return;
  }
  const useToggle = event.target.closest("[data-use-preview]");
  if (useToggle) {
    const kind = useToggle.dataset.usePreview;
    const spec = useSections[kind];
    const block = useToggle.closest(".field");
    const area = block.querySelector("textarea");
    const preview = block.querySelector(".repo-preview");
    const on = useToggle.getAttribute("aria-pressed") !== "true";
    if (on) state[spec.state][area.dataset.use] = area.value;
    showMarkdownPreview(area, preview, on);
    useToggle.setAttribute("aria-pressed", on ? "true" : "false");
    return;
  }
  const collabToggle = event.target.closest("[data-collab-preview]");
  if (collabToggle) {
    const block = collabToggle.closest(".field");
    const area = block.querySelector("textarea");
    const preview = block.querySelector(".collab-preview");
    const on = collabToggle.getAttribute("aria-pressed") !== "true";
    if (on) state.peerNotes[area.dataset.peer] = area.value;
    showMarkdownPreview(area, preview, on);
    collabToggle.setAttribute("aria-pressed", on ? "true" : "false");
    return;
  }
  const tag = event.target.closest(".service-tags button");
  if (tag) {
    tag.setAttribute("aria-pressed", tag.getAttribute("aria-pressed") === "true" ? "false" : "true");
    syncSave();
    return;
  }
  if (event.target.closest("[data-save]")) {
    save();
    return;
  }
  const refreshRepo = event.target.closest("[data-refresh-repo]");
  if (refreshRepo) {
    refreshRepoIndex(refreshRepo.dataset.refreshRepo);
    return;
  }
  const indexUse = event.target.closest("[data-repo-index-use]");
  if (indexUse) {
    toggleRepoIndex(indexUse);
    return;
  }
  const browseFolder = event.target.closest("[data-browse-folder]");
  if (browseFolder) {
    browseRepoFolder(browseFolder.dataset.browseFolder);
    return;
  }
  const clear = event.target.closest("[data-clear-index]");
  if (clear) {
    openRemoveIndex(clear.dataset.clearIndex);
  }
});

document.body.addEventListener("input", (event) => {
  if (event.target.closest("#settings-fields")) clearFieldValidation(event.target);
  if (event.target.id === "team-path" || event.target.id === "team-user") syncTeamName();
  if (event.target.id === "workspace-id" || event.target.id === "team-path" || event.target.id === "team-user") syncSave();
  if (event.target.id && event.target.id.startsWith("mcp-")) syncSave();
  if (event.target.id === "team-path") {
    if (!event.target.value.trim()) cancelTeamStage();
    refreshTeamNameChoices(event.target);
  }
  if (event.target.id === "feature-name") refreshFeatureFolder();
});

document.body.addEventListener("keydown", (event) => {
  if (event.key !== "Enter" || !event.target.dataset || !event.target.dataset.repoFolder) return;
  event.preventDefault();
  applyRepoFolder(event.target.dataset.repoFolder, event.target.value);
});

document.body.addEventListener("focusout", (event) => {
  if (event.target.dataset && event.target.dataset.repoFolder && event.target.isConnected) {
    applyRepoFolder(event.target.dataset.repoFolder, event.target.value);
  }
  if (event.target.id !== "team-path" || !event.target.isConnected) return;
  const path = event.target.value.trim();
  if (!path) return;
  if (path === teamListedPath) return;
  stageTeamPath(path).catch((error) => banner(error.message, true));
});

document.body.addEventListener("change", (event) => {
  if (event.target.id === "appearance") {
    applyTheme(event.target.value);
    return;
  }
  const repoIndexChoice = event.target.closest("[data-repo-index-choice]");
  if (repoIndexChoice) {
    requestRepoIndex(repoIndexChoice);
    return;
  }
  if (event.target.name === "team-name-mode") {
    syncTeamName();
    syncSave();
    const control = document.querySelector(event.target.value === "existing" ? "#team-member-name" : "#team-user");
    if (control) control.focus();
    return;
  }
  if (event.target.id === "team-member-name") {
    clearFieldValidation(event.target);
    syncTeamName();
    syncSave();
    return;
  }
  if (event.target.id === "agent-member") {
    chooseMember(event.target).catch((error) => banner(error.message, true));
    return;
  }
  if (event.target.id === "mcp-jira-own" || event.target.id === "mcp-confluence-own") {
    syncMCP();
    syncSave();
    return;
  }
  if (event.target.id === "orchestrator") syncSave();
  if (event.target.id === "generate-tool") {
    const modelSelect = document.querySelector("#generate-model");
    modelSelect.dataset.previousModel = modelSelect.value;
    refreshModels(modelSelect, event.target.value).then(() => syncSave());
    return;
  }
  if (event.target.id === "generate-model") syncSave();
  if (event.target.id === "agent-tool") {
    const modelSelect = document.querySelector("#agent-model");
    modelSelect.dataset.previousModel = modelSelect.value;
    refreshModels(modelSelect, event.target.value);
    return;
  }
  const featureTool = event.target.closest("[data-feature-tool]");
  if (featureTool) {
    const next = document.querySelector(`[data-feature-model="${CSS.escape(featureTool.dataset.featureTool)}"]`);
    next.dataset.previousModel = next.value;
    refreshModels(next, featureTool.value);
    return;
  }
  const cardTool = event.target.closest(".agent-card [data-tool]");
  if (cardTool) {
    const modelSelect = cardTool.closest(".agent-picks").querySelector("[data-model]");
    modelSelect.dataset.previousModel = modelSelect.value;
    refreshModels(modelSelect, cardTool.value).then((ok) => {
      if (ok) saveCardChoice(cardTool.dataset.tool, cardTool, modelSelect);
    });
    return;
  }
  const cardModel = event.target.closest(".agent-card [data-model]");
  if (cardModel) {
    const toolSelect = cardModel.closest(".agent-picks").querySelector("[data-tool]");
    saveCardChoice(cardModel.dataset.model, toolSelect, cardModel);
  }
});

function openClone(kind) {
  const repo = state.data.repos.find((item) => item.kind === kind);
  if (!repo) return;
  const dialog = document.querySelector("#clone-dialog");
  dialog.dataset.kind = kind;
  document.querySelector("#clone-title").textContent = "Clone " + repoLabel[kind];
  document.querySelector("#clone-path").textContent = "KIKI clones this repository into " + repo.folder + ".";
  const input = document.querySelector("#clone-url");
  input.value = "";
  const sparse = document.querySelector("#clone-sparse");
  sparse.value = "";
  const error = document.querySelector("#clone-error");
  error.hidden = true;
  error.textContent = "";
  const go = document.querySelector("#clone-go");
  go.disabled = false;
  go.textContent = "Clone";
  dialog.showModal();
  input.focus();
}

async function cloneRepo() {
  const dialog = document.querySelector("#clone-dialog");
  const kind = dialog.dataset.kind;
  const input = document.querySelector("#clone-url");
  const sparse = document.querySelector("#clone-sparse");
  const error = document.querySelector("#clone-error");
  const go = document.querySelector("#clone-go");
  const checkout = sparse.value.trim();
  error.hidden = true;
  go.disabled = true;
  go.textContent = "Cloning…";
  try {
    const data = await api("/api/repos/" + encodeURIComponent(kind) + "/clone", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url: input.value.trim(), sparse: checkout }),
    });
    dialog.close();
    render(data);
    const repo = data.repos.find((item) => item.kind === kind);
    const where = repo ? repo.folder : "the folder";
    banner(checkout ? "Cloned " + checkout + " into " + where + "." : "Cloned the repository into " + where + ".");
  } catch (err) {
    error.hidden = false;
    error.textContent = err.message;
    go.disabled = false;
    go.textContent = "Clone";
  }
}

function featureStamp() {
  const now = new Date();
  const part = (value) => String(value).padStart(2, "0");
  return (
    String(now.getFullYear()) +
    part(now.getMonth() + 1) +
    part(now.getDate()) +
    part(now.getHours()) +
    part(now.getMinutes()) +
    part(now.getSeconds())
  );
}

function snakeCase(value) {
  return value
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/đ/gi, "d")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
}

function featureFolder() {
  const slug = snakeCase(document.querySelector("#feature-name").value);
  const repo = state.data.repos.find((item) => item.kind === "document");
  const base = repo && repo.folder ? repo.folder.replace(/\/$/, "") : "";
  return { slug, path: slug && base ? base + "/" + slug : "" };
}

function refreshFeatureFolder() {
  const folder = featureFolder();
  document.querySelector("#feature-folder").textContent = folder.path
    ? "Requirement folder: " + folder.path
    : "The folder name is the snake_case form of the feature name.";
}

function featurePicks(agentId) {
  const choice = choiceFor(agentId);
  const picks = el("div", "agent-picks");
  picks.dataset.featurePicks = agentId;
  picks.hidden = true;
  const toolField = el("label", "field");
  toolField.append(el("span", "", "AI Tool"));
  const toolSelect = el("select");
  toolSelect.dataset.featureTool = agentId;
  fillToolSelect(toolSelect, choice.tool);
  toolField.append(toolSelect);
  const modelField = el("label", "field");
  modelField.append(el("span", "", "Model"));
  const modelSelect = el("select");
  modelSelect.dataset.featureModel = agentId;
  fillModels(modelSelect, choice.tool, choice.model);
  modelField.append(modelSelect);
  picks.append(toolField, modelField);
  return picks;
}

function openFeature() {
  document.querySelector("#feature-name").value = "";
  document.querySelector("#feature-description").value = "";
  refreshFeatureFolder();
  const list = document.querySelector("#feature-agents");
  list.replaceChildren();
  for (const group of groups(state.data.agents)) {
    const agents = group.agents.filter((agent) => agent.enabled !== false);
    if (!agents.length) continue;
    const section = el("section", "feature-group");
    section.append(el("h3", "", categoryLabel(group.name)));
    for (const agent of agents) {
      const row = el("div", "feature-agent");
      const toggle = el("div", "feature-toggle");
      const text = el("span", "feature-label");
      text.append(el("span", "feature-title", agent.title));
      text.append(el("span", "feature-id", "ID: " + agent.id));
      const button = el("button", "switch");
      button.type = "button";
      button.dataset.featureAgent = agent.id;
      button.setAttribute("aria-pressed", "false");
      button.setAttribute("aria-label", agent.title);
      button.append(el("span", "switch-knob"));
      const prompt = el("button", "icon-button feature-prompt");
      prompt.type = "button";
      prompt.hidden = true;
      prompt.dataset.featurePrompt = agent.id;
      prompt.setAttribute("aria-label", "Show the prompt for " + agent.title);
      prompt.append(promptIcon());
      const controls = el("div", "feature-controls");
      controls.append(prompt, button);
      toggle.append(text, controls);
      row.append(toggle, featurePicks(agent.id));
      section.append(row);
    }
    list.append(section);
  }
  const error = document.querySelector("#feature-error");
  error.hidden = true;
  error.textContent = "";
  const result = document.querySelector("#feature-result");
  result.hidden = true;
  result.textContent = "";
  result.classList.remove("error");
  const label = orchestratorLabel();
  document.querySelector("#feature-lead").textContent =
    "Turn on the agents to start in " + label + ". AI Tool and model start from each agent. Agents that are turned off stay out of this list.";
  const go = document.querySelector("#feature-go");
  go.disabled = false;
  go.textContent = "Start on " + label;
  document.querySelector("#feature-dialog").showModal();
  document.querySelector("#feature-name").focus();
}

function featureNote(agentId, folder) {
  const name = document.querySelector("#feature-name").value.trim();
  const description = document.querySelector("#feature-description").value.trim();
  if (agentId === "requirement_document") {
    return [
      "Feature name: " + name,
      "Requirement folder: " + folder.path,
      "The folder name is " + folder.slug + ", the snake_case form of the feature name. Create this requirement folder and write the requirement there.",
      "",
      "What this feature does:",
      description,
    ].join("\n");
  }
  return [
    "Requirement folder: " + folder.path,
    "Use this folder when you need the requirement for this feature.",
  ].join("\n");
}

async function openFeaturePrompt(id) {
  const agent = agentById(id);
  const tool = document.querySelector(`[data-feature-tool="${CSS.escape(id)}"]`).value;
  const model = document.querySelector(`[data-feature-model="${CSS.escape(id)}"]`).value;
  const dialog = document.querySelector("#prompt-reader");
  document.querySelector("#prompt-reader-title").textContent = agent.title + " — prompt sent to " + orchestratorLabel();
  document.querySelector("#prompt-reader-body").textContent = "Loading…";
  dialog.showModal();
  try {
    const data = await api("/api/agents/" + encodeURIComponent(id) + "/preview", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        note: featureNote(id, featureFolder()),
        repo: agent.repos[0],
        agent: tool,
        model,
      }),
    });
    document.querySelector("#prompt-reader-body").innerHTML = renderMarkdown(data.prompt);
  } catch (error) {
    dialog.close();
    featureError(error.message);
  }
}

function featureError(message) {
  const error = document.querySelector("#feature-error");
  error.hidden = false;
  error.textContent = message;
}

async function startFeature() {
  const error = document.querySelector("#feature-error");
  error.hidden = true;
  const name = document.querySelector("#feature-name").value.trim();
  const description = document.querySelector("#feature-description").value.trim();
  const folder = featureFolder();
  const checked = [...document.querySelectorAll("[data-feature-agent][aria-pressed='true']")];
  if (!name || !folder.slug) {
    featureError("Enter a feature name.");
    return;
  }
  if (!folder.path) {
    featureError("The document folder is not set.");
    return;
  }
  if (!description) {
    featureError("Describe what this feature does.");
    return;
  }
  if (!checked.length) {
    featureError("Turn on at least one agent.");
    return;
  }
  const go = document.querySelector("#feature-go");
  const result = document.querySelector("#feature-result");
  go.disabled = true;
  go.textContent = "Starting…";
  result.hidden = false;
  result.classList.remove("error");
  const stamp = featureStamp();
  const label = orchestratorLabel();
  const lines = [];
  let failed = 0;
  for (const box of checked) {
    const id = box.dataset.featureAgent;
    const agent = agentById(id);
    const tool = document.querySelector(`[data-feature-tool="${CSS.escape(id)}"]`).value;
    const model = document.querySelector(`[data-feature-model="${CSS.escape(id)}"]`).value;
    result.textContent = "Starting " + id + " on " + label + "…";
    try {
      const data = await api("/api/agents/" + encodeURIComponent(id) + "/launch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          note: featureNote(id, folder),
          repo: agent.repos[0],
          agent: tool,
          model,
          slug: folder.slug,
          stamp,
        }),
      });
      if (data.launch.status === "ok") lines.push(id + ": started on " + label);
      else {
        failed += 1;
        lines.push(id + ": " + data.launch.detail);
      }
    } catch (err) {
      failed += 1;
      lines.push(id + ": " + err.message);
    }
  }
  result.classList.toggle("error", failed > 0);
  result.textContent = lines.join("\n");
  try {
    const fresh = await api("/api/state");
    state.data = fresh;
    renderHistory(fresh);
  } catch (err) {
    lines.push(err.message);
    result.textContent = lines.join("\n");
  }
  if (failed === 0) {
    document.querySelector("#feature-dialog").close();
    banner("Started " + checked.length + (checked.length === 1 ? " agent" : " agents") + " on " + label + " for " + folder.slug + ".");
    return;
  }
  go.disabled = false;
  go.textContent = "Start on " + label;
}

document.querySelector("#settings-fields").addEventListener("click", (event) => {
  if (event.target.closest("#team-browse")) browseTeamFolder();
});
document.querySelector("#start-feature").addEventListener("click", openFeature);
document.querySelector("#open-settings").addEventListener("click", () => {
  if (!state.data) return;
  renderConfig(state.data);
  syncSave();
  document.querySelector("#settings-dialog").showModal();
  const teamPath = ((state.data.teamFolder || {}).path || "").trim();
  if (teamPath) stageTeamPath(teamPath).catch((error) => banner(error.message, true));
});
document.querySelector("#settings-close").addEventListener("click", () => {
  document.querySelector("#settings-dialog").close();
});
document.querySelector("#settings-save").addEventListener("click", () => {
  saveSettings();
});
document.querySelector("#settings-dialog").addEventListener("close", () => {
  teamStage += 1;
  teamListedPath = "";
  teamListedMembers = [];
  if (!state.data) return;
  renderConfig(state.data);
  syncSave();
});
document.querySelector("#feature-cancel").addEventListener("click", () => {
  document.querySelector("#feature-dialog").close();
});
document.querySelector("#agent-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  saveAgent();
});

document.querySelector("#feature-form").addEventListener("submit", (event) => {
  event.preventDefault();
  startFeature();
});

document.querySelector("#clone-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  cloneRepo();
});

document.querySelector("#repo-index-form").addEventListener("submit", (event) => {
  const dialog = document.querySelector("#repo-index-dialog");
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  const kind = dialog.dataset.kind;
  const tool = dialog.dataset.tool;
  dialog.close("setup");
  setupRepoIndex(kind, tool);
});

document.querySelector("#repo-index-dialog").addEventListener("close", () => {
  const dialog = document.querySelector("#repo-index-dialog");
  if (dialog.returnValue === "setup") return;
  revertRepoIndexChoice(dialog.dataset.kind, dialog.dataset.previous || "");
});

document.querySelector("#repo-index-choose-tool").addEventListener("change", (event) => {
  const dialog = document.querySelector("#repo-index-choose-dialog");
  syncRepoIndexChoiceNote(dialog.dataset.kind, event.target.value);
});

document.querySelector("#repo-index-choose-form").addEventListener("submit", (event) => {
  const dialog = document.querySelector("#repo-index-choose-dialog");
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  const kind = dialog.dataset.kind;
  const tool = document.querySelector("#repo-index-choose-tool").value;
  dialog.close("use");
  applyRepoIndexChoice(kind, tool);
});

document.querySelector("#index-remove-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  confirmRemoveIndex();
});

document.querySelector("#launch-go").addEventListener("click", launch);
document.querySelector("#prompt-toggle").addEventListener("click", () => {
  const view = document.querySelector("#launch-prompt");
  const open = view.hidden;
  const button = document.querySelector("#prompt-toggle");
  button.setAttribute("aria-expanded", open ? "true" : "false");
  button.textContent = open ? "Hide the prompt" : "Show the prompt";
  if (open) showPrompt();
  else view.hidden = true;
});
document.querySelector("#prompt-open").addEventListener("click", () => {
  openPromptReader().catch((error) => banner(error.message, true));
});
document.querySelector("#prompt-reader-close").addEventListener("click", () => {
  document.querySelector("#prompt-reader").close();
});
function leaveGenerate() {
  generatePending = null;
  generateDraft = null;
  stopGeneratePreview();
  if (generateAbort) generateAbort.abort();
  showAgentForm();
}

document.querySelector("#generate-run").addEventListener("click", () => {
  const pending = generatePending;
  if (!pending) return;
  rewriteText(pending.button);
});
document.querySelector("#generate-back").addEventListener("click", leaveGenerate);
document.querySelector("#generate-cancel").addEventListener("click", leaveGenerate);
document.querySelector("#generate-use").addEventListener("click", () => {
  const draft = generateDraft;
  if (!draft) return;
  if (draft.area.isConnected) {
    draft.area.value = draft.text;
    storeGeneratedText(draft.button, draft.text);
  }
  generateDraft = null;
  showAgentForm();
});
document.querySelector("#agent-dialog").addEventListener("cancel", (event) => {
  if (document.querySelector("#agent-generate").hidden) return;
  event.preventDefault();
  leaveGenerate();
});
document.querySelector("#agent-dialog").addEventListener("input", (event) => {
  if (event.target.closest(".compose textarea")) placeGenerateButtons();
});
document.querySelector("#agent-dialog").addEventListener("close", () => {
  generatePending = null;
  stopGeneratePreview();
  if (generateAbort) generateAbort.abort();
  showAgentForm();
});
document.querySelector("#launch-repo").addEventListener("change", () => {
  showWorktree();
  if (!document.querySelector("#launch-prompt").hidden) showPrompt();
});

document.querySelector("#team-share-dialog").addEventListener("click", (event) => {
  const diff = event.target.closest("[data-agent-diff]");
  if (diff) {
    openAgentDiff(diff.dataset.agentDiff, diff.dataset.folder).catch((error) => banner(error.message, true));
    return;
  }
  const arrow = event.target.closest(".team-arrows button");
  if (!arrow) return;
  syncFolder(arrow.dataset.folder, arrow.dataset.sync).catch((error) => banner(error.message, true));
});
document.querySelector("#team-share-back").addEventListener("click", showTeamShareList);
document.querySelector("#team-share-dialog").addEventListener("close", showTeamShareList);
document.querySelector("#team-share-close").addEventListener("click", () => {
  document.querySelector("#team-share-dialog").close();
});

document.querySelector("#panel-agents").addEventListener("click", (event) => {
  if (event.target.closest("[data-open-team-share]")) {
    document.querySelector("#team-share-dialog").showModal();
    refreshTeamShare();
    return;
  }
  const remove = event.target.closest("[data-delete-run]");
  if (remove) {
    deleteRun(remove.dataset.deleteRun).catch((error) => banner(error.message, true));
    return;
  }
  if (event.target.closest("[data-clear-runs]")) {
    clearRuns().catch((error) => banner(error.message, true));
    return;
  }
  const row = event.target.closest("[data-run]");
  if (row) openRun(row.dataset.run);
});
document.querySelector("#run-close").addEventListener("click", () => {
  document.querySelector("#run-dialog").close();
});
document.querySelector("#run-delete").addEventListener("click", () => {
  const id = document.querySelector("#run-dialog").dataset.id;
  if (!id) return;
  deleteRun(id).catch((error) => banner(error.message, true));
});

paintGenerateIcons();

api("/api/state")
  .then((data) => {
    render(data);
    refreshAssignedRepoIndexes();
  })
  .catch((error) => {
    document.querySelector("#panel-agents").replaceChildren();
    banner(error.message, true);
  });
