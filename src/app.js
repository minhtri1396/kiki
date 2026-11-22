const state = { data: null, agentId: null, editingAgentId: "", creatingAgent: false, setupBusy: new Set(), pendingRepoIndex: null, checkingIndexes: new Set(), tab: "tasks", taskView: "board", indexChoice: {}, taskId: "", taskQuery: "", taskDoneFrom: "", taskDoneTo: "" };
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

function repoTitle(kind) {
  const repos = (state.data && state.data.repos) || [];
  const repo = repos.find((item) => item.kind === kind);
  if (repo && String(repo.name || "").trim()) return repo.name;
  return repoLabel[kind] || kind;
}

function agentRepoIds(agent) {
  return (agent.repos || [])
    .map((repo) => (typeof repo === "string" ? repo : repo && repo.kind))
    .filter(Boolean);
}

function agentsUsingRepo(kind) {
  return ((state.data && state.data.agents) || []).filter((agent) => agentRepoIds(agent).includes(kind));
}
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
  const res = await kikiFetch(url, options);
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
  renderTasks();
  watchTaskRuns();
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
  copy.append(el("p", "hint", "Share this app's agents and repositories with the team. The team folder keeps a folder of each for you and for each member."));
  const badge = el("span", "badge", "");
  badge.id = "team-share-badge";
  badge.hidden = true;
  card.append(copy, badge);

  const body = document.querySelector("#team-share-body");
  if (!body) return card;
  body.replaceChildren();
  body.append(el("p", "hint", "Copy agents or repositories between this app and the team folder. An arrow opens a list so you can choose what to include. Your own folder can go both ways. Another member's folder only copies into this app."));
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
    "Your folder in the team folder. → opens the agents or repositories that differ in this app, and ← opens the ones that differ in your folder. Choose which to copy. Anything you leave unticked stays as it is."
  ));
  const memberCompare = renderCompare(
    "member-compare",
    "This app and a member",
    false,
    "Another member's folder. Choose a member above, then use ←. The dialog lists the agents or repositories that differ so you can choose which to copy into this app. Anything you leave unticked stays as it is."
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
  block.append(compareRow("agents", "This app's agents", "Folder agents", push));
  block.append(compareRow("repos", "This app's repositories", "Folder repositories", push));
  return block;
}

function compareRow(part, left, right, push) {
  const row = el("div", "team-compare");
  row.dataset.part = part;
  row.append(agentColumn(left), arrowColumn(push !== false, part), agentColumn(right));
  return row;
}

function agentColumn(heading) {
  const view = el("div", "team-view");
  view.append(el("h4", "", heading));
  view.append(el("ul"));
  return view;
}

function arrowColumn(push, part) {
  const arrows = el("div", "team-arrows");
  if (push) arrows.append(arrowButton("→", "push", part));
  arrows.append(arrowButton("←", "pull", part));
  return arrows;
}

function arrowButton(glyph, direction, part) {
  const button = el("button", "text-button", glyph);
  button.type = "button";
  button.disabled = true;
  button.dataset.sync = direction;
  button.dataset.part = part || "agents";
  const noun = button.dataset.part === "repos" ? "repositories" : "agents";
  button.setAttribute("aria-label", glyph === "→" ? "Copy this app's " + noun + " to the folder" : "Copy the folder's " + noun + " to this app");
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
  const head = el("div", "agents-head");
  const add = el("button", "primary", "New agent");
  add.type = "button";
  add.id = "agent-new";
  head.append(add);
  panel.append(head);
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
      card.dataset.editAgent = agent.id;
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
      const remove = el("button", "icon-button danger");
      remove.type = "button";
      remove.dataset.deleteAgent = agent.id;
      remove.setAttribute("aria-label", "Delete " + agent.title);
      remove.title = "Delete";
      remove.append(trashIcon());
      const prompt = el("button", "icon-button");
      prompt.type = "button";
      prompt.dataset.prompt = agent.id;
      prompt.setAttribute("aria-label", "Show the prompt for " + agent.title);
      prompt.append(promptIcon());
      actions.append(remove, prompt);
      head.append(actions);
      card.append(head);
      card.append(el("p", "id", "ID: " + agent.id));
      const body = el("div", "card-copy");
      body.append(el("p", "summary", agent.summary));
      const repos = agent.repos.map((kind) => repoTitle(kind)).join(", ");
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
}

function renderRepos(data) {
  const panel = document.querySelector("#panel-repos");
  panel.replaceChildren();
  panel.append(el("p", "hint", "Each repository has a name, an index, and a folder. Add a repository, or rename and delete one. Turn the index on to use Obsidian or CodeGraph. A tool already in the folder is used as it is. Choose a folder and KIKI uses that path. Clone into it when the folder is empty. KIKI saves each repository in " + data.root + "/.kiki/repos."));
  const bar = el("div", "repo-toolbar");
  const add = el("button", "primary", "Add repository");
  add.type = "button";
  add.dataset.addRepo = "1";
  bar.append(add);
  panel.append(bar);

  const grid = el("div", "repo-grid");
  const rank = (kind) => {
    const index = repoKinds.indexOf(kind);
    return index === -1 ? repoKinds.length : index;
  };
  const repos = data.repos.slice().sort((a, b) => rank(a.kind) - rank(b.kind));
  for (const repo of repos) {
    const card = el("article", "card");
    const head = el("div", "repo-head");
    const title = el("div");
    title.append(el("h3", "", repoTitle(repo.kind)));
    title.append(el("p", "id", repo.kind));
    head.append(title, repoNameActions(repo.kind));
    card.append(head, repoFolderSection(repo), repoIndexSection(repo));
    grid.append(card);
  }
  panel.append(grid);
  panel.append(saveButton());
  if (state.checkingIndexes.size) markIndexesChecking();
  if (state.setupBusy.size) paintRepoSetupButtons();
}

function repoNameActions(kind) {
  const actions = el("div", "card-actions");
  const label = repoTitle(kind);
  const rename = el("button", "icon-button");
  rename.type = "button";
  rename.dataset.renameRepo = kind;
  rename.setAttribute("aria-label", "Rename " + label);
  rename.title = "Rename";
  rename.append(editIcon());
  const remove = el("button", "icon-button danger");
  remove.type = "button";
  remove.dataset.deleteRepo = kind;
  remove.setAttribute("aria-label", "Delete " + label);
  remove.title = "Delete";
  remove.append(trashIcon());
  actions.append(rename, remove);
  return actions;
}

function openRepoName(kind) {
  const dialog = document.querySelector("#repo-name-dialog");
  const input = document.querySelector("#repo-name-input");
  const error = document.querySelector("#repo-name-error");
  dialog.dataset.kind = kind || "";
  document.querySelector("#repo-name-title").textContent = kind ? "Rename repository" : "Add repository";
  document.querySelector("#repo-name-hint").textContent = kind
    ? "This changes the name. The id stays the same."
    : "The name starts with a letter and uses letters, digits, spaces, hyphens, or underscores. A new repository gets an id from that name.";
  const save = document.querySelector("#repo-name-save");
  save.textContent = kind ? "Rename" : "Add";
  save.disabled = false;
  input.value = kind ? repoTitle(kind) : "";
  const repo = kind && state.data ? state.data.repos.find((item) => item.kind === kind) : null;
  const divided = document.querySelector("#repo-name-divided");
  divided.checked = !!(repo && repo.divided);
  document.querySelector("#repo-name-divided-note").hidden = !divided.checked;
  error.hidden = true;
  dialog.showModal();
  input.focus();
  input.select();
}

function openDeleteRepo(kind) {
  const dialog = document.querySelector("#repo-delete-dialog");
  const label = repoTitle(kind);
  dialog.dataset.kind = kind;
  const users = agentsUsingRepo(kind);
  const list = document.querySelector("#repo-delete-agents");
  const note = document.querySelector("#repo-delete-note");
  list.replaceChildren();
  document.querySelector("#repo-delete-text").textContent = "Delete " + label + "? The folder on disk stays.";
  if (users.length) {
    for (const agent of users) {
      list.append(el("li", "", agent.title || agent.id));
    }
    list.hidden = false;
    note.hidden = false;
    note.textContent = "These agents use this repository.";
  } else {
    list.hidden = true;
    note.hidden = true;
  }
  document.querySelector("#repo-delete-error").hidden = true;
  document.querySelector("#repo-delete-form button[value='confirm']").disabled = false;
  dialog.showModal();
}

async function saveRepoName() {
  const dialog = document.querySelector("#repo-name-dialog");
  const input = document.querySelector("#repo-name-input");
  const error = document.querySelector("#repo-name-error");
  const current = dialog.dataset.kind || "";
  const creating = current === "";
  error.hidden = true;
  const button = document.querySelector("#repo-name-save");
  button.disabled = true;
  try {
    const data = await api(creating ? "/api/repos" : "/api/repos/" + encodeURIComponent(current), {
      method: creating ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: input.value,
        divided: document.querySelector("#repo-name-divided").checked,
      }),
    });
    dialog.close();
    render(data);
    showTab("repos");
    banner(creating ? "Added the repository." : "Renamed the repository.");
  } catch (err) {
    error.hidden = false;
    error.textContent = err.message;
  } finally {
    button.disabled = false;
  }
}

async function confirmDeleteRepo() {
  const dialog = document.querySelector("#repo-delete-dialog");
  const error = document.querySelector("#repo-delete-error");
  const kind = dialog.dataset.kind || "";
  error.hidden = true;
  const button = document.querySelector("#repo-delete-form button[value='confirm']");
  button.disabled = true;
  try {
    const data = await api("/api/repos/" + encodeURIComponent(kind), { method: "DELETE" });
    dialog.close();
    render(data);
    showTab("repos");
    banner("Deleted the repository. The folder on disk stays.");
  } catch (err) {
    error.hidden = false;
    error.textContent = err.message;
  } finally {
    button.disabled = false;
  }
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
	card.append(el("p", "hint", "The hook checks each command, file, MCP server, and agent before it runs. When a repository has services selected, a file in another Nx project is blocked."));
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
      load: (query) => loadSettingsPlaces("jira", "/api/atlassian/jira/boards?q=" + encodeURIComponent(query)),
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
    load: (query) => loadSettingsPlaces("confluence", "/api/atlassian/confluence/spaces?q=" + encodeURIComponent(query)),
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
      return loadSettingsPlaces("confluence", "/api/atlassian/confluence/pages?" + params.toString());
    },
    onChoose: (item) => {
      saved().parentId = item ? item.id : "";
      saved().parentTitle = item ? (item.name || "") : "";
      syncSave();
    },
  });
  return [el("p", "hint", "Agents use this space and parent page unless an agent chooses another one."), space, parentField];
}

function settingsPlaceReady(name) {
  const saved = (state.data && state.data.atlassian) || {};
  const service = saved[name] || {};
  const url = fieldValue("#mcp-" + name + "-url");
  if (!validHttpUrl(url) || !mcpPressed("#mcp-atlassian") || !mcpPressed("#mcp-" + name)) return false;
  if (mcpChecked("#mcp-" + name + "-own")) {
    return mcpCredentialReady(
      fieldValue("#mcp-" + name + "-username"),
      fieldValue("#mcp-" + name + "-api-token"),
      !!service.apiTokenSet,
    );
  }
  return mcpCredentialReady(fieldValue("#mcp-username"), fieldValue("#mcp-api-token"), !!saved.apiTokenSet);
}

function loadSettingsPlaces(name, url) {
  if (!settingsPlaceReady(name)) return Promise.reject(new Error("Enter the email, API token, and site URL."));
  return fetchSettingsPlaces(url);
}

async function fetchSettingsPlaces(url) {
  const data = await api(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(collectAtlassian() || {}),
  });
  return data.items || [];
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
  section.append(el("p", "hint", "Choose a folder on this computer or type its full path. Your name appears after that folder is chosen. If it already has members, pick one as your name or enter a new name. Save copies this app's agents into your agents folder there, and the repositories folder beside it. A name you type that already exists asks whether to enter a different name or use that folder."));
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

function shareOrder(left, right) {
  const seen = new Set();
  const order = [];
  for (const item of [...(left || []), ...(right || [])]) {
    if (!item || item.id == null || seen.has(item.id)) continue;
    seen.add(item.id);
    order.push(item.id);
  }
  return order;
}

function shareLabel(kind, item, id) {
  if (kind === "repos") {
    const named = item && String(item.name || "").trim();
    if (named) return named;
    return repoTitle((item && item.id) || id);
  }
  return (item && (item.name || item.id)) || id;
}

function fillAgentList(list, order, selfByID, otherByID, folder, part) {
  const kind = part === "repos" ? "repos" : "agents";
  list.replaceChildren();
  if (!order.length) {
    list.append(el("li", "hint", kind === "repos" ? "No repositories." : "No agents."));
    return;
  }
  for (const id of order) {
    const self = selfByID.get(id);
    const other = otherByID ? otherByID.get(id) : null;
    const item = el("li");
    if (!self) {
      item.classList.add("is-missing");
      item.dataset.shareId = id;
      if (other && other.category) item.dataset.shareMeta = other.category;
      const line = el("div", "agent-line");
      line.append(el("span", "name", shareLabel(kind, other, id)));
      line.append(el("span", "badge bad", "Deleted"));
      item.append(line);
      list.append(item);
      continue;
    }
    const label = shareLabel(kind, self, id);
    const meta = kind === "agents" && self.category ? el("span", "meta", self.category) : null;
    const changed = agentFileChanged(self, otherByID);
    const line = changed ? el("button", "agent-diff") : el("div", "agent-line");
    if (changed) {
      item.classList.add("is-diff");
      line.type = "button";
      line.dataset.agentDiff = self.id;
      line.dataset.folder = folder || "";
      line.dataset.part = kind;
      line.setAttribute("aria-label", "Show how " + label + " differs");
    }
    line.append(el("span", "name", label));
    if (meta) line.append(meta);
    item.append(line);
    list.append(item);
  }
}

function setColumnHeading(view, title) {
  view.querySelector("h4").replaceChildren(el("span", "", title));
}

function sourceDiffs(row, side) {
  const view = row && row.querySelectorAll(".team-view")[side === "folder" ? 1 : 0];
  if (!view) return [];
  const choices = [];
  for (const item of view.querySelectorAll("li")) {
    const name = item.querySelector(".name");
    if (item.classList.contains("is-missing")) {
      const id = item.dataset.shareId || "";
      if (!id) continue;
      choices.push({
        id: id,
        name: name ? name.textContent : id,
        meta: item.dataset.shareMeta || "",
        deleted: true,
      });
      continue;
    }
    if (!item.classList.contains("is-diff")) continue;
    const button = item.querySelector("[data-agent-diff]");
    const meta = item.querySelector(".meta");
    const id = button ? button.dataset.agentDiff : "";
    if (!id) continue;
    choices.push({
      id: id,
      name: name ? name.textContent : id,
      meta: meta ? meta.textContent : "",
      deleted: false,
    });
  }
  return choices;
}

function arrowChoiceLabel(part, direction, folderName, count) {
  const many = part === "repos" ? "repositories" : "agents";
  if (!count) return "Nothing to copy";
  if (direction === "push") {
    return folderName ? "Choose " + many + " to copy to " + folderName : "Choose " + many + " to copy to the folder";
  }
  return folderName ? "Choose " + many + " to copy from " + folderName : "Choose " + many + " to copy into this app";
}

function syncAgentArrows(row) {
  if (!row) return;
  const part = row.dataset.part === "repos" ? "repos" : "agents";
  const sample = row.querySelector("[data-sync]");
  const folder = sample ? sample.dataset.folder || "" : "";
  const push = sourceDiffs(row, "app");
  const pull = sourceDiffs(row, "folder");
  setArrow(row.querySelector("[data-sync='push']"), !!folder && push.length > 0, arrowChoiceLabel(part, "push", folder, push.length));
  setArrow(row.querySelector("[data-sync='pull']"), !!folder && pull.length > 0, arrowChoiceLabel(part, "pull", folder, pull.length));
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
  const comparable = !!(side && side.name && !side.missing);
  const reposHeading = folderHeading.replace(/agents$/, "repositories");
  paintShareRow(block, "agents", "This app's agents", folderHeading, app, side && side.agents, !!side && side.differs, comparable, side);
  paintShareRow(block, "repos", "This app's repositories", reposHeading, (teamShare && teamShare.repos) || [], side && side.repos, !!side && side.reposDiffer, comparable, side);
  const badge = block.querySelector(".badge");
  if (!comparable) {
    badge.hidden = true;
    return;
  }
  const matched = !side.differs && !side.reposDiffer;
  badge.hidden = false;
  badge.className = "badge " + (matched ? "ok" : "wait");
  badge.textContent = matched ? "Matches" : "Not matched";
}

function paintShareRow(block, part, leftTitle, rightTitle, appItems, sideItems, _differs, comparable, side) {
  const row = block.querySelector('[data-part="' + part + '"]');
  if (!row) return;
  const views = row.querySelectorAll(".team-view");
  const folderItems = comparable ? (sideItems || []) : null;
  const folderName = comparable ? side.name : "";
  const appBy = agentsByID(appItems);
  const folderBy = comparable ? agentsByID(sideItems) : null;
  const order = shareOrder(appItems, comparable ? sideItems : null);
  fillAgentList(views[0].querySelector("ul"), order, appBy, folderBy, folderName, part);
  if (!comparable) {
    views[1].querySelector("ul").replaceChildren(el("li", "hint", side && side.missing ? "This folder is not in the team folder." : "Choose a member."));
  } else {
    fillAgentList(views[1].querySelector("ul"), order, folderBy, appBy, folderName, part);
  }
  setColumnHeading(views[0], leftTitle);
  setColumnHeading(views[1], rightTitle);
  const toFolder = row.querySelector("[data-sync='push']");
  const toApp = row.querySelector("[data-sync='pull']");
  if (toFolder) toFolder.dataset.folder = folderName;
  if (toApp) toApp.dataset.folder = folderName;
  syncAgentArrows(row);
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
  const userOff = !!(user.differs || user.reposDiffer);
  const memberOff = !!(member.name && member.name !== user.name && (member.missing || member.differs || member.reposDiffer));
  paintShareBadge(!!user.name && !userOff && !memberOff, true);
  paintPair("user-compare", "This app and " + (user.name || "your folder"), user.name ? user.name + "'s agents" : "Your agents", app, user);
  if (!yours) paintPair("member-compare", member.name ? "This app and " + member.name : "This app and a member", member.name ? member.name + "'s agents" : "Member agents", app, member);
  const note = document.querySelector("#agent-member-note");
  const select = document.querySelector("#agent-member");
  if (!note || (select && select.disabled)) return;
  if (!(share.members || []).length) return;
  if (member.missing) note.textContent = member.name + " is no longer in the team folder. Choose another.";
  else if (!member.name) note.textContent = "Choose a member to compare that folder with this app.";
  else if (member.differs && member.reposDiffer) note.textContent = member.name + "'s agents and repositories differ from this app.";
  else if (member.differs) note.textContent = member.name + "'s agents differ from this app's agents.";
  else if (member.reposDiffer) note.textContent = member.name + "'s repositories differ from this app's repositories.";
  else note.textContent = member.name + "'s agents and repositories match this app.";
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
  const actions = el("div", "actions repo-save-bar");
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

function repoDividedBox(kind) {
  return document.querySelector(`#panel-repos [data-repo-divided="${CSS.escape(kind)}"]`);
}

function reposDirty() {
  if (!state.data) return false;
  for (const repo of state.data.repos) {
    const box = repoDividedBox(repo.kind);
    if (!box) return false;
    if (box.checked !== !!repo.divided) return true;
    const guideList = document.querySelector(`#panel-repos [data-repo-guide-list="${CSS.escape(repo.kind)}"]`);
    if (guideList && !sameGuideList(guideValues(guideList), repo.guides || [])) return true;
    if (!box.checked) continue;
    const tags = document.querySelector(`#panel-repos [data-services="${CSS.escape(repo.kind)}"]`);
    if (!tags) return false;
    const selected = [...tags.querySelectorAll("button[aria-pressed='true']")].map((item) => item.dataset.name);
    if (!sameNames(selected, repo.services || [])) return true;
    for (const service of selected) {
      const saved = (repo.serviceGuides || []).find((row) => row.service === service);
      const sharedBox = document.querySelector(`#panel-repos [data-service-guide-shared="${CSS.escape(repo.kind)}"][data-guide-service="${CSS.escape(service)}"]`);
      const shared = !sharedBox || sharedBox.checked;
      const savedShared = saved ? !!saved.shared : true;
      if (shared !== savedShared) return true;
      if (shared) continue;
      const own = document.querySelector(`#panel-repos [data-service-guide-list="${CSS.escape(repo.kind)}"][data-guide-service="${CSS.escape(service)}"]`);
      const savedNames = (saved && saved.guides) || [];
      if (!sameGuideList(guideValues(own), savedNames)) return true;
    }
  }
  return false;
}

function guideValues(list) {
  if (!list) return [];
  return [...list.querySelectorAll("input[data-guide-name]")].map((input) => input.value.trim()).filter(Boolean);
}

function sameGuideList(left, right) {
  return left.length === right.length && left.every((name, index) => name === right[index]);
}

function collectedGuides(repo) {
  const list = document.querySelector(`#panel-repos [data-repo-guide-list="${CSS.escape(repo.kind)}"]`);
  if (!list) {
    return {
      guides: repo.guides || [],
      serviceGuides: repo.divided ? (repo.serviceGuides || []) : [],
    };
  }
  const guides = guideValues(list);
  const box = repoDividedBox(repo.kind);
  if (!box || !box.checked) return { guides, serviceGuides: [] };
  const services = [...document.querySelectorAll(`#panel-repos [data-services="${CSS.escape(repo.kind)}"] button[aria-pressed="true"]`)].map((item) => item.dataset.name);
  const serviceGuides = services.map((service) => {
    const sharedBox = document.querySelector(`#panel-repos [data-service-guide-shared="${CSS.escape(repo.kind)}"][data-guide-service="${CSS.escape(service)}"]`);
    const shared = !sharedBox || sharedBox.checked;
    const own = document.querySelector(`#panel-repos [data-service-guide-list="${CSS.escape(repo.kind)}"][data-guide-service="${CSS.escape(service)}"]`);
    return { service, shared, guides: shared ? [] : guideValues(own) };
  });
  return { guides, serviceGuides };
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

function paintGenerateIcons() {
  for (const button of document.querySelectorAll(".field-generate")) paintGenerateButton(button);
}

function generateSubject(button) {
  const current = button.dataset.generateSubject || "";
  const kind = button.dataset.generate;
  if ((kind !== "prompt" && kind !== "prompt_field" && kind !== "prompt_input" && kind !== "prompt_output") || !button.closest("#agent-dialog")) return current;
  const name = document.querySelector("#agent-name").value.trim();
  const id = state.creatingAgent ? newAgentId() : state.editingAgentId;
  return name || id || current;
}

function agentBrief(name, id, description) {
  const title = (name || "").trim();
  const slug = (id || "").trim();
  const who = title && slug && title !== slug ? title + " (" + slug + ")" : (title || slug);
  const about = (description || "").trim();
  if (!who) return about;
  if (!about) return who;
  return who + ": " + about;
}

function handoffValue(button, part) {
  const row = button.closest(".handoff");
  const area = row && row.querySelector("textarea[data-part='" + part + "']");
  return area ? area.value.trim() : "";
}

function rewriteContext(button) {
  const kind = button.dataset.generate;
  if (!["teamwork", "teamwork_when", "teamwork_reply"].includes(kind)) return {};
  const peerId = button.dataset.generateSubject || "";
  const peer = agentById(peerId);
  const name = document.querySelector("#agent-name").value.trim();
  const id = state.creatingAgent ? newAgentId() : state.editingAgentId;
  return {
    prompt: agentPromptText(),
    agent: agentBrief(name, id, document.querySelector("#agent-description").value),
    peer: agentBrief((peer && (peer.title || peer.name)) || "", peerId, peer && peer.summary),
    when: handoffValue(button, "when"),
    send: handoffValue(button, "send"),
  };
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
  for (const area of document.querySelectorAll("#agent-dialog .compose textarea, #agent-dialog .compose input:not([type='checkbox']), #task-dialog .compose textarea, #task-start-dialog .compose textarea")) {
    const compose = area.closest(".compose");
    if (!compose) continue;
    compose.classList.toggle("has-scroll", area.scrollHeight > area.clientHeight + 1);
  }
}

const generateRuns = new Map();
const generateUndo = new WeakMap();

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

async function openAgentDiff(id, folder, part) {
  const kind = part === "repos" ? "repos" : "agents";
  const data = await api("/api/team/diff?folder=" + encodeURIComponent(folder) + "&id=" + encodeURIComponent(id) + "&part=" + kind);
  document.querySelector("#team-share-diff-title").textContent = kind === "repos" ? repoTitle(id) : (data.name || id);
  const noun = kind === "repos" ? "repositories" : "agents";
  showSideBySideDiff(data.diff || [], {
    grid: document.querySelector("#team-share-diff-grid"),
    titles: ["This app", (data.folder || folder) + "'s " + noun],
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

function storeGeneratedText(button, value) {
  if (button.dataset.generate === "repo") state.repoNotes[button.dataset.generateSubject] = value;
  if (["teamwork", "teamwork_when", "teamwork_reply"].includes(button.dataset.generate)) {
    rememberCollaborations();
    syncHandoffRows();
  }
  const spec = useSections[button.dataset.generate];
  if (!spec) return;
  if (!state[spec.state]) state[spec.state] = {};
  const area = button.closest(".field") && button.closest(".field").querySelector("textarea");
  const id = (area && area.dataset.use) || button.dataset.generateSubject;
  state[spec.state][id] = value;
}

function paintGenerateButton(button) {
  if (button.classList.contains("busy")) return;
  const step = generateUndo.get(button);
  const label = step ? (step.applied ? "Undo" : "Redo") : "Rewrite by AI";
  button.textContent = label;
  button.setAttribute("aria-label", label);
  button.classList.toggle("undo", !!step);
}

function setGenerateValue(button, area, value) {
  area.value = value;
  storeGeneratedText(button, value);
  placeGenerateButtons();
  if (area.closest("#agent-prompt-fields")) syncAgentPromptMirror();
}

function generateButtonFor(target) {
  const block = target.closest && target.closest(".field");
  const own = block && block.querySelector(".field-generate");
  if (own) return own;
  const item = target.closest && target.closest(".prompt-item");
  return item && item.querySelector(".prompt-item-fields > .field-generate");
}

function resetGenerateUndo(target) {
  const button = generateButtonFor(target);
  const step = button && generateUndo.get(button);
  if (!step) return;
  if (step.fields) {
    const label = promptFieldLabel(target);
    const stored = label && step.fields[label];
    if (!stored || target.value === (step.applied ? stored.rewritten : stored.previous)) return;
  } else if (target.value === (step.applied ? step.rewritten : step.previous)) {
    return;
  }
  generateUndo.delete(button);
  paintGenerateButton(button);
}

function prepareGenerate(button) {
  if (button.classList.contains("busy")) return;
  if (button.dataset.generate === "prompt_input" || button.dataset.generate === "prompt_output") {
    prepareItemGenerate(button);
    return;
  }
  const block = button.closest(".field");
  const area = block && block.querySelector("textarea, input");
  if (!area) return;
  const host = button.closest("dialog");
  const error = host ? host.querySelector(".result.error") : null;
  const step = generateUndo.get(button);
  if (step) {
    step.applied = !step.applied;
    setGenerateValue(button, area, step.applied ? step.rewritten : step.previous);
    paintGenerateButton(button);
    area.focus();
    return;
  }
  const text = area.value.trim();
  if (!text) {
    if (error) {
      error.hidden = false;
      error.textContent = "Enter some text first.";
    }
    return;
  }
  if (error) {
    error.hidden = true;
    error.textContent = "";
  }
  rewriteText(button, area, text, error);
}

function stopGenerateRuns() {
  for (const controller of [...generateRuns.values()]) controller.abort();
}

async function readGenerateResult(response) {
  if (!response.ok) {
    const data = await response.json().catch(() => ({}));
    throw new Error(data.error || response.statusText);
  }
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let result = null;
  while (result === null) {
    const chunk = await reader.read();
    if (chunk.done) break;
    buffer += decoder.decode(chunk.value, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop();
    for (const line of lines) {
      if (!line.trim()) continue;
      const event = JSON.parse(line);
      if (event.type === "error") throw new Error(event.error || "Generate failed.");
      if (event.type === "done") {
        result = event.text || "";
        break;
      }
    }
  }
  if (result === null) throw new Error("Generate failed.");
  return result;
}

function generateRequest(button, text) {
  return {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      kind: button.dataset.generate,
      subject: generateSubject(button),
      text,
      ...rewriteContext(button),
    }),
  };
}

function repoRewriteField(kind) {
  if (!kind) return null;
  const area = document.querySelector("#agent-repos textarea[data-repo='" + CSS.escape(kind) + "']");
  if (!area) return null;
  const compose = area.closest(".compose");
  return { area, compose, button: compose && compose.querySelector(".field-generate") };
}

function releaseRewriteField(area, compose, button, readOnly) {
  if (area) {
    area.readOnly = readOnly;
    area.removeAttribute("aria-busy");
  }
  if (compose) compose.classList.remove("ai-rewriting");
  if (button && button.isConnected) {
    button.disabled = false;
    button.classList.remove("busy");
    paintGenerateButton(button);
  }
}

function resumeRepoRewrites() {
  for (const button of generateRuns.keys()) {
    if (button.dataset.generate !== "repo" || button.isConnected) continue;
    const live = repoRewriteField(button.dataset.generateSubject);
    if (!live) continue;
    live.area.readOnly = true;
    live.area.setAttribute("aria-busy", "true");
    if (live.compose) live.compose.classList.add("ai-rewriting");
    if (live.button) {
      live.button.disabled = true;
      live.button.classList.add("busy");
    }
  }
}

async function rewriteText(button, area, text, error) {
  const controller = new AbortController();
  generateRuns.set(button, controller);
  const wasReadOnly = area.readOnly;
  const compose = area.closest(".compose") || area;
  const previous = area.value;
  const repoKind = button.dataset.generate === "repo" ? (button.dataset.generateSubject || area.dataset.repo) : "";
  area.readOnly = true;
  area.setAttribute("aria-busy", "true");
  compose.classList.add("ai-rewriting");
  button.disabled = true;
  button.classList.add("busy");
  try {
    const response = await kikiFetch("/api/generate", { ...generateRequest(button, text), signal: controller.signal });
    const result = await readGenerateResult(response);
    let value = result;
    const live = area.isConnected ? null : repoRewriteField(repoKind);
    const targetArea = area.isConnected ? area : live && live.area;
    const targetButton = area.isConnected ? button : (live && live.button) || button;
    if (targetArea) {
      if (targetArea.tagName === "INPUT") value = value.replace(/\s+/g, " ").trim();
      generateUndo.set(targetButton, { previous, rewritten: value, applied: true });
      setGenerateValue(targetButton, targetArea, value);
      const block = targetArea.closest(".field");
      const toggle = block && block.querySelector("[data-repo-preview]");
      const preview = block && block.querySelector(".repo-preview");
      if (toggle && preview && toggle.getAttribute("aria-pressed") === "true") preview.innerHTML = renderMarkdown(value);
    } else if (repoKind) {
      if (!state.repoNotes) state.repoNotes = {};
      state.repoNotes[repoKind] = value;
    }
  } catch (err) {
    if (err.name === "AbortError") return;
    if (error && error.isConnected) {
      error.hidden = false;
      error.textContent = err.message;
    }
  } finally {
    if (generateRuns.get(button) === controller) generateRuns.delete(button);
    releaseRewriteField(area, compose, button, wasReadOnly);
    if (!area.isConnected && repoKind) {
      const live = repoRewriteField(repoKind);
      if (live) releaseRewriteField(live.area, live.compose, live.button, false);
    }
  }
}

function prepareItemGenerate(button) {
  const item = button.closest(".prompt-item");
  if (!item) return;
  const host = button.closest("dialog");
  const error = host ? host.querySelector(".result.error") : null;
  const step = generateUndo.get(button);
  if (step && step.fields) {
    step.applied = !step.applied;
    applyItemValues(item, step.fields, step.applied ? "rewritten" : "previous");
    paintGenerateButton(button);
    return;
  }
  if (!promptItemEntries(item).some((entry) => entry.control.value.trim())) {
    if (error) {
      error.hidden = false;
      error.textContent = "Enter some text first.";
    }
    return;
  }
  if (error) {
    error.hidden = true;
    error.textContent = "";
  }
  rewriteItem(button, item, promptItemSource(item), error);
}

async function rewriteItem(button, item, text, error) {
  const controller = new AbortController();
  generateRuns.set(button, controller);
  const entries = promptItemEntries(item);
  const previous = {};
  for (const entry of entries) {
    previous[entry.label.toLowerCase()] = entry.control.value;
    entry.control.readOnly = true;
    entry.control.setAttribute("aria-busy", "true");
    const compose = entry.control.closest(".compose");
    if (compose) compose.classList.add("ai-rewriting");
  }
  button.disabled = true;
  button.classList.add("busy");
  try {
    const response = await kikiFetch("/api/generate", { ...generateRequest(button, text), signal: controller.signal });
    const result = await readGenerateResult(response);
    if (!item.isConnected) return;
    const rewritten = promptItemRewriteValues(item, result);
    if (!rewritten) throw new Error("The rewrite did not keep the fields.");
    const fields = {};
    for (const entry of entries) {
      const key = entry.label.toLowerCase();
      fields[key] = { previous: previous[key], rewritten: rewritten[key] };
    }
    generateUndo.set(button, { fields, applied: true });
    applyItemValues(item, fields, "rewritten");
  } catch (err) {
    if (err.name === "AbortError") return;
    if (error && error.isConnected) {
      error.hidden = false;
      error.textContent = err.message;
    }
  } finally {
    if (generateRuns.get(button) === controller) generateRuns.delete(button);
    for (const entry of entries) {
      entry.control.readOnly = false;
      entry.control.removeAttribute("aria-busy");
      const compose = entry.control.closest(".compose");
      if (compose) compose.classList.remove("ai-rewriting");
    }
    if (button.isConnected) {
      button.disabled = false;
      button.classList.remove("busy");
      paintGenerateButton(button);
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
  select.setAttribute("aria-label", "Index for " + (repoTitle(repo.kind)));
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
  input.setAttribute("aria-label", "Folder for " + (repoTitle(repo.kind)));
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
  section.append(repoServicesSection(repo));
  section.append(repoGuidesSection(repo));
  return section;
}

function repoServicesSection(repo) {
  const section = el("div", "services");
  const choice = el("label", "check-line");
  const box = el("input");
  box.type = "checkbox";
  box.dataset.repoDivided = repo.kind;
  box.checked = !!repo.divided;
  choice.append(box, el("span", "", "This repository is divided into services"));
  section.append(choice);
  const body = el("div", "repo-services-body");
  body.dataset.repoServicesBody = repo.kind;
  body.hidden = !repo.divided;
  body.append(el("p", "hint", "KIKI finds services in this repository with Nx."));
  const tags = el("div", "service-tags");
  tags.dataset.services = repo.kind;
  const note = el("p", "hint", "");
  note.dataset.serviceNote = repo.kind;
  body.append(tags, note);
  section.append(body);
  if (repo.divided) paintRepoServices(section, repo);
  return section;
}

function repoGuidesSection(repo) {
  const section = el("div", "repo-guides");
  section.dataset.repoGuides = repo.kind;
  section.append(el("p", "repo-region", "Guide files"));
  section.append(el("p", "hint", "Each name is a markdown file in this repository, such as AGENTS.md. The launch prompt tells the agent to read these files and follow them."));
  const list = el("div", "guide-list");
  list.dataset.repoGuideList = repo.kind;
  for (const name of repo.guides || []) list.append(guideRow(repo.kind, "", name));
  const add = el("button", "", "Add guide file");
  add.type = "button";
  add.dataset.addGuide = repo.kind;
  const services = el("div", "service-guides");
  services.dataset.serviceGuides = repo.kind;
  section.append(list, add, services);
  paintServiceGuides(section, repo);
  return section;
}

function guideRow(kind, service, value) {
  const row = el("div", "guide-row");
  const input = el("input");
  input.type = "text";
  input.spellcheck = false;
  input.autocomplete = "off";
  input.placeholder = "AGENTS.md";
  input.value = value || "";
  input.dataset.guideName = kind;
  if (service) input.dataset.guideService = service;
  const remove = el("button", "", "Remove");
  remove.type = "button";
  remove.dataset.removeGuide = kind;
  row.append(input, remove);
  return row;
}

function paintServiceGuides(section, repo) {
  const host = section.querySelector("[data-service-guides]");
  if (!host) return;
  const kind = repo.kind;
  const divided = repoDividedBox(kind);
  const on = divided ? divided.checked : !!repo.divided;
  const tags = section.closest(".repo-folder") && section.closest(".repo-folder").querySelector("[data-services]");
  const selected = tags
    ? [...tags.querySelectorAll("button[aria-pressed='true']")].map((item) => item.dataset.name)
    : (repo.services || []);
  const previous = new Map();
  for (const block of host.querySelectorAll("[data-service-guide]")) {
    const service = block.dataset.serviceGuide;
    const shared = block.querySelector("[data-service-guide-shared]");
    previous.set(service, {
      shared: !shared || shared.checked,
      names: [...block.querySelectorAll("input[data-guide-name]")].map((input) => input.value),
    });
  }
  host.replaceChildren();
  if (!on || !selected.length) {
    host.hidden = true;
    return;
  }
  host.hidden = false;
  host.append(el("p", "hint", "Each selected service can use the repository guide files, or its own."));
  for (const service of selected) {
    const saved = (repo.serviceGuides || []).find((row) => row.service === service);
    const draft = previous.get(service);
    const shared = draft ? draft.shared : (saved ? !!saved.shared : true);
    const names = draft ? draft.names : ((saved && !saved.shared && saved.guides) || []);
    host.append(serviceGuideBlock(kind, service, shared, names));
  }
}

function serviceGuideBlock(kind, service, shared, names) {
  const block = el("div", "service-guide");
  block.dataset.serviceGuide = service;
  const choice = el("label", "check-line");
  const box = el("input");
  box.type = "checkbox";
  box.dataset.serviceGuideShared = kind;
  box.dataset.guideService = service;
  box.checked = shared;
  choice.append(box, el("span", "", service + " uses the repository guide files"));
  const own = el("div", "service-guide-own");
  own.hidden = shared;
  const list = el("div", "guide-list");
  list.dataset.serviceGuideList = kind;
  list.dataset.guideService = service;
  for (const name of names) list.append(guideRow(kind, service, name));
  const add = el("button", "", "Add guide file");
  add.type = "button";
  add.dataset.addGuide = kind;
  add.dataset.guideService = service;
  own.append(el("p", "hint", "These files are for " + service + ". The launch prompt tells the agent to follow them inside that service."), list, add);
  block.append(choice, own);
  return block;
}

function refreshServiceGuides(kind) {
  const section = document.querySelector(`#panel-repos [data-repo-guides="${CSS.escape(kind)}"]`);
  const repo = ((state.data && state.data.repos) || []).find((item) => item.kind === kind) || { kind, serviceGuides: [] };
  if (section) paintServiceGuides(section, repo);
}

function paintRepoServices(section, repo) {
  const tags = section.querySelector("[data-services]");
  const note = section.querySelector("[data-service-note]");
  if (!tags || !note) return;
  if (!(repo.folder || "").trim()) {
    note.textContent = "Set a folder before services can be found.";
    tags.replaceChildren();
    return;
  }
  fillServiceTags(tags, note, repo.available || [], repo.services || []);
}

async function toggleRepoDivided(box) {
  const kind = box.dataset.repoDivided;
  const section = box.closest(".services");
  const body = section && section.querySelector("[data-repo-services-body]");
  if (body) body.hidden = !box.checked;
  if (box.checked && section) await loadRepoServices(kind, section);
  refreshServiceGuides(kind);
  syncSave();
}

async function loadRepoServices(kind, section) {
  const tags = section.querySelector("[data-services]");
  const note = section.querySelector("[data-service-note]");
  if (!tags || !note) return;
  const input = document.querySelector(`#panel-repos [data-repo-folder="${CSS.escape(kind)}"]`);
  const folder = input ? input.value.trim() : "";
  if (!folder) {
    note.textContent = "Set a folder before services can be found.";
    tags.replaceChildren();
    return;
  }
  note.textContent = "Looking for Nx projects…";
  try {
    const data = await api("/api/repos/services", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ folder }),
    });
    const repo = (state.data.repos || []).find((item) => item.kind === kind);
    const pressed = [...tags.querySelectorAll("button[aria-pressed='true']")].map((item) => item.dataset.name);
    const chosen = pressed.length ? pressed : ((repo && repo.services) || []);
    fillServiceTags(tags, note, data.services || [], chosen);
    refreshServiceGuides(kind);
  } catch (error) {
    note.textContent = error.message;
    tags.replaceChildren();
  }
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
  const settingUp = repoSettingUp(repo.kind);
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
    const name = repoTitle(repo.kind);
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
      if (using && badgeText(item) === "Not ready") actions.append(indexSetupButton(item));
      const refresh = el("button", "icon-button");
      refresh.type = "button";
      refresh.dataset.refreshRepo = repo.kind;
      const busy = state.checkingIndexes.has(repo.kind);
      refresh.disabled = busy;
      refresh.classList.toggle("spin", busy);
      refresh.setAttribute("aria-busy", busy ? "true" : "false");
      refresh.setAttribute("aria-label", "Check " + indexLabel(item.index) + " on " + (repoTitle(repo.kind)));
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
    const settingUp = repoSettingUp(kind);
    const pending = state.pendingRepoIndex && state.pendingRepoIndex.kind === kind;
    select.disabled = settingUp || pending;
    if (settingUp) select.setAttribute("aria-busy", "true");
    else select.removeAttribute("aria-busy");
  }
  for (const toggle of document.querySelectorAll("#panel-repos [data-repo-index-use]")) {
    const kind = toggle.dataset.repoIndexUse;
    const settingUp = repoSettingUp(kind);
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
  if (repoSettingUp(kind) || (state.pendingRepoIndex && state.pendingRepoIndex.kind === kind)) {
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
  if (!kind || repoSettingUp(kind) || state.checkingIndexes.has(kind)) return;
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
  const name = repoTitle(kind);
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
  if (!kind || repoSettingUp(kind) || state.checkingIndexes.has(kind)) return;
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
    banner("Stopped using " + indexLabel(saved) + " on " + (repoTitle(kind)) + ".");
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
    banner("Using " + indexLabel(tool) + " already in " + (repoTitle(kind)) + ".");
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
  const name = repoTitle(kind);
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
    banner("Pointed " + (repoTitle(kind)) + " at " + folder + ".");
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

function indexSetupButton(item) {
  const button = el("button", "repo-index-setup", "Set up");
  button.type = "button";
  button.dataset.setupIndex = item.kind;
  button.dataset.setupTool = item.index;
  const name = repoTitle(item.kind);
  button.setAttribute("aria-label", "Set up " + indexLabel(item.index) + " for " + name);
  return button;
}

function indexRemoveButton(item) {
  const remove = el("button", "icon-button danger");
  remove.type = "button";
  remove.dataset.clearIndex = item.kind;
  const label = "Remove " + (repoTitle(item.kind));
  remove.setAttribute("aria-label", label);
  remove.title = label;
  remove.append(trashIcon());
  return remove;
}

function editIcon() {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  for (const d of [
    "M12 20h9",
    "M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z",
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

const pageWorkflow = {
  name: "page",
  root() { return document.querySelector("#panel-workflow"); },
  edges: [],
  selection: null,
  draft: null,
};
const agentWorkflow = {
  name: "agent",
  root() { return document.querySelector("#agent-topology"); },
  edges: [],
  selection: null,
  draft: null,
};
let workflowLoad = 0;
const workflowObserver = new ResizeObserver(() => layoutWorkflow(pageWorkflow));
const agentWorkflowObserver = new ResizeObserver(() => {
  const root = agentWorkflow.root();
  if (!root || root.closest("[hidden]")) return;
  layoutWorkflow(agentWorkflow);
});

function agentTeamwork(agent) {
  if (Array.isArray(agent.teamwork)) return agent.teamwork;
  return (agent.worksWith || []).map((id) => ({ id, handoffs: [] }));
}

function repoNoteText(use) {
  return (use && use.description) || "";
}

function handoffsText(peer) {
  const pairs = (peer && peer.handoffs) || [];
  return pairs.map((pair, index) => {
    const lines = [];
    if (pairs.length > 1) lines.push("Message " + (index + 1));
    lines.push("Send when: " + (pair.when || "(no condition set)"));
    lines.push("Send: " + (pair.send || ""));
    lines.push(pair.reply ? "Reports back with: " + pair.reply : "Reports back: no");
    if (pair.reply && pair.maxLoops > 0) lines.push("Loops: up to " + pair.maxLoops);
    return lines.join("\n");
  }).join("\n\n");
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
  pageWorkflow.selection = null;
  pageWorkflow.draft = null;
  pageWorkflow.edges = [];
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
    workflowAgentColumn(data, pageWorkflow.edges),
    workflowGuardrailColumn(),
    workflowUseColumn(data, "mcp", "MCP Servers", "No agent lists an MCP server."),
    workflowUseColumn(data, "tool", "Tools", "No agent lists a tool."),
    workflowRepoColumn(data, pageWorkflow.edges),
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
  paintWorkflowDetail(pageWorkflow);
  if (!panel.hidden) requestAnimationFrame(() => layoutWorkflow(pageWorkflow));
}

function workflowSwatch(kind, label) {
  const item = el("span", "workflow-swatch");
  const mark = el("i", "workflow-swatch-mark " + kind);
  mark.setAttribute("aria-hidden", "true");
  item.append(mark, document.createTextNode(label));
  return item;
}

function workflowAgentColumn(data, edges) {
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
      col.append(workflowAgentNode(agent));
      pushAgentWorkflowEdges(agent, edges);
    }
  }
  return col;
}

function workflowAgentNode(agent, options) {
  const current = !!(options && options.current);
  const showPrompt = !options || options.prompt !== false;
  const card = el("div", "workflow-node" + (agent.enabled === false ? " is-off" : "") + (current ? " is-current" : ""));
  card.dataset.flow = "agent";
  card.dataset.id = agent.id;
  const open = el("button", "workflow-open");
  open.type = "button";
  if (!showPrompt) open.classList.add("plain");
  open.setAttribute("aria-pressed", "false");
  const row = el("span", "workflow-name-row");
  const title = el("span", "workflow-title-line");
  title.append(el("span", "workflow-name", agent.title || agent.id));
  if (current) title.append(el("span", "badge ok", "This agent"));
  if (agent.enabled === false) title.append(el("span", "badge bad", "Off"));
  row.append(title);
  open.append(row, el("span", "id", agent.id));
  const choice = workflowChoice(agent);
  if (choice) open.append(el("span", "workflow-meta", choice));
  card.append(open);
  if (showPrompt) {
    const prompt = el("button", "icon-button workflow-prompt");
    prompt.type = "button";
    prompt.dataset.prompt = agent.id;
    prompt.setAttribute("aria-label", "Show the prompt for " + (agent.title || agent.id));
    prompt.append(promptIcon());
    card.append(prompt);
  }
  return card;
}

function pushAgentWorkflowEdges(agent, edges) {
  for (const peer of agentTeamwork(agent)) {
    if (!peer.id || peer.id === agent.id) continue;
    edges.push({
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
      note: handoffsText(peer),
    });
  }
  for (const use of agentRepositoryUses(agent)) {
    if (!use.kind) continue;
    edges.push({
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
      title: agent.title + " → " + (repoTitle(use.kind)),
      note: repoNoteText(use),
    });
  }
  pushWorkflowUses(agent, "mcp", configuredUses("mcp", agent.mcps || []), edges);
  pushWorkflowUses(agent, "tool", agent.tools || [], edges);
}

function pushWorkflowUses(agent, kind, items, edges) {
  const kicker = kind === "mcp" ? "MCP Servers" : "Tool";
  for (const item of items) {
    if (!item.id) continue;
    edges.push({
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

function workflowRepoColumn(data, edges) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", "Repositories"));
  const kinds = workflowRepoOrder(data);
  if (!kinds.length) {
    col.append(el("p", "hint", "No repositories yet."));
    return col;
  }
  for (const kind of kinds) {
    const repo = workflowRepo(kind);
    const name = repoTitle(kind);
    const card = workflowCard("repo", kind);
    const open = card.querySelector(".workflow-open");
    open.append(el("span", "workflow-name", name));
    const indexName = repo && repo.index ? indexLabel(repo.index) : "No index";
    open.append(el("span", "workflow-meta", indexName));
    card.append(workflowAboutButton("repo", kind, name));
    col.append(card);
    const index = repo && repo.index ? repo.index : "none";
    edges.push({
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
      title: (repoTitle(kind)) + " → " + (index === "none" ? "Not set" : indexLabel(index)),
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

function layoutWorkflow(view) {
  const root = view.root();
  const stage = root && root.querySelector(".workflow-stage");
  const svg = stage && stage.querySelector(".workflow-lines");
  if (!svg || stage.getClientRects().length === 0) return;
  placeGuardrail(stage, view);
  svg.replaceChildren(workflowMarkers(view.name));
  if (view.selection) {
    const show = workflowRelated(view.selection, view);
    const linked = new Set();
    const fan = {};
    view.edges.forEach((edge, index) => {
      if (!show.edges.has(index)) return;
      if (edge.kind === "repository" || edge.kind === "teamwork" || edge.kind === "mcp" || edge.kind === "tool") {
        drawThroughGuardrail(stage, svg, edge, index, linked, fan, view.name);
        return;
      }
      drawWorkflowEdge(stage, svg, edge, index, true, view.name);
    });
  }
  paintWorkflowHighlight(view);
}

function placeGuardrail(stage, view) {
  const card = stage.querySelector(".workflow-guardrail");
  if (!card) return;
  const sel = view.selection;
  let anchorId = "";
  if (sel && sel.kind === "agent") anchorId = sel.id;
  if (sel && sel.kind === "edge") {
    const edge = view.edges[sel.id];
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

function drawThroughGuardrail(stage, svg, edge, index, linked, fan, marker) {
  const guard = stage.querySelector('.workflow-node[data-flow="guardrail"]');
  const from = stage.querySelector(`.workflow-node[data-flow="${edge.fromType}"][data-id="${CSS.escape(edge.fromId)}"]`);
  const to = stage.querySelector(`.workflow-node[data-flow="${edge.toType}"][data-id="${CSS.escape(edge.toId)}"]`);
  if (from && guard && !linked.has(edge.fromId)) {
    linked.add(edge.fromId);
    drawCurve(svg, workflowAnchor(stage, from, "right"), workflowAnchor(stage, guard, "left"), "guardrail", -1, false, 0, marker);
  }
  if (!guard || !to) return;
  const back = edge.kind === "teamwork";
  const start = workflowAnchor(stage, guard, back ? "left" : "right");
  const end = workflowAnchor(stage, to, back ? "right" : "left");
  const n = fan[edge.fromId] || 0;
  fan[edge.fromId] = n + 1;
  drawCurve(svg, start, end, edge.kind, index, true, (n - 1) * 14, marker);
}

function drawWorkflowEdge(stage, svg, edge, index, hittable, marker) {
  const from = stage.querySelector(`.workflow-node[data-flow="${edge.fromType}"][data-id="${CSS.escape(edge.fromId)}"]`);
  const to = stage.querySelector(`.workflow-node[data-flow="${edge.toType}"][data-id="${CSS.escape(edge.toId)}"]`);
  if (!from || !to) return;
  drawCurve(svg, workflowAnchor(stage, from, edge.fromSide), workflowAnchor(stage, to, edge.toSide), edge.kind, index, hittable, 0, marker);
}

function drawCurve(svg, start, end, kind, index, hittable, bend = 0, marker = "page") {
  const svgNS = "http://www.w3.org/2000/svg";
  const c1 = start.x + (end.x - start.x) * 0.45;
  const c2 = end.x - (end.x - start.x) * 0.45;
  const d = `M ${start.x} ${start.y} C ${c1} ${start.y + bend}, ${c2} ${end.y + bend}, ${end.x} ${end.y}`;
  const path = document.createElementNS(svgNS, "path");
  path.setAttribute("d", d);
  path.setAttribute("class", "workflow-edge " + kind);
  if (index >= 0) path.dataset.edge = String(index);
  path.setAttribute("marker-end", "url(#workflow-arrow-" + marker + "-" + kind + ")");
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

function workflowMarkers(prefix) {
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
    marker.id = "workflow-arrow-" + prefix + "-" + kind;
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

function selectWorkflow(next, view) {
  const current = view.selection;
  const same = current && next && current.kind === next.kind && String(current.id) === String(next.id);
  view.selection = same ? null : next;
  applyWorkflowSelection(view);
}

function workflowRelated(sel, view) {
  const nodes = new Set();
  const edges = new Set();
  if (!sel) return { nodes, edges };
  if (sel.kind === "edge") {
    const edge = view.edges[sel.id];
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
    view.edges.forEach((edge, index) => {
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
    view.edges.forEach((edge, index) => {
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
    view.edges.forEach((edge, index) => {
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
    view.edges.forEach((edge, index) => {
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
    view.edges.forEach((edge, index) => {
      if (edge.kind === "index" && edge.toKey === key) {
        edges.add(index);
        nodes.add(edge.fromKey);
        repos.add(edge.fromKey);
      }
    });
    view.edges.forEach((edge, index) => {
      if (edge.kind === "repository" && repos.has(edge.toKey)) {
        edges.add(index);
        nodes.add(edge.fromKey);
      }
    });
  }
  return { nodes, edges };
}

function applyWorkflowSelection(view) {
  layoutWorkflow(view);
  paintWorkflowDetail(view);
}

function paintWorkflowHighlight(view) {
  const panel = view.root();
  if (!panel) return;
  const related = workflowRelated(view.selection, view);
  const active = view.selection;
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
  paintGuardrailCard(view);
}

function paintGuardrailCard(view) {
  const card = view.root() && view.root().querySelector('.workflow-node[data-flow="guardrail"] .workflow-meta');
  if (!card) return;
  card.textContent = "Before input and after output";
}

function paintWorkflowDetail(view) {
  const box = view.root() && view.root().querySelector(".workflow-detail");
  if (!box) return;
  box.replaceChildren();
  const sel = view.selection;
  if (!sel) {
    box.append(el("p", "hint", view.draft
      ? "Select a card or a line."
      : "Select an agent. The guardrail is what KIKI checks before input and after output."));
    return;
  }
  if (sel.kind === "guardrail") {
    box.append(el("p", "workflow-kicker", "Guardrail"));
    box.append(el("h3", "", "KIKI guardrail"));
    box.append(el("p", "hint", "Before a prompt is sent to the model, and after the model output, KIKI checks the agent. It allows only the MCP servers, tools, repositories, and agents set for that agent. A repository marked Readonly may be read and must not be changed. The check is code in the app. When KIKI is not running, the AI tool is not blocked."));
    return;
  }
  if (sel.kind === "edge") {
    const edge = view.edges[sel.id];
    if (!edge) {
      box.append(el("p", "hint", "Select an agent, a repository, an index, or a line."));
      return;
    }
    box.append(el("p", "workflow-kicker", edge.kicker));
    box.append(el("h3", "", edge.title));
    box.append(el("p", "workflow-note", edge.note || "No note is saved."));
    return;
  }
  if (sel.kind === "agent") paintAgentWorkflow(box, sel.id, view);
  else if (sel.kind === "repo") paintRepoWorkflow(box, sel.id, view);
  else if (sel.kind === "mcp" || sel.kind === "tool") paintUseWorkflow(box, sel.kind, sel.id, view);
  else if (sel.kind === "index") paintIndexWorkflow(box, sel.id, view);
}

function workflowAgentList(view) {
  const agents = (state.data && state.data.agents) || [];
  if (!view.draft) return agents;
  return [view.draft, ...agents.filter((agent) => agent.id !== view.draft.id)];
}

function paintAgentWorkflow(box, id, view) {
  const agent = workflowAgentList(view).find((item) => item.id === id);
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
    block.append(el("p", "workflow-note", handoffsText(peer) || "No message is saved."));
    box.append(block);
  }
  box.append(el("h4", "", "Repositories"));
  if (!uses.length) box.append(el("p", "hint", "This agent does not list a repository."));
  for (const use of uses) {
    const repo = workflowRepo(use.kind);
    const index = repo && repo.index ? indexLabel(repo.index) : "not set";
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", (repoTitle(use.kind)) + " · " + index));
    block.append(el("p", "workflow-note", repoNoteText(use) || "No note is saved."));
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

function paintUseWorkflow(box, kind, id, view) {
  box.append(el("p", "workflow-kicker", kind === "mcp" ? "MCP Servers" : "Tool"));
  box.append(el("h3", "", workflowUseName(kind, id)));
  box.append(el("p", "id", id));
  box.append(el("h4", "", "Used by"));
  const users = workflowAgentList(view).filter((agent) => {
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

function paintRepoWorkflow(box, kind, view) {
  const repo = workflowRepo(kind);
  box.append(el("p", "workflow-kicker", "Repository"));
  box.append(el("h3", "", repoTitle(kind)));
  box.append(workflowPath(repo && repo.folder));
  const remote = workflowRemote(repo && repo.git);
  if (remote) box.append(remote);
  box.append(el("p", "hint", "Index: " + (repo && repo.index ? indexLabel(repo.index) : "not set")));
  box.append(el("h4", "", "Used by"));
  const users = workflowAgentList(view).filter((agent) => agentRepositoryUses(agent).some((use) => use.kind === kind));
  if (!users.length) box.append(el("p", "hint", "No agent lists this repository."));
  for (const agent of users) {
    const use = agentRepositoryUses(agent).find((item) => item.kind === kind);
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", agent.title));
    block.append(el("p", "workflow-note", (use && repoNoteText(use)) || "No note is saved."));
    box.append(block);
  }
}

function paintIndexWorkflow(box, id, view) {
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
    const users = workflowAgentList(view).filter((agent) => agentRepositoryUses(agent).some((use) => use.kind === kind));
    const block = el("div", "workflow-block");
    block.append(el("p", "workflow-title", repoTitle(kind)));
    block.append(workflowPath(repo && repo.folder));
    if (users.length) block.append(el("p", "hint", "Agents: " + users.map((agent) => agent.title).join(", ")));
    box.append(block);
  }
}

document.querySelector("#panel-workflow").addEventListener("click", (event) => {
  if (event.target.closest("[data-prompt], [data-guardrail-view], [data-flow-about], .index-tool-link")) return;
  const edge = event.target.closest(".workflow-hit");
  if (edge) {
    selectWorkflow({ kind: "edge", id: Number(edge.dataset.edge) }, pageWorkflow);
    return;
  }
  const node = event.target.closest(".workflow-node");
  if (node) {
    selectWorkflow({ kind: node.dataset.flow, id: node.dataset.id }, pageWorkflow);
    return;
  }
  if (event.target.closest("#workflow-detail")) return;
  selectWorkflow(null, pageWorkflow);
});

document.querySelector("#agent-topology").addEventListener("click", (event) => {
  if (event.target.closest(".index-tool-link")) return;
  if (event.target.closest(".workflow-node, .workflow-hit")) event.preventDefault();
});

function unlockNavigation() {
  for (const button of document.querySelectorAll("[data-tab='repos'], [data-tab='workflow'], #open-settings")) {
    button.disabled = false;
  }
}

function showTab(name) {
  if (!state.data && name !== "agents" && name !== "tasks") return;
  const enterRepos = name === "repos" && state.tab !== "repos";
  const enterWorkflow = name === "workflow" && state.tab !== "workflow";
  const enterAgents = name === "agents" && state.tab !== "agents";
  state.tab = name;
  for (const button of document.querySelectorAll("[data-tab]")) {
    button.setAttribute("aria-selected", button.dataset.tab === name ? "true" : "false");
  }
  document.querySelector("#panel-tasks").hidden = name !== "tasks";
  document.querySelector("#panel-agents").hidden = name !== "agents";
  document.querySelector("#panel-repos").hidden = name !== "repos";
  document.querySelector("#panel-workflow").hidden = name !== "workflow";
  if (enterRepos) checkIndexes();
  if (enterAgents) refreshTeamShare();
  if (name === "workflow") requestAnimationFrame(() => layoutWorkflow(pageWorkflow));
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
    repos: state.data.repos.map((repo) => {
      const box = repoDividedBox(repo.kind);
      if (!box) {
        const kept = collectedGuides(repo);
        return {
          kind: repo.kind,
          folder: repo.folder,
          divided: !!repo.divided,
          services: repo.divided ? (repo.services || []) : [],
          guides: kept.guides,
          serviceGuides: kept.serviceGuides,
        };
      }
      const divided = box.checked;
      const services = divided
        ? [...document.querySelectorAll(`#panel-repos [data-services="${CSS.escape(repo.kind)}"] button[aria-pressed="true"]`)].map((item) => item.dataset.name)
        : [];
      const guides = collectedGuides(repo);
      return { kind: repo.kind, folder: repo.folder, divided, services, guides: guides.guides, serviceGuides: guides.serviceGuides };
    }),
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
    showTab((selected && selected.dataset.tab) || "tasks");
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
    const option = el("option", "", repoTitle(kind));
    option.value = kind;
    select.append(option);
    const line = el("p", "");
    const folder = repo && repo.folder ? repo.folder : "(no folder)";
    let text = repoTitle(kind) + ": " + folder;
    if (repo && repo.divided) {
      const services = repo.services.length ? repo.services.join(", ") : "(none)";
      text += " · services: " + services;
    }
    line.textContent = text;
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
  const select = document.querySelector("#launch-repo");
  const several = select && select.options.length > 1;
  const start = orchestratorLabel() + " starts in " + repo.folder;
  node.textContent = several ? "Each repository has its own worktree. " + start + "." : start;
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
  const name = repoTitle(kind);
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
    renderTasks();
    watchTaskRuns();
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
    const tab = (selected && selected.dataset.tab) || "tasks";
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
  const tab = (selected && selected.dataset.tab) || "tasks";
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

async function postMember(name, repos, direction, part, agents, ids, remove) {
  const body = { name, direction: direction || "pull" };
  if (part) body.part = part;
  if (repos) body.repos = repos;
  if (agents) body.agents = agents;
  if (ids && ids.length) body.ids = ids;
  if (remove && remove.length) body.remove = remove;
  try {
    return await api("/api/team/member", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch (error) {
    if (part !== "repos" && direction !== "push" && error.payload && Array.isArray(error.payload.unknown) && error.payload.unknown.length) {
      const choices = await askRepoChoices(error.payload.unknown, error.payload.repos || []);
      if (!choices) return null;
      return postMember(name, choices, direction, part, agents, ids, remove);
    }
    throw error;
  }
}

function confirmMemberSync(name, direction, part, candidates) {
  const dialog = document.querySelector("#team-member-dialog");
  const push = direction === "push";
  const repos = part === "repos";
  const many = repos ? "repositories" : "agents";
  dialog.querySelector("h2").textContent = push ? "Copy " + many + " to " + name : "Copy " + many + " from " + name;
  document.querySelector("#team-member-text").textContent = repos
    ? (push
      ? "Choose which repositories to copy into " + name + "'s folder. A ticked Deleted repository is removed there. Unticked ones stay as they are. Checkout folders are not copied."
      : "Choose which repositories to copy into this app. A ticked Deleted repository is removed here. Unticked ones stay as they are. Checkout folders on this computer stay.")
    : (push
      ? "Choose which agents to copy into " + name + "'s folder. A ticked Deleted agent is removed there. Unticked ones stay as they are."
      : "Choose which agents to copy into this app. A ticked Deleted agent is removed here. Unticked ones stay as they are.");
  const allRow = document.querySelector("#team-member-all");
  const list = document.querySelector("#team-member-picks");
  const copyButton = document.querySelector("#team-member-replace");
  allRow.replaceChildren();
  list.replaceChildren();
  const allLabel = el("label", "agent-pick-all");
  const all = el("input");
  all.type = "checkbox";
  all.checked = true;
  all.setAttribute("aria-label", repos ? "Select every repository" : "Select every agent");
  allLabel.append(all, document.createTextNode("All"));
  allRow.append(allLabel);
  for (const candidate of candidates || []) {
    const item = el("li");
    const label = el("label", "copy-pick");
    const box = el("input");
    box.type = "checkbox";
    box.checked = true;
    box.dataset.copyId = candidate.id;
    box.dataset.copyName = candidate.name || candidate.id;
    if (candidate.deleted) box.dataset.copyDeleted = "1";
    box.setAttribute("aria-label", (candidate.deleted ? "Remove " : "Include ") + (candidate.name || candidate.id));
    label.append(box, el("span", "name", candidate.name || candidate.id));
    if (candidate.meta) label.append(el("span", "meta", candidate.meta));
    if (candidate.deleted) label.append(el("span", "badge bad", "Deleted"));
    item.append(label);
    list.append(item);
  }
  const boxes = () => [...list.querySelectorAll("[data-copy-id]")];
  const syncCopy = () => {
    const chosen = boxes().filter((box) => box.checked).length;
    all.checked = boxes().length > 0 && chosen === boxes().length;
    all.indeterminate = chosen > 0 && chosen < boxes().length;
    copyButton.disabled = chosen === 0;
  };
  all.onchange = () => {
    for (const box of boxes()) box.checked = all.checked;
    syncCopy();
  };
  list.onchange = syncCopy;
  syncCopy();
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
    copyButton.onclick = () => {
      const picked = boxes().filter((box) => box.checked).map((box) => ({
        id: box.dataset.copyId,
        name: box.dataset.copyName || box.dataset.copyId,
        deleted: box.dataset.copyDeleted === "1",
      }));
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish(picked);
    };
    dialog.showModal();
  });
}

async function syncFolder(name, direction, part, candidates) {
  if (!name || agentMemberBusy) return;
  const team = (state.data && state.data.teamFolder) || {};
  if (direction === "push" && name !== (team.user || "")) return;
  if (!candidates || !candidates.length) return;
  const picked = await confirmMemberSync(name, direction, part, candidates);
  if (!picked || !picked.length) return;
  await runAgentCopy(name, direction, part, picked);
}

async function runAgentCopy(name, direction, part, picked) {
  if (!name || agentMemberBusy) return;
  const kind = part === "repos" ? "repos" : "agents";
  const select = document.querySelector("#agent-member");
  const arrows = [...document.querySelectorAll(".team-arrows button")];
  const selectWasDisabled = !!(select && select.disabled);
  agentMemberBusy = true;
  if (select) select.disabled = true;
  for (const button of arrows) {
    button.disabled = true;
    if (button.dataset.folder === name && button.dataset.sync === direction && button.dataset.part === kind) button.classList.add("busy");
  }
  let copied = null;
  const copies = (picked || []).filter((item) => !item.deleted);
  const removals = (picked || []).filter((item) => item.deleted);
  try {
    const team = (state.data && state.data.teamFolder) || {};
    const ownPush = direction === "push" && name === (team.user || "");
    const ids = copies.map((item) => item.id);
    const remove = removals.map((item) => item.id);
    const body = {};
    if (kind === "agents") {
      if (ids.length) body.agents = ids;
    } else if (ids.length) body.ids = ids;
    if (remove.length) body.remove = remove;
    copied = ownPush
      ? await api(kind === "repos" ? "/api/team/repos" : "/api/team/agents", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      })
      : await postMember(name, null, direction, kind, kind === "agents" ? ids : null, kind === "repos" ? ids : null, remove);
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
    if (picked && picked.length) {
      const many = kind === "repos" ? "repositories" : "agents";
      const place = kind === "repos" ? "repositories" : "agents folder";
      const labelFor = (items) => items.length === 1 ? items[0].name : items.length + " " + many;
      if (copies.length && removals.length) {
        banner("Copied " + labelFor(copies) + " and removed " + labelFor(removals) + ".");
        return;
      }
      if (removals.length) {
        banner(direction === "push"
          ? "Removed " + labelFor(removals) + " from " + name + "'s " + place + "."
          : "Removed " + labelFor(removals) + " from this app.");
        return;
      }
      banner(direction === "push"
        ? "Copied " + labelFor(copies) + " into " + name + "'s " + place + "."
        : "Copied " + labelFor(copies) + " into this app.");
      return;
    }
    const noun = kind === "repos" ? "repositories" : "agents";
    banner(direction === "push"
      ? "Copied this app's " + noun + " into " + name + "'s " + noun + " folder."
      : "Copied " + name + "'s " + noun + " into this app.");
    return;
  }
  if (select && select.isConnected) loadAgentMembers(select.value).catch((error) => banner(error.message, true));
}

async function chooseMember(select) {
  const name = select.value;
  const previous = select.dataset.kept || "";
  if (name === previous) return;
  agentMemberChoice = name;
  select.dataset.kept = name;
  loadAgentMembers(name).catch((error) => banner(error.message, true));
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
  document.querySelector("#team-sync-text").textContent = user + " already has an agents folder in the team folder. Copy this app's agents and the repositories folder beside them into that folder, or copy those folders into this app.";
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
      const option = el("option", "", repoTitle(kind));
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

const setupWatches = new Map();

function repoSettingUp(kind) {
  return state.setupBusy.has(kind);
}

function watchJob(id, kind) {
  stopSetupWatch(id);
  const watch = { stopped: false, timer: 0 };
  const tick = async () => {
    if (watch.stopped) return;
    try {
      const job = await api("/api/jobs/" + id);
      if (watch.stopped) return;
      if (job.status === "running") return;
      stopSetupWatch(id);
      const failed = job.status !== "ok";
      try {
        const data = await api("/api/indexes/check/" + encodeURIComponent(kind), { method: "POST" });
        applyCheckedIndexes(data, [kind]);
      } catch (error) {
        banner(error.message, true);
      }
      markSetupBusy(kind, false);
      const message = job.status === "cancelled" ? "Setup cancelled." : failed ? "Setup failed." : "Setup finished.";
      banner(message, failed);
    } catch (error) {
      if (watch.stopped) return;
      stopSetupWatch(id);
      markSetupBusy(kind, false);
      banner(error.message, true);
    }
  };
  watch.timer = setInterval(tick, 1000);
  setupWatches.set(id, watch);
  tick();
}

function stopSetupWatch(id) {
  const watch = setupWatches.get(id);
  if (!watch) return;
  watch.stopped = true;
  clearInterval(watch.timer);
  setupWatches.delete(id);
}

function markSetupBusy(kind, busy) {
  if (!kind) return;
  if (busy) state.setupBusy.add(kind);
  else state.setupBusy.delete(kind);
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
  if (!kind || repoSettingUp(kind) || state.checkingIndexes.has(kind)) return;
  const item = ((state.data && state.data.indexes) || []).find((row) => row.kind === kind);
  if (!item || (item.index !== "obsidian" && item.index !== "codegraph")) return;
  const dialog = document.querySelector("#index-remove-dialog");
  dialog.dataset.kind = kind;
  const folderName = item.index === "obsidian" ? ".obsidian" : ".codegraph";
  const label = repoTitle(kind);
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
  if (!kind || repoSettingUp(kind) || state.checkingIndexes.has(kind)) return;
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
    const label = repoTitle(kind);
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

function setupRepoIndex(kind, tool, options) {
  if (!kind || repoSettingUp(kind)) return;
  if (tool !== "obsidian" && tool !== "codegraph") return;
  return startSetup(tool, kind, options);
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

async function startSetup(tool, kind, options = {}) {
  if (repoSettingUp(kind) || !kind) return;
  markSetupBusy(kind, true);
  try {
    const path = tool === "obsidian" ? "/api/indexes/obsidian" : "/api/indexes/codegraph";
    const result = await api(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ repo: kind, force: !!options.force }),
    });
    if (!result || !result.id) {
      if (result && Array.isArray(result.repos)) state.data = result;
      markSetupBusy(kind, false);
      if (result && Array.isArray(result.repos)) {
        render(result);
        banner("Using " + indexLabel(tool) + " already in " + (repoTitle(kind)) + ".");
      }
      return;
    }
    watchJob(result.id, kind);
  } catch (error) {
    markSetupBusy(kind, false);
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

const defaultMaxLoops = 3;
const maxLoopsLimit = 20;

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
  const kinds = [];
  for (const repo of (state.data && state.data.repos) || []) {
    if (repo.kind && !kinds.includes(repo.kind)) kinds.push(repo.kind);
  }
  for (const kind of repos || []) {
    if (!kinds.includes(kind)) kinds.push(kind);
  }
  return kinds;
}

function rememberRepoNotes() {
  if (!state.repoNotes) state.repoNotes = {};
  if (!state.repoReadonly) state.repoReadonly = {};
  for (const area of document.querySelectorAll("#agent-repos textarea[data-repo]")) {
    state.repoNotes[area.dataset.repo] = area.value;
  }
  for (const box of document.querySelectorAll("#agent-repos [data-repo-readonly]")) {
    state.repoReadonly[box.dataset.repoReadonly] = box.checked;
  }
}

function paintScopeCount(selector, countId) {
  const node = document.querySelector(countId);
  if (!node) return;
  const count = document.querySelectorAll(selector).length;
  node.textContent = count === 0 ? "None selected" : count === 1 ? "1 selected" : count + " selected";
  node.dataset.empty = count === 0 ? "true" : "false";
}

function scopeSwitch(name, value, label, on) {
  const button = el("button", "switch");
  button.type = "button";
  button.dataset[name] = value;
  button.setAttribute("aria-pressed", on ? "true" : "false");
  button.setAttribute("aria-label", label);
  button.append(el("span", "switch-knob"));
  return button;
}

function scopeCard(title, id, on, toggle, hint) {
  const card = el("article", "scope-card" + (on ? " is-on" : ""));
  const head = el("div", "scope-card-head");
  const copy = el("div", "scope-card-copy");
  copy.append(el("strong", "", title));
  if (id && id !== title) copy.append(el("span", "peer-id", id));
  if (hint) copy.append(el("span", "hint", hint));
  head.append(copy, toggle);
  card.append(head);
  return card;
}

function repoDetail(kind) {
  const wrap = el("div", "repo-block");
  const block = el("div", "field");
    const bar = el("div", "prompt-bar");
    bar.append(el("span", "", "What this agent does in this repository"));
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
    area.placeholder = "What this agent uses the " + kind + " repository for. The AI Tool receives this with that repository.";
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
    block.append(compose, preview);
  wrap.append(block, readonly);
  return wrap;
}

function renderRepoNotes() {
  rememberRepoNotes();
  const root = document.querySelector("#agent-repos");
  root.replaceChildren();
  document.querySelector("#agent-repo-notes").replaceChildren();
  const chosen = state.repoChosen || new Set();
  const kinds = repoChoices([...chosen]);
  if (!kinds.length) {
    root.append(el("p", "hint", "No repositories are set up."));
  }
  for (const kind of kinds) {
    const on = chosen.has(kind);
    const card = scopeCard(repoTitle(kind), kind, on, scopeSwitch("repo", kind, repoTitle(kind), on));
    if (on) {
      const body = el("div", "scope-card-body");
      body.append(repoDetail(kind));
      card.append(body);
    }
    root.append(card);
  }
  paintScopeCount("#agent-repos button[data-repo][aria-pressed='true']", "#agent-repo-count");
  resumeRepoRewrites();
  placeGenerateButtons();
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
  const fresh = el("option", "", "New category");
  fresh.value = "";
  select.append(fresh);
  const custom = document.querySelector("#agent-category-custom");
  custom.value = "";
  custom.hidden = !!select.value;
}

function snakeCase(value) {
  return String(value || "")
    .trim()
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/_+/g, "_");
}

function chosenCategory() {
  const select = document.querySelector("#agent-category");
  const value = select.value ? select.value : document.querySelector("#agent-category-custom").value;
  return snakeCase(value);
}

function newAgentId() {
  const category = chosenCategory();
  const name = snakeCase(document.querySelector("#agent-name").value);
  if (!category || !name) return "";
  return category + "_" + name;
}

function paintNewAgentId() {
  if (!state.creatingAgent) return;
  const id = newAgentId();
  const node = document.querySelector("#agent-id");
  node.hidden = !id;
  node.textContent = id;
}

function peerList(value) {
  if (!Array.isArray(value)) {
    if (!value) return [];
    return [{ id: value, handoffs: [] }];
  }
  return value.map((item) => {
    if (typeof item === "string") return { id: item, handoffs: [] };
    const pairs = Array.isArray(item && item.handoffs) ? item.handoffs : [];
    return {
      id: item && item.id,
      handoffs: pairs.map((pair) => {
        const when = (pair && pair.when) || "";
        const send = (pair && pair.send) || "";
        return {
          when,
          send,
          reply: (pair && pair.reply) || "",
          loop: !!(pair && pair.maxLoops > 0),
          maxLoops: (pair && pair.maxLoops > 0) ? pair.maxLoops : defaultMaxLoops,
          // Agents the app creates store the coordination note as what to send, with no condition.
          whenOptional: !when.trim() && !!send.trim(),
        };
      }),
    };
  }).filter((item) => item.id);
}

function rememberCollaborations() {
  if (!state.peerPairs) state.peerPairs = {};
  for (const block of document.querySelectorAll("#agent-works-with .collab-peer")) {
    const pairs = [];
    for (const row of block.querySelectorAll(".handoff")) {
      const when = row.querySelector("textarea[data-part='when']");
      const send = row.querySelector("textarea[data-part='send']");
      const reply = row.querySelector("textarea[data-part='reply']");
      const loop = row.querySelector("input[data-part='loop']");
      const loops = row.querySelector("input[data-part='loops']");
      pairs.push({
        when: when ? when.value : "",
        send: send ? send.value : "",
        reply: reply ? reply.value : "",
        loop: !!(loop && loop.checked),
        maxLoops: loops ? loops.value : String(defaultMaxLoops),
        whenOptional: row.dataset.whenOptional === "true",
      });
    }
    state.peerPairs[block.dataset.peer] = pairs;
  }
}

function previewSwitch(id) {
  const wrap = el("span", "preview-toggle");
  wrap.append(el("span", "", "Preview"));
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "switch";
  toggle.dataset.collabPreview = id;
  toggle.setAttribute("aria-pressed", "false");
  toggle.setAttribute("aria-label", "Preview");
  toggle.append(el("span", "switch-knob"));
  wrap.append(toggle);
  return wrap;
}

// loopLimit is the loop count to save. A count that is not a whole number from 1
// to the limit is sent as -1, so Save reports it.
function loopLimit(value) {
  const count = Math.trunc(Number(value));
  return count >= 1 && count <= maxLoopsLimit ? count : -1;
}

// syncHandoffRow shows the loop options only when the message has a report to wait
// for, and the loop limit only when the loop box is ticked.
function syncHandoffRow(row) {
  const reply = row.querySelector("textarea[data-part='reply']");
  const loop = row.querySelector("input[data-part='loop']");
  const loopLine = row.querySelector(".handoff-loop");
  const limit = row.querySelector(".handoff-limit");
  if (loopLine) loopLine.hidden = !(reply && reply.value.trim());
  if (limit) limit.hidden = !(loop && loop.checked);
}

// missingHandoff finds the first message with no condition or nothing to send.
function missingHandoff(worksWith) {
  for (const peer of worksWith) {
    for (let index = 0; index < peer.handoffs.length; index++) {
      const pair = peer.handoffs[index];
      if (!pair.when && !pair.whenOptional) {
        return { id: peer.id, index, part: "when", message: "Enter Send when for message " + (index + 1) + " to " + peer.id + ", or remove the message." };
      }
      if (!pair.send) {
        return { id: peer.id, index, part: "send", message: "Enter What to send for message " + (index + 1) + " to " + peer.id + ", or remove the message." };
      }
    }
  }
  return null;
}

function syncHandoffRows() {
  for (const row of document.querySelectorAll("#agent-works-with .handoff")) syncHandoffRow(row);
}

function handoffRow(id, index, pair) {
  const row = el("div", "handoff");
  if (pair.whenOptional) row.dataset.whenOptional = "true";
  const head = el("div", "prompt-bar");
  head.append(el("span", "collab-title", "Message " + (index + 1)));
  const remove = el("button", "text-button", "Remove");
  remove.type = "button";
  remove.dataset.handoffRemove = id;
  remove.dataset.handoffIndex = String(index);
  remove.setAttribute("aria-label", "Remove message " + (index + 1) + " to " + id);
  head.append(remove);
  row.append(head);

  const whenField = el("div", "field handoff-when");
  const whenBar = el("div", "prompt-bar");
  whenBar.append(el("span", "", pair.whenOptional ? "Send when" : "Send when (required)"));
  whenBar.append(previewSwitch(id));
  whenField.append(whenBar);
  const whenCompose = el("div", "compose");
  const when = document.createElement("textarea");
  when.dataset.peer = id;
  when.dataset.part = "when";
  when.rows = 2;
  when.placeholder = "The condition that tells this agent to send the message, for example: the requirement files are ready.";
  when.value = pair.when || "";
  whenCompose.append(when, generateButton("teamwork_when", id));
  const whenPreview = el("div", "markdown collab-preview");
  whenPreview.hidden = true;
  whenField.append(whenCompose, whenPreview);
  row.append(whenField);

  const sendField = el("div", "field");
  const bar = el("div", "prompt-bar");
  bar.append(el("span", "", "What to send to " + id + " (required)"));
  bar.append(previewSwitch(id));
  sendField.append(bar);
  const send = document.createElement("textarea");
  send.dataset.peer = id;
  send.dataset.part = "send";
  send.rows = 4;
  send.placeholder = "The message this agent sends to " + id + " when the condition is met. KIKI puts it in the launch prompt for the Agent Orchestration tool.";
  send.value = pair.send || "";
  const compose = el("div", "compose");
  compose.append(send, generateButton("teamwork", id));
  const preview = el("div", "markdown collab-preview");
  preview.hidden = true;
  sendField.append(compose, preview);
  row.append(sendField);
  const back = el("div", "handoff-back");
  const replyField = el("div", "field handoff-reply");
  const replyBar = el("div", "prompt-bar");
  replyBar.append(el("span", "", "What " + id + " reports back (empty means no report)"));
  replyBar.append(previewSwitch(id));
  replyField.append(replyBar);
  const replyCompose = el("div", "compose");
  const reply = document.createElement("textarea");
  reply.dataset.peer = id;
  reply.dataset.part = "reply";
  reply.rows = 3;
  reply.placeholder = "What " + id + " reports back to this agent when it has finished, for example: the path of the design file and any open question. Leave it empty when " + id + " does not report back.";
  reply.value = pair.reply || "";
  replyCompose.append(reply, generateButton("teamwork_reply", id));
  const replyPreview = el("div", "markdown collab-preview");
  replyPreview.hidden = true;
  replyField.append(replyCompose, replyPreview);
  back.append(replyField);

  const loopLine = el("div", "handoff-loop");
  const loopLabel = el("label", "repo-readonly");
  loopLabel.title = "Tick this when this agent and " + id + " may pass work back and forth.";
  const loopBox = document.createElement("input");
  loopBox.type = "checkbox";
  loopBox.dataset.part = "loop";
  loopBox.dataset.peer = id;
  loopBox.checked = !!pair.loop;
  loopLabel.append(loopBox, el("span", "", "This can loop back and forth between the two agents"));
  const limit = el("label", "handoff-limit");
  limit.append(el("span", "", "Most loops"));
  const loops = document.createElement("input");
  loops.type = "number";
  loops.min = "1";
  loops.max = String(maxLoopsLimit);
  loops.step = "1";
  loops.dataset.part = "loops";
  loops.dataset.peer = id;
  loops.value = pair.maxLoops || defaultMaxLoops;
  loops.setAttribute("aria-label", "Most loops between this agent and " + id);
  limit.append(loops);
  loopLine.append(loopLabel, limit);
  back.append(loopLine);
  row.append(back);
  syncHandoffRow(row);
  return row;
}

function renderCollaborations() {
  rememberCollaborations();
  paintCollaborations();
}

function peerDetail(id) {
  if (state.peerPairs[id] === undefined) state.peerPairs[id] = [{ when: "", send: "", reply: "", loop: false, maxLoops: defaultMaxLoops }];
  const pairs = state.peerPairs[id];
  const block = el("div", "collab-peer");
  block.dataset.peer = id;
  const bar = el("div", "prompt-bar");
  bar.append(el("span", "", "Messages"));
  const add = el("button", "text-button", "Add message");
  add.type = "button";
  add.dataset.handoffAdd = id;
  add.setAttribute("aria-label", "Add a message to " + id);
  bar.append(add);
  block.append(bar);
  if (!pairs.length) block.append(el("p", "hint", "No message is set. This agent sends nothing to " + id + "."));
  pairs.forEach((pair, index) => block.append(handoffRow(id, index, pair)));
  return block;
}

function peerCard(id, title, on, hint) {
  const card = scopeCard(title, id, on, scopeSwitch("peer", id, title, on), hint);
  if (on) {
    const body = el("div", "scope-card-body");
    body.append(peerDetail(id));
    card.append(body);
  }
  return card;
}

function paintCollaborations() {
  const root = document.querySelector("#agent-works-with");
  root.replaceChildren();
  document.querySelector("#agent-collaborations").replaceChildren();
  const chosen = state.peerChosen || new Set();
  const others = state.data.agents.filter((agent) => agent.id !== state.editingAgentId);
  if (!others.length && !chosen.size) {
    root.append(el("p", "hint", "No other agents to coordinate with."));
  }
  for (const group of groups(others)) {
    const block = el("div", "peer-group");
    block.append(el("span", "peer-label", categoryLabel(group.name)));
    for (const agent of group.agents) block.append(peerCard(agent.id, agent.title, chosen.has(agent.id)));
    root.append(block);
  }
  for (const id of chosen) {
    if (others.some((agent) => agent.id === id)) continue;
    root.append(peerCard(id, id, true, "This agent is not in the list."));
  }
  paintScopeCount("#agent-works-with button[data-peer][aria-pressed='true']", "#agent-peer-count");
  placeGenerateButtons();
}

function fillAgentPeers(currentId, selectedIds) {
  document.querySelector("#agent-works-with").replaceChildren();
  document.querySelector("#agent-collaborations").replaceChildren();
  const peers = peerList(selectedIds);
  state.peerPairs = {};
  state.peerChosen = new Set();
  for (const peer of peers) {
    state.peerPairs[peer.id] = peer.handoffs;
    state.peerChosen.add(peer.id);
  }
  state.editingAgentId = currentId || "";
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
  document.querySelector(spec.root).replaceChildren();
  if (spec.notes) document.querySelector(spec.notes).replaceChildren();
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
  state[kind === "mcp" ? "mcpChoiceRows" : "toolChoiceRows"] = choices;
  state[kind === "mcp" ? "mcpChosen" : "toolChosen"] = new Set([...chosen].filter((id) => choices.some((item) => item.id === id)));
}

function useDetail(kind, choice) {
  const spec = useSections[kind];
  const block = el("div", "field");
  const bar = el("div", "prompt-bar");
  bar.append(el("span", "", "How this agent uses it"));
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
  const fields = kind === "mcp" ? mcpPlaceFields(choice.id) : [];
  for (const field of fields) block.append(field);
  const area = document.createElement("textarea");
  area.dataset.use = choice.id;
  area.rows = 4;
  area.placeholder = spec.placeholder(choice.name);
  area.value = state[spec.state][choice.id] || "";
  const compose = el("div", "compose");
  compose.append(area, generateButton(spec.generate, choice.name));
  const preview = el("div", "markdown repo-preview");
  preview.hidden = true;
  block.append(compose, preview);
  if (fields.length) {
    const row = useSettingsRow(choice.id, fields);
    for (const field of fields) field.syncSettings = () => row.sync();
    block.append(row);
  }
  return block;
}

function rememberUseNotes(kind) {
  const spec = useSections[kind];
  if (!spec.notes) return;
  if (!state[spec.state]) state[spec.state] = {};
  for (const area of document.querySelectorAll(spec.root + " textarea[data-use]")) {
    state[spec.state][area.dataset.use] = area.value;
  }
}

function renderUseNotes(kind) {
  const spec = useSections[kind];
  if (spec.notes) rememberUseNotes(kind);
  const root = document.querySelector(spec.root);
  const notes = spec.notes ? document.querySelector(spec.notes) : null;
  root.replaceChildren();
  if (notes) notes.replaceChildren();
  const choices = (kind === "mcp" ? state.mcpChoiceRows : state.toolChoiceRows) || [];
  const chosen = (kind === "mcp" ? state.mcpChosen : state.toolChosen) || new Set();
  if (!choices.length) root.append(el("p", "hint", spec.empty));
  for (const choice of choices) {
    const on = chosen.has(choice.id);
    const meta = ((state.data && state.data.toolChoices) || []).find((item) => item.id === choice.id) || {};
    const hint = choice.missing ? "Not enabled in Settings." : (kind === "tool" ? meta.detail : "");
    const card = scopeCard(choice.name, choice.id, on, scopeSwitch("use", choice.id, choice.name, on), hint);
    if (choice.missing) card.classList.add("missing");
    if (on && spec.notes) {
      const body = el("div", "scope-card-body");
      body.append(useDetail(kind, choice));
      card.append(body);
    }
    root.append(card);
  }
  paintScopeCount(spec.root + " button[data-use][aria-pressed='true']", spec.count);
  placeGenerateButtons();
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
  const picked = [...document.querySelectorAll(spec.root + " button[data-use][aria-pressed='true']")].map((button) => {
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
  document.querySelector("#agent-repos").replaceChildren();
  document.querySelector("#agent-repo-notes").replaceChildren();
  const items = repoList(selected);
  state.repoNotes = {};
  state.repoReadonly = {};
  state.repoChosen = new Set();
  for (const repo of items) {
    state.repoNotes[repo.kind] = repo.description;
    state.repoReadonly[repo.kind] = !!repo.readonly;
    state.repoChosen.add(repo.kind);
  }
}

const promptFieldOrder = ["role", "input", "output", "startup", "principles"];

function emptyPromptModel() {
  return {
    title: "",
    order: promptFieldOrder.slice(),
    role: "",
    principles: "",
    startup: "",
    inputs: [],
    outputs: [],
    extras: [],
  };
}

function blankPromptItem(kind) {
  return kind === "input" ? { name: "", description: "", defaultValue: "", required: false } : { name: "", description: "" };
}

function promptSectionKey(title) {
  const name = title.trim().toLowerCase();
  if (name === "role") return "role";
  if (name === "principles") return "principles";
  if (name === "startup behavior" || name === "startup") return "startup";
  if (name === "input") return "input";
  if (name === "output") return "output";
  // The launch prompt writes the app root, the setup file, and repository indexes. It asks for
  // required inputs itself. These sections are not part of the prompt the user edits.
  if (name === "setup" || name === "boundaries" || name === "repository indexes" || name === "repository index" || name.startsWith("ask the user") || name === "workflow") return "drop";
  return "";
}

function inputLabel(phrase) {
  const text = phrase.trim();
  if (/status/i.test(text)) return "Status";
  if (/surface/i.test(text)) return "Surfaces";
  if (/rule/i.test(text)) return "Rules and limits";
  if (/scope/i.test(text)) return "Out of scope";
  if (/link|file/i.test(text)) return "Links and files";
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function outputLabel(text) {
  const rules = [
    [/requirement folder/i, "Requirement folder"],
    [/path of each file|file you wrote|HTML file/i, "Files"],
    [/PR-xxx[`\s]*items you wrote/i, "Items"],
    [/affected scope/i, "Affected scope"],
    [/repository index/i, "Index"],
    [/question still open|open questions/i, "Open question"],
    [/surface and the UI/i, "Surface"],
    [/items you designed/i, "Designed items"],
    [/items you skipped/i, "Skipped items"],
    [/sent the report/i, "Report sent"],
    [/parent ticket key/i, "Parent"],
    [/each sub-task/i, "Sub-tasks"],
    [/reviewed \(LGTM\)/i, "Review"],
    [/parent PR is approved/i, "Parent PR"],
    [/Final Jira status/i, "Jira status"],
    [/\bstatus\b/i, "Status"],
  ];
  for (const [pattern, name] of rules) {
    if (pattern.test(text)) return name;
  }
  return text.replace(/^The\s+/i, "").split(/\s+/).slice(0, 4).join(" ").replace(/[,.:]$/, "");
}

function alsoGivenInputs(lead, source) {
  const parts = lead.split(/\n\s*\n/);
  const kept = [];
  const items = [];
  for (const part of parts) {
    const match = /^The user may also give:\s*([\s\S]+)$/.exec(part.trim());
    if (!match) {
      kept.push(part);
      continue;
    }
    const dot = match[1].indexOf(".");
    const list = (dot >= 0 ? match[1].slice(0, dot) : match[1]).trim();
    const tail = dot >= 0 ? match[1].slice(dot + 1).trim() : "";
    const bits = list.split(/,\s*/).map((bit) => bit.replace(/^and\s+/i, "").trim()).filter(Boolean);
    bits.forEach((bit, index) => {
      const name = inputLabel(bit);
      let description = bit.charAt(0).toUpperCase() + bit.slice(1);
      if (!/[.!?]$/.test(description)) description += ".";
      if (index === bits.length - 1 && tail) description += " " + tail;
      const defaultValue = name === "Status" && /status:\s*`?draft`?/i.test(source) ? "draft" : "";
      items.push({ name, description, defaultValue, required: false });
    });
  }
  return { lead: kept.join("\n\n").trim(), items };
}

function parseInputBody(body, source) {
  const items = [];
  const lead = [];
  let item = null;
  const finish = () => {
    if (!item) return;
    item.description = item.description.trim();
    item.defaultValue = item.defaultValue.trim();
    if (item.name || item.description || item.defaultValue || item.required) items.push(item);
    item = null;
  };
  for (const raw of body.split("\n")) {
    const line = raw.trim();
    const named = /^- \*\*([^*]+)\*\*\s*$/.exec(line);
    const legacy = /^- \*\*(.+?):\*\*\s*(.*)$/.exec(line);
    const given = /^Default:\s*(.*)$/.exec(line);
    const required = /^Required:\s*/.test(line);
    if (named) {
      finish();
      item = { name: named[1].trim(), description: "", defaultValue: "", required: false };
      continue;
    }
    if (legacy) {
      finish();
      items.push({ name: legacy[1].trim(), description: legacy[2].trim(), defaultValue: "", required: false });
      continue;
    }
    if (item && given) {
      item.defaultValue = given[1].trim() === "(none)" ? "" : given[1].trim();
      continue;
    }
    if (item && required) {
      item.required = true;
      continue;
    }
    if (item) {
      item.description += (item.description ? "\n" : "") + line;
      continue;
    }
    lead.push(raw);
  }
  finish();
  const extra = alsoGivenInputs(lead.join("\n").trim(), source);
  return { lead: extra.lead, items: items.concat(extra.items) };
}

function parseOutputBody(body) {
  const items = [];
  const lead = [];
  let item = null;
  const finish = () => {
    if (!item) return;
    item.description = item.description.trim();
    if (item.name || item.description) items.push(item);
    item = null;
  };
  for (const raw of body.split("\n")) {
    const line = raw.trim();
    const named = /^- \*\*([^*]+)\*\*\.?\s*(.*)$/.exec(line);
    const plain = /^- (.+)$/.exec(line);
    if (named) {
      finish();
      item = { name: named[1].replace(/:$/, "").trim(), description: named[2].trim() };
      continue;
    }
    if (plain) {
      finish();
      items.push({ name: outputLabel(plain[1].trim()), description: plain[1].trim() });
      continue;
    }
    if (item && line) {
      item.description += "\n" + line;
      continue;
    }
    if (line) lead.push(line);
  }
  finish();
  return { lead: lead.join("\n").trim(), items };
}

function parseAgentPrompt(markdown) {
  const text = String(markdown || "").replace(/\r\n/g, "\n").trim();
  const model = emptyPromptModel();
  if (!text) {
    model.inputs = [blankPromptItem("input")];
    model.outputs = [blankPromptItem("output")];
    return model;
  }
  const lines = text.split("\n");
  const chunks = [];
  let current = null;
  for (const line of lines) {
    const heading = /^##\s+(.+)$/.exec(line);
    const title = /^#\s+Agent:\s*(.*)$/.exec(line);
    if (title && !current && !chunks.length) {
      model.title = title[1].trim();
      continue;
    }
    if (heading) {
      if (current) chunks.push(current);
      current = { title: heading[1].trim(), body: [] };
      continue;
    }
    if (current) current.body.push(line);
  }
  if (current) chunks.push(current);
  model.order = [];
  chunks.forEach((chunk) => {
    const key = promptSectionKey(chunk.title);
    const body = chunk.body.join("\n").trim();
    if (key === "input") {
      const parsed = parseInputBody(body, text);
      model.inputs = parsed.items;
      model.order.push("input");
      return;
    }
    if (key === "output") {
      const parsed = parseOutputBody(body);
      model.outputs = parsed.items;
      model.order.push("output");
      return;
    }
    if (key === "drop") return;
    if (key) {
      model[key] = body;
      model.order.push(key);
      return;
    }
    model.extras.push({ title: chunk.title, body });
    model.order.push("extra:" + (model.extras.length - 1));
  });
  for (const key of promptFieldOrder) {
    if (!model.order.includes(key)) model.order.push(key);
  }
  return model;
}

function sectionFilled(model, key) {
  if (key === "input") return !!model.inputs.some((item) => item.name.trim() || item.description.trim() || item.defaultValue.trim() || item.required);
  if (key === "output") return !!model.outputs.some((item) => item.name.trim() || item.description.trim());
  return !!(model[key] || "").trim();
}

function compileSequence(model) {
  const order = model.order.length ? model.order.slice() : promptFieldOrder.slice();
  promptFieldOrder.forEach((key, index) => {
    if (order.includes(key) || !sectionFilled(model, key)) return;
    let at = order.length;
    for (let previous = index - 1; previous >= 0; previous -= 1) {
      const found = order.indexOf(promptFieldOrder[previous]);
      if (found >= 0) {
        at = found + 1;
        break;
      }
    }
    order.splice(at, 0, key);
  });
  return order;
}

function compileInput(model) {
  const items = model.inputs.filter((item) => item.name.trim() || item.description.trim() || item.defaultValue.trim() || item.required);
  if (!items.length) return "";
  const lines = [];
  items.forEach((item) => {
    const name = item.name.trim() || "Input";
    lines.push("- **" + name + "**");
    if (item.description.trim()) lines.push("  " + item.description.trim().replace(/\n/g, "\n  "));
    if (item.required) lines.push("  Required:");
    else if (item.defaultValue.trim()) lines.push("  Default: " + item.defaultValue.trim());
    lines.push("");
  });
  return lines.join("\n").trim();
}

function compileOutput(model) {
  const items = model.outputs.filter((item) => item.name.trim() || item.description.trim());
  if (!items.length) return "";
  const lines = [];
  items.forEach((item) => {
    const name = item.name.trim() || "Output";
    const description = item.description.trim();
    lines.push(description ? "- **" + name + "**. " + description.replace(/\n/g, " ") : "- **" + name + "**");
  });
  return lines.join("\n").trim();
}

function compileAgentPrompt(model, fallbackTitle) {
  const title = model.title.trim() || (fallbackTitle || "").trim() || "Agent";
  const parts = ["# Agent: " + title];
  const headings = {
    role: "Role",
    principles: "Principles",
    startup: "Startup behavior",
  };
  compileSequence(model).forEach((key) => {
    if (key === "input") {
      const body = compileInput(model);
      if (body) parts.push("## Input\n\n" + body);
      return;
    }
    if (key === "output") {
      const body = compileOutput(model);
      if (body) parts.push("## Output\n\n" + body);
      return;
    }
    if (key.startsWith("extra:")) {
      const extra = model.extras[Number(key.slice(6))];
      if (!extra || (!extra.title.trim() && !extra.body.trim())) return;
      parts.push("## " + (extra.title.trim() || "Notes") + "\n\n" + extra.body.trim());
      return;
    }
    const body = (model[key] || "").trim();
    if (body && headings[key]) parts.push("## " + headings[key] + "\n\n" + body);
  });
  return parts.join("\n\n").replace(/\n{3,}/g, "\n\n") + "\n";
}

function promptPreviewToggle(scope) {
  const wrap = el("span", "preview-toggle");
  wrap.append(el("span", "", "Preview"));
  const toggle = el("button", "switch");
  toggle.type = "button";
  toggle.dataset.promptPreview = scope || "field";
  toggle.setAttribute("aria-pressed", "false");
  toggle.setAttribute("aria-label", "Preview");
  toggle.append(el("span", "switch-knob"));
  wrap.append(toggle);
  return wrap;
}

function promptControl(label, control) {
  const wrap = el("div", "field");
  const bar = el("div", "prompt-bar");
  bar.append(el("span", "", label));
  bar.append(promptPreviewToggle());
  wrap.append(bar);
  control.setAttribute("aria-label", label);
  const compose = el("div", "compose");
  compose.append(control, generateButton("prompt_field"));
  const preview = el("div", "markdown prompt-preview");
  preview.hidden = true;
  wrap.append(compose, preview);
  return wrap;
}

function promptTextarea(label, value, rows, part) {
  const area = el("textarea");
  area.rows = rows;
  area.value = value || "";
  area.dataset.promptPart = part;
  return promptControl(label, area);
}

function promptLine(label, part, value) {
  const input = el("input");
  input.type = "text";
  input.value = value || "";
  input.dataset.promptPart = part;
  return promptControl(label, input);
}

function promptRequiredToggle(item) {
  const label = el("label", "repo-readonly prompt-required");
  label.title = "If the user leaves this out of a task, the agent asks for it.";
  const box = document.createElement("input");
  box.type = "checkbox";
  box.dataset.promptRequired = "true";
  box.checked = !!item.required;
  label.append(box, el("span", "", "Required"));
  return label;
}

function promptItemControl(label, control) {
  const wrap = el("div", "field");
  const bar = el("div", "prompt-bar");
  bar.append(el("span", "", label));
  wrap.append(bar);
  control.setAttribute("aria-label", label);
  const compose = el("div", "compose");
  compose.append(control);
  wrap.append(compose);
  return wrap;
}

function promptItemLine(label, part, value) {
  const input = el("input");
  input.type = "text";
  input.value = value || "";
  input.dataset.promptPart = part;
  return promptItemControl(label, input);
}

function promptItemText(label, value, rows, part) {
  const area = el("textarea");
  area.rows = rows;
  area.value = value || "";
  area.dataset.promptPart = part;
  return promptItemControl(label, area);
}

function promptItemRow(kind, item) {
  const row = el("div", "prompt-item");
  row.dataset.promptItem = kind;
  const bar = el("div", "prompt-bar");
  if (kind === "input") bar.append(promptRequiredToggle(item));
  bar.append(promptPreviewToggle("item"));
  const remove = el("button", "text-button", "Remove");
  remove.type = "button";
  remove.dataset.promptRemove = kind;
  bar.append(remove);
  row.append(bar);
  const body = el("div", "prompt-item-fields");
  body.append(promptItemLine("Name", "name", item.name));
  body.append(promptItemText("Description", item.description, 3, "description"));
  if (kind === "input") {
    const fallback = promptItemLine("Default value", "default", item.defaultValue);
    fallback.dataset.promptDefault = "true";
    fallback.hidden = !!item.required;
    body.append(fallback);
  }
  body.append(generateButton(kind === "input" ? "prompt_input" : "prompt_output"));
  const preview = el("div", "markdown prompt-preview");
  preview.hidden = true;
  row.append(body, preview);
  return row;
}

function promptSection(title, hint, action) {
  const section = el("section", "prompt-section");
  if (action) {
    const head = el("div", "prompt-bar");
    head.append(el("h3", "", title), action);
    section.append(head);
  } else {
    section.append(el("h3", "", title));
  }
  if (hint) section.append(el("p", "hint", hint));
  return section;
}

function paintPromptFields(model) {
  const root = document.querySelector("#agent-prompt-fields");
  root.replaceChildren();
  root.dataset.promptTitle = model.title || "";
  const extras = model.order.filter((key) => key.startsWith("extra:"));
  root.dataset.promptOrder = promptFieldOrder.concat(extras).join("|");
  const role = promptSection("Role", "Who the agent is and what it produces.");
  role.append(promptTextarea("Role", model.role, 12, "role"));
  const principles = promptSection("Principles", "The rules it follows on every task.");
  principles.append(promptTextarea("Principles", model.principles, 18, "principles"));
  const startup = promptSection("Startup behavior", "What it does before the user has given a task.");
  startup.append(promptTextarea("Startup behavior", model.startup, 16, "startup"));
  const addInput = el("button", "text-button", "Add input");
  addInput.type = "button";
  addInput.dataset.promptAdd = "input";
  const input = promptSection("Input", "Each item is one thing the user can give. Required means the agent asks if they leave it out of a task. Otherwise the default is used.", addInput);
  const inputRows = el("div", "prompt-items");
  inputRows.dataset.promptList = "input";
  (model.inputs.length ? model.inputs : [blankPromptItem("input")]).forEach((item) => inputRows.append(promptItemRow("input", item)));
  input.append(inputRows);
  const addOutput = el("button", "text-button", "Add output");
  addOutput.type = "button";
  addOutput.dataset.promptAdd = "output";
  const output = promptSection("Output", "What it reports when the work is done.", addOutput);
  const outputRows = el("div", "prompt-items");
  outputRows.dataset.promptList = "output";
  (model.outputs.length ? model.outputs : [blankPromptItem("output")]).forEach((item) => outputRows.append(promptItemRow("output", item)));
  output.append(outputRows);
  root.append(role, input, output, startup, principles);
  model.extras.forEach((extra, index) => {
    const section = promptSection(extra.title || "Other", "Kept from the current prompt.");
    section.dataset.promptExtra = String(index);
    section.append(promptLine("Section", "title", extra.title));
    section.append(promptTextarea("Text", extra.body, 6, "body"));
    root.append(section);
  });
  syncAgentPromptMirror();
  placeGenerateButtons();
}

function readPromptItems(kind) {
  return [...document.querySelectorAll("#agent-prompt-fields [data-prompt-item='" + kind + "']")].map((row) => {
    const value = (part) => {
      const field = row.querySelector("[data-prompt-part='" + part + "']");
      return field ? field.value : "";
    };
    const item = { name: value("name").trim(), description: value("description").trim() };
    if (kind === "input") {
      item.defaultValue = value("default").trim();
      const required = row.querySelector("[data-prompt-required]");
      item.required = !!(required && required.checked);
    }
    return item;
  });
}

function readPromptFields() {
  const root = document.querySelector("#agent-prompt-fields");
  const value = (part) => {
    const field = root.querySelector("textarea[data-prompt-part='" + part + "'], input[data-prompt-part='" + part + "']");
    return field ? field.value : "";
  };
  const extras = [...root.querySelectorAll("[data-prompt-extra]")].map((section) => ({
    title: (section.querySelector("[data-prompt-part='title']") || {}).value || "",
    body: (section.querySelector("[data-prompt-part='body']") || {}).value || "",
  }));
  return {
    title: root.dataset.promptTitle || "",
    order: (root.dataset.promptOrder || "").split("|").filter(Boolean),
    role: value("role"),
    principles: value("principles"),
    startup: value("startup"),
    inputs: readPromptItems("input"),
    outputs: readPromptItems("output"),
    extras,
  };
}

function syncAgentPromptMirror() {
  const area = document.querySelector("#agent-prompt");
  const root = document.querySelector("#agent-prompt-fields");
  if (!area || !root || !root.childElementCount) return;
  const name = document.querySelector("#agent-name");
  area.value = compileAgentPrompt(readPromptFields(), name && name.value);
}

function agentPromptText() {
  syncAgentPromptMirror();
  return document.querySelector("#agent-prompt").value.trim();
}

function promptFieldLabel(target) {
  const field = target.closest && target.closest(".field");
  const span = field && field.querySelector(".prompt-bar > span");
  return span ? span.textContent.trim().toLowerCase() : "";
}

function promptItemEntries(item) {
  return [...item.querySelectorAll(".prompt-item-fields .field")].filter((field) => !field.hidden).map((field) => {
    const control = field.querySelector("textarea, input:not([type='checkbox'])");
    const span = field.querySelector(".prompt-bar > span");
    return { control, label: span ? span.textContent.trim() : "" };
  }).filter((entry) => entry.control && entry.label);
}

function promptItemSource(item) {
  return promptItemEntries(item).map((entry) => "## " + entry.label + "\n" + entry.control.value.trim()).join("\n\n");
}

function promptItemMarkdown(item) {
  return promptItemEntries(item).map((entry) => "**" + entry.label + "**\n\n" + entry.control.value.trim()).join("\n\n");
}

function parsePromptItemRewrite(text) {
  const fields = [];
  let current = null;
  for (const line of String(text || "").replace(/\r\n/g, "\n").split("\n")) {
    const heading = line.match(/^##\s+(.+?)\s*:?\s*$/);
    if (heading) {
      current = { label: heading[1].trim().toLowerCase(), lines: [] };
      fields.push(current);
      continue;
    }
    if (current) current.lines.push(line);
  }
  return fields.map((field) => ({ label: field.label, value: field.lines.join("\n").trim() }));
}

function promptItemRewriteValues(item, text) {
  const parsed = parsePromptItemRewrite(text);
  const entries = promptItemEntries(item);
  if (!entries.length) return null;
  const values = {};
  for (const entry of entries) {
    const found = parsed.find((field) => field.label === entry.label.toLowerCase());
    if (!found) return null;
    let value = found.value;
    if (entry.control.tagName === "INPUT") value = value.replace(/\s+/g, " ").trim();
    values[entry.label.toLowerCase()] = value;
  }
  return values;
}

function applyItemValues(item, fields, key) {
  for (const entry of promptItemEntries(item)) {
    const stored = fields[entry.label.toLowerCase()];
    if (!stored) continue;
    entry.control.value = stored[key];
  }
  const toggle = item.querySelector("[data-prompt-preview='item']");
  if (toggle && toggle.getAttribute("aria-pressed") === "true") refreshPromptItemPreview(item);
  placeGenerateButtons();
  syncAgentPromptMirror();
}

function refreshPromptItemPreview(item) {
  const preview = item.querySelector(":scope > .prompt-preview");
  if (preview) preview.innerHTML = renderMarkdown(promptItemMarkdown(item));
}

function showPromptItemPreview(item, on) {
  const body = item.querySelector(".prompt-item-fields");
  const preview = item.querySelector(":scope > .prompt-preview");
  if (!body || !preview) return;
  if (on) {
    const height = body.getBoundingClientRect().height;
    preview.style.height = height + "px";
    preview.style.maxHeight = height + "px";
    preview.innerHTML = renderMarkdown(promptItemMarkdown(item));
    preview.hidden = false;
    body.hidden = true;
  } else {
    preview.hidden = true;
    preview.style.height = "";
    preview.style.maxHeight = "";
    preview.replaceChildren();
    body.hidden = false;
    placeGenerateButtons();
  }
}

function showMarkdownPreview(area, preview, on) {
  const compose = area.closest(".compose") || area;
  if (on) {
    const height = compose.getBoundingClientRect().height;
    preview.style.height = height + "px";
    preview.style.maxHeight = height + "px";
    preview.innerHTML = renderMarkdown(area.value);
    preview.hidden = false;
    compose.hidden = true;
  } else {
    preview.hidden = true;
    preview.style.height = "";
    preview.style.maxHeight = "";
    preview.replaceChildren();
    compose.hidden = false;
    placeGenerateButtons();
  }
}

function blankAgent() {
  const tool = usableTool((state.data && state.data.generateTool) || "");
  return {
    id: "",
    name: "",
    description: "",
    category: "",
    enabled: true,
    tool,
    model: defaultModel(tool),
    repos: [],
    worksWith: [],
    mcps: [],
    tools: [],
    prompt: "",
  };
}

async function openAgentEditor(id) {
  const data = await api("/api/agents/" + encodeURIComponent(id));
  showAgentEditor(data, false);
}

function openNewAgent() {
  showAgentEditor(blankAgent(), true);
}

function showAgentEditorTab(id) {
  for (const tab of document.querySelectorAll("#agent-editor-tabs [data-agent-tab]")) {
    tab.setAttribute("aria-selected", tab.dataset.agentTab === id ? "true" : "false");
  }
  for (const panel of document.querySelectorAll("#agent-dialog [data-agent-panel]")) {
    panel.hidden = panel.dataset.agentPanel !== id;
  }
  if (id === "topology") renderAgentTopology();
}

function editorAgentDraft() {
  rememberRepoNotes();
  rememberCollaborations();
  rememberUseNotes("mcp");
  const name = document.querySelector("#agent-name").value.trim();
  const id = state.creatingAgent ? (newAgentId() || "new-agent") : (state.editingAgentId || "new-agent");
  const repos = [...(state.repoChosen || [])].map((kind) => ({
    kind,
    description: ((state.repoNotes || {})[kind] || "").trim(),
    readonly: !!(state.repoReadonly && state.repoReadonly[kind]),
  }));
  const teamwork = [...(state.peerChosen || [])].filter((peerId) => peerId && peerId !== id).map((peerId) => ({
    id: peerId,
    handoffs: ((state.peerPairs || {})[peerId] || []).map((pair) => ({
      when: pair.when || "",
      send: pair.send || "",
      reply: pair.reply || "",
      maxLoops: (pair.reply || "").trim() && pair.loop ? (Number(pair.maxLoops) || defaultMaxLoops) : 0,
    })),
  }));
  const mcps = [...(state.mcpChosen || [])].map((mcpId) => ({
    id: mcpId,
    description: ((state.mcpNotes || {})[mcpId] || "").trim(),
  }));
  const tools = [...(state.toolChosen || [])].map((toolId) => ({ id: toolId }));
  return {
    id,
    title: name || "New agent",
    name,
    group: chosenCategory(),
    enabled: document.querySelector("#agent-enabled").getAttribute("aria-pressed") === "true",
    summary: document.querySelector("#agent-description").value.trim(),
    tool: document.querySelector("#agent-tool").value,
    model: document.querySelector("#agent-model").value,
    teamwork,
    repositoryUses: repos,
    mcps,
    tools,
  };
}

function renderAgentTopology() {
  const root = document.querySelector("#agent-topology");
  const draft = editorAgentDraft();
  agentWorkflow.edges = [];
  agentWorkflow.draft = draft;
  agentWorkflow.selection = { kind: "agent", id: draft.id };
  root.replaceChildren();
  root.append(el("p", "hint", "This agent as the dialog is filled in now. Lines go through the guardrail to the repositories, agents, MCP servers, and tools that are turned on."));
  const legend = el("div", "workflow-legend");
  legend.append(
    workflowSwatch("guardrail", "Guardrail"),
    workflowSwatch("mcp", "MCP Servers"),
    workflowSwatch("tool", "Tool"),
    workflowSwatch("repository", "Repository"),
    workflowSwatch("teamwork", "Agents Coordination"),
    workflowSwatch("index", "Index"),
  );
  root.append(legend);
  const scroll = el("div", "workflow-scroll");
  const stage = el("div", "workflow-stage");
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("workflow-lines");
  svg.setAttribute("aria-hidden", "true");
  const cols = el("div", "workflow-cols");
  cols.append(
    agentTopologyAgentColumn(draft),
    agentTopologyGuardrailColumn(),
    agentTopologyUseColumn(draft, "mcp", "MCP Servers"),
    agentTopologyUseColumn(draft, "tool", "Tools"),
    agentTopologyRepoColumn(draft, agentWorkflow.edges),
    agentTopologyIndexColumn(draft),
  );
  stage.append(svg, cols);
  scroll.append(stage);
  root.append(scroll);
  for (const open of root.querySelectorAll(".workflow-node:not(.is-current) .workflow-open")) open.tabIndex = -1;
  const detail = el("div", "card workflow-detail");
  root.append(detail);
  pushAgentWorkflowEdges(draft, agentWorkflow.edges);
  agentWorkflowObserver.disconnect();
  agentWorkflowObserver.observe(stage);
  paintWorkflowDetail(agentWorkflow);
  requestAnimationFrame(() => layoutWorkflow(agentWorkflow));
}

function agentTopologyGuardrailColumn() {
  const col = workflowGuardrailColumn();
  const view = col.querySelector("[data-guardrail-view]");
  if (view) view.remove();
  const open = col.querySelector(".workflow-open");
  if (open) open.classList.add("plain");
  return col;
}

function agentTopologyAgentColumn(draft) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", "Agents"));
  const saved = new Map(((state.data && state.data.agents) || []).map((agent) => [agent.id, agent]));
  const peers = [];
  for (const peer of draft.teamwork) {
    if (peers.some((agent) => agent.id === peer.id)) continue;
    peers.push(saved.get(peer.id) || { id: peer.id, title: peer.id, group: "", enabled: true });
  }
  for (const group of groups([draft, ...peers])) {
    col.append(el("p", "workflow-group", categoryLabel(group.name)));
    for (const agent of group.agents) {
      col.append(workflowAgentNode(agent, { current: agent.id === draft.id, prompt: false }));
    }
  }
  return col;
}

function agentTopologyUseColumn(draft, kind, title) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", title));
  const items = kind === "mcp" ? configuredUses("mcp", draft.mcps) : (draft.tools || []);
  if (!items.length) {
    col.append(el("p", "hint", "None turned on."));
    return col;
  }
  for (const item of items) {
    const name = workflowUseName(kind, item.id);
    const card = workflowCard(kind, item.id);
    const open = card.querySelector(".workflow-open");
    open.classList.add("plain");
    open.append(el("span", "workflow-name", name));
    open.append(el("span", "id", item.id));
    col.append(card);
  }
  return col;
}

function agentTopologyRepoColumn(draft, edges) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", "Repositories"));
  const uses = agentRepositoryUses(draft);
  if (!uses.length) {
    col.append(el("p", "hint", "None turned on."));
    return col;
  }
  for (const use of uses) {
    const repo = workflowRepo(use.kind);
    const name = repoTitle(use.kind);
    const card = workflowCard("repo", use.kind);
    const open = card.querySelector(".workflow-open");
    open.classList.add("plain");
    open.append(el("span", "workflow-name", name));
    open.append(el("span", "workflow-meta", repo && repo.index ? indexLabel(repo.index) : "No index"));
    col.append(card);
    const index = repo && repo.index ? repo.index : "none";
    edges.push({
      kind: "index",
      fromType: "repo",
      fromId: use.kind,
      toType: "index",
      toId: index,
      fromKey: "repo:" + use.kind,
      toKey: "index:" + index,
      fromSide: "right",
      toSide: "left",
      kicker: "Index",
      title: name + " → " + (index === "none" ? "Not set" : indexLabel(index)),
      note: index === "none"
        ? "No index is saved for this repository."
        : "The launch prompt tells the model to query " + indexLabel(index) + " for this repository.",
    });
  }
  return col;
}

function agentTopologyIndexColumn(draft) {
  const col = el("div", "workflow-col");
  col.append(el("h2", "", "Indexes"));
  const ids = [];
  const seen = new Set();
  for (const use of agentRepositoryUses(draft)) {
    const repo = workflowRepo(use.kind);
    const id = repo && repo.index ? repo.index : "none";
    if (seen.has(id)) continue;
    seen.add(id);
    ids.push(id);
  }
  if (!ids.length) {
    col.append(el("p", "hint", "None turned on."));
    return col;
  }
  for (const id of ids) {
    const name = id === "none" ? "Not set" : indexLabel(id);
    const card = el("div", "workflow-node");
    card.dataset.flow = "index";
    card.dataset.id = id;
    const open = el("button", "workflow-open plain");
    open.type = "button";
    open.setAttribute("aria-pressed", "false");
    open.append(el("span", "workflow-name", name));
    card.append(open);
    const url = indexToolURL[id];
    if (url) card.append(workflowIndexLink(url, "index-tool-link workflow-index-link"));
    col.append(card);
  }
  return col;
}

function showAgentEditor(data, creating) {
  state.creatingAgent = creating;
  state.editingAgentId = creating ? "" : data.id;
  document.querySelector("#agent-dialog-title").textContent = creating ? "New agent" : "Agent: " + (data.name || data.id);
  document.querySelector("#agent-enabled").setAttribute("aria-pressed", data.enabled !== false ? "true" : "false");
  document.querySelector("#agent-name").value = data.name || "";
  const idText = document.querySelector("#agent-id");
  idText.hidden = creating || !data.id;
  idText.textContent = creating ? "" : (data.id || "");
  fillAgentCategories(creating ? "" : (data.category || ""));
  fillAgentPeers(creating ? "" : data.id, data.worksWith);
  renderCollaborations();
  document.querySelector("#agent-description").value = data.description || "";
  paintPromptFields(parseAgentPrompt(data.prompt || ""));
  fillAgentRepos(data.repos);
  renderRepoNotes();
  fillUses("mcp", data.mcps);
  renderUseNotes("mcp");
  fillUses("tool", data.tools);
  renderUseNotes("tool");
  const tool = document.querySelector("#agent-tool");
  const toolName = usableTool(data.tool);
  fillToolSelect(tool, toolName);
  fillModels(document.querySelector("#agent-model"), toolName, creating ? defaultModel(toolName) : data.model);
  const error = document.querySelector("#agent-error");
  error.hidden = true;
  error.textContent = "";
  const save = document.querySelector("#agent-save");
  save.disabled = false;
  save.textContent = creating ? "Add" : "Save";
  document.querySelector("#agent-delete").hidden = creating;
  showAgentEditorTab("prompt");
  document.querySelector("#agent-dialog").showModal();
  if (creating) paintNewAgentId();
  document.querySelector("#agent-name").focus();
  requestAnimationFrame(placeGenerateButtons);
}

function agentSaveLabel() {
  return state.creatingAgent ? "Add" : "Save";
}

async function saveAgent() {
  const button = document.querySelector("#agent-save");
  const error = document.querySelector("#agent-error");
  error.hidden = true;
  button.disabled = true;
  button.textContent = "Saving…";
  rememberRepoNotes();
  const repos = [...document.querySelectorAll("#agent-repos button[data-repo][aria-pressed='true']")].map((button) => ({
    kind: button.dataset.repo,
    description: (state.repoNotes[button.dataset.repo] || "").trim(),
    readonly: !!(state.repoReadonly && state.repoReadonly[button.dataset.repo]),
  }));
  rememberCollaborations();
  const worksWith = [...document.querySelectorAll("#agent-works-with button[data-peer][aria-pressed='true']")].map((button) => ({
    id: button.dataset.peer,
    handoffs: ((state.peerPairs || {})[button.dataset.peer] || [])
      .map((pair) => ({
        when: (pair.when || "").trim(),
        send: (pair.send || "").trim(),
        reply: (pair.reply || "").trim(),
        maxLoops: (pair.reply || "").trim() && pair.loop ? loopLimit(pair.maxLoops) : 0,
        whenOptional: !!pair.whenOptional,
      })),
  }));
  const missing = missingHandoff(worksWith);
  if (missing) {
    error.hidden = false;
    error.textContent = missing.message;
    button.disabled = false;
    button.textContent = agentSaveLabel();
    const rows = document.querySelectorAll("#agent-works-with .collab-peer[data-peer='" + missing.id + "'] .handoff");
    const field = rows[missing.index] && rows[missing.index].querySelector("textarea[data-part='" + missing.part + "']");
    if (field) {
      const preview = field.closest(".field") && field.closest(".field").querySelector("[data-collab-preview]");
      if (preview && preview.getAttribute("aria-pressed") === "true") preview.click();
      field.focus();
    }
    return;
  }
  const mcps = selectedUses("mcp");
  const tools = selectedUses("tool");
  const name = document.querySelector("#agent-name").value.trim();
  const creating = state.creatingAgent;
  const id = creating ? newAgentId() : state.editingAgentId;
  if (creating && !id) {
    error.hidden = false;
    error.textContent = "Enter a category and a name.";
    button.disabled = false;
    button.textContent = agentSaveLabel();
    return;
  }
  try {
    const data = await api(creating ? "/api/agents" : "/api/agents/" + encodeURIComponent(id), {
      method: creating ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        id,
        name,
        description: document.querySelector("#agent-description").value.trim(),
        category: chosenCategory(),
        enabled: document.querySelector("#agent-enabled").getAttribute("aria-pressed") === "true",
        tool: document.querySelector("#agent-tool").value,
        model: document.querySelector("#agent-model").value,
        repos,
        worksWith,
        mcps,
        tools,
        prompt: agentPromptText(),
      }),
    });
    const selected = document.querySelector("[data-tab][aria-selected='true']");
    const tab = (selected && selected.dataset.tab) || "tasks";
    render(data);
    showTab(tab);
    document.querySelector("#agent-dialog").close();
    banner((creating ? "Added " : "Saved ") + name + ".");
  } catch (err) {
    error.hidden = false;
    error.textContent = err.message;
    button.disabled = false;
    button.textContent = agentSaveLabel();
  }
}

function confirmAgentDelete(agent) {
  const dialog = document.querySelector("#agent-delete-dialog");
  const title = (agent && (agent.title || agent.name)) || (agent && agent.id) || "this agent";
  document.querySelector("#agent-delete-title").textContent = "Delete " + title;
  document.querySelector("#agent-delete-text").textContent =
    "Remove " + title + " from this app? Other agents stop working with it. Tasks that already name it keep that record.";
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
    document.querySelector("#agent-delete-confirm").onclick = () => {
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish(true);
    };
    dialog.showModal();
  });
}

async function deleteAgent(id) {
  const agent = agentById(id) || { id, title: id };
  const confirmed = await confirmAgentDelete(agent);
  if (!confirmed) return;
  const data = await api("/api/agents/" + encodeURIComponent(id), { method: "DELETE" });
  const editor = document.querySelector("#agent-dialog");
  if (editor.open && state.editingAgentId === id) editor.close();
  const selected = document.querySelector("[data-tab][aria-selected='true']");
  const tab = (selected && selected.dataset.tab) || "tasks";
  render(data);
  showTab(tab);
  banner("Deleted " + (agent.title || agent.name || id) + ".");
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
  if (editAgent && !event.target.closest("button, select, input, textarea, a, label")) {
    openAgentEditor(editAgent.dataset.editAgent).catch((error) => banner(error.message, true));
    return;
  }
  if (event.target.closest("#agent-new")) {
    openNewAgent();
    return;
  }
  const deleteAgentButton = event.target.closest("[data-delete-agent]");
  if (deleteAgentButton) {
    deleteAgent(deleteAgentButton.dataset.deleteAgent).catch((error) => banner(error.message, true));
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
  const clone = event.target.closest("[data-clone]");
  if (clone) {
    openClone(clone.dataset.clone);
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
  const agentTab = event.target.closest("#agent-editor-tabs [data-agent-tab]");
  if (agentTab) {
    event.preventDefault();
    showAgentEditorTab(agentTab.dataset.agentTab);
    return;
  }
  const taskTab = event.target.closest("[data-task-tab]");
  if (taskTab) {
    event.preventDefault();
    const id = taskTab.dataset.taskTab;
    for (const tab of document.querySelectorAll("#task-dialog [data-task-tab]")) {
      tab.setAttribute("aria-selected", tab.dataset.taskTab === id ? "true" : "false");
    }
    for (const panel of document.querySelectorAll("#task-dialog [data-task-panel]")) {
      panel.hidden = panel.dataset.taskPanel !== id;
    }
    return;
  }
  const taskApprove = event.target.closest("[data-task-approve]");
  if (taskApprove) {
    event.preventDefault();
    approveTaskAgent(taskApprove.dataset.taskApprove, taskApprove);
    return;
  }
  const taskSend = event.target.closest("[data-task-send]");
  if (taskSend) {
    event.preventDefault();
    const area = document.querySelector(`#task-dialog [data-task-comment="${CSS.escape(taskSend.dataset.taskSend)}"]`);
    sendTaskAgent(taskSend.dataset.taskSend, area ? area.value : "", taskSend);
    return;
  }
  const taskAnswer = event.target.closest("[data-task-ask-submit]");
  if (taskAnswer) {
    event.preventDefault();
    const id = taskAnswer.dataset.taskAskSubmit;
    sendTaskAgent(id, taskAnswerText(id), taskAnswer, true);
    return;
  }
  const taskPrompt = event.target.closest("[data-task-prompt]");
  if (taskPrompt) {
    openTaskPrompt(taskPrompt.dataset.taskPrompt).catch((error) => taskError(error.message));
    return;
  }
  const taskAgent = event.target.closest("[data-task-agent]");
  if (taskAgent) {
    if (taskAgent.disabled) return;
    const id = taskAgent.dataset.taskAgent;
    const manual = taskManualSet();
    if (manual.has(id)) {
      manual.delete(id);
    } else {
      manual.add(id);
      for (const peer of taskReach(id)) manual.delete(peer);
    }
    syncTaskForced();
    return;
  }
  const agentSwitch = event.target.closest(".agent-switch");
  if (agentSwitch) {
    const button = agentSwitch.querySelector("#agent-enabled");
    button.setAttribute("aria-pressed", button.getAttribute("aria-pressed") === "true" ? "false" : "true");
    return;
  }
  const scopeToggle = event.target.closest("#agent-repos .switch[data-repo], #agent-works-with .switch[data-peer], #agent-mcps .switch[data-use], #agent-tools .switch[data-use]");
  if (scopeToggle) {
    const turnOn = scopeToggle.getAttribute("aria-pressed") !== "true";
    if (scopeToggle.dataset.repo) {
      if (!state.repoChosen) state.repoChosen = new Set();
      if (turnOn) state.repoChosen.add(scopeToggle.dataset.repo);
      else state.repoChosen.delete(scopeToggle.dataset.repo);
      renderRepoNotes();
    } else if (scopeToggle.dataset.peer) {
      if (!state.peerChosen) state.peerChosen = new Set();
      const id = scopeToggle.dataset.peer;
      if (turnOn) {
        state.peerChosen.add(id);
        if (state.peerPairs[id] === undefined) state.peerPairs[id] = [{ when: "", send: "", reply: "", loop: false, maxLoops: defaultMaxLoops }];
      } else state.peerChosen.delete(id);
      renderCollaborations();
    } else {
      const mcp = !!scopeToggle.closest("#agent-mcps");
      const setName = mcp ? "mcpChosen" : "toolChosen";
      if (!state[setName]) state[setName] = new Set();
      if (turnOn) state[setName].add(scopeToggle.dataset.use);
      else state[setName].delete(scopeToggle.dataset.use);
      renderUseNotes(mcp ? "mcp" : "tool");
    }
    return;
  }
  const generate = event.target.closest("[data-generate]");
  if (generate) {
    prepareGenerate(generate);
    return;
  }
  const promptToggle = event.target.closest("[data-prompt-preview]");
  if (promptToggle) {
    const on = promptToggle.getAttribute("aria-pressed") !== "true";
    if (promptToggle.dataset.promptPreview === "item") {
      const item = promptToggle.closest(".prompt-item");
      if (!item) return;
      showPromptItemPreview(item, on);
    } else {
      const block = promptToggle.closest(".field");
      const area = block && block.querySelector("textarea, input");
      const preview = block && block.querySelector(".prompt-preview");
      if (!area || !preview) return;
      showMarkdownPreview(area, preview, on);
    }
    promptToggle.setAttribute("aria-pressed", on ? "true" : "false");
    return;
  }
  const promptAdd = event.target.closest("[data-prompt-add]");
  if (promptAdd) {
    const model = readPromptFields();
    const kind = promptAdd.dataset.promptAdd;
    const list = kind === "input" ? model.inputs : model.outputs;
    list.push(blankPromptItem(kind));
    paintPromptFields(model);
    const rows = document.querySelectorAll("#agent-prompt-fields [data-prompt-item='" + kind + "']");
    const field = rows[rows.length - 1] && rows[rows.length - 1].querySelector("[data-prompt-part='name']");
    if (field) field.focus();
    return;
  }
  const promptRemove = event.target.closest("[data-prompt-remove]");
  if (promptRemove) {
    const row = promptRemove.closest("[data-prompt-item]");
    const kind = promptRemove.dataset.promptRemove;
    const rows = [...document.querySelectorAll("#agent-prompt-fields [data-prompt-item='" + kind + "']")];
    const index = rows.indexOf(row);
    const model = readPromptFields();
    const list = kind === "input" ? model.inputs : model.outputs;
    if (index >= 0) list.splice(index, 1);
    if (!list.length) list.push(blankPromptItem(kind));
    paintPromptFields(model);
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
  const handoffBox = event.target.closest("input[data-part='loop']");
  if (handoffBox) {
    const row = handoffBox.closest(".handoff");
    if (row) syncHandoffRow(row);
    return;
  }
  const handoffAdd = event.target.closest("[data-handoff-add]");
  if (handoffAdd) {
    rememberCollaborations();
    const id = handoffAdd.dataset.handoffAdd;
    state.peerPairs[id] = (state.peerPairs[id] || []).concat([{ when: "", send: "", reply: "", loop: false, maxLoops: defaultMaxLoops }]);
    paintCollaborations();
    const rows = document.querySelectorAll("#agent-works-with .collab-peer[data-peer='" + id + "'] .handoff");
    const last = rows[rows.length - 1];
    const field = last && last.querySelector("textarea[data-part='when']");
    if (field) field.focus();
    return;
  }
  const handoffRemove = event.target.closest("[data-handoff-remove]");
  if (handoffRemove) {
    rememberCollaborations();
    const id = handoffRemove.dataset.handoffRemove;
    const index = Number(handoffRemove.dataset.handoffIndex);
    state.peerPairs[id] = (state.peerPairs[id] || []).filter((_, i) => i !== index);
    paintCollaborations();
    return;
  }
  const collabToggle = event.target.closest("[data-collab-preview]");
  if (collabToggle) {
    const block = collabToggle.closest(".field");
    const area = block.querySelector("textarea");
    const preview = block.querySelector(".collab-preview");
    const on = collabToggle.getAttribute("aria-pressed") !== "true";
    showMarkdownPreview(area, preview, on);
    collabToggle.setAttribute("aria-pressed", on ? "true" : "false");
    return;
  }
  const tag = event.target.closest(".service-tags button");
  if (tag) {
    tag.setAttribute("aria-pressed", tag.getAttribute("aria-pressed") === "true" ? "false" : "true");
    const tags = tag.closest("[data-services]");
    if (tags) refreshServiceGuides(tags.dataset.services);
    syncSave();
    return;
  }
  const addGuide = event.target.closest("[data-add-guide]");
  if (addGuide) {
    const service = addGuide.dataset.guideService || "";
    const list = service
      ? addGuide.parentElement.querySelector("[data-service-guide-list]")
      : addGuide.parentElement.querySelector("[data-repo-guide-list]");
    if (list) list.append(guideRow(addGuide.dataset.addGuide, service, ""));
    syncSave();
    return;
  }
  const removeGuide = event.target.closest("[data-remove-guide]");
  if (removeGuide) {
    const row = removeGuide.closest(".guide-row");
    if (row) row.remove();
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
  const setupIndex = event.target.closest("[data-setup-index]");
  if (setupIndex) {
    setupRepoIndex(setupIndex.dataset.setupIndex, setupIndex.dataset.setupTool, { force: true });
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
  if (event.target.closest("[data-add-repo]")) {
    openRepoName("");
    return;
  }
  const renameRepo = event.target.closest("[data-rename-repo]");
  if (renameRepo) {
    openRepoName(renameRepo.dataset.renameRepo);
    return;
  }
  const deleteRepo = event.target.closest("[data-delete-repo]");
  if (deleteRepo) {
    openDeleteRepo(deleteRepo.dataset.deleteRepo);
    return;
  }
  const clear = event.target.closest("[data-clear-index]");
  if (clear) {
    openRemoveIndex(clear.dataset.clearIndex);
  }
});

document.body.addEventListener("input", (event) => {
  if (event.target.id === "task-search") {
    state.taskQuery = event.target.value;
    const start = event.target.selectionStart;
    const end = event.target.selectionEnd;
    renderTasks();
    const next = document.querySelector("#task-search");
    if (next) {
      next.focus();
      if (start != null && end != null) next.setSelectionRange(start, end);
    }
    return;
  }
  if (event.target.closest("#settings-fields")) clearFieldValidation(event.target);
  if (event.target.id === "team-path" || event.target.id === "team-user") syncTeamName();
  if (event.target.dataset && event.target.dataset.guideName) syncSave();
  if (event.target.id === "workspace-id" || event.target.id === "team-path" || event.target.id === "team-user") syncSave();
  if (event.target.id && event.target.id.startsWith("mcp-")) syncSave();
  if (event.target.id === "team-path") {
    if (!event.target.value.trim()) cancelTeamStage();
    refreshTeamNameChoices(event.target);
  }
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
  if (event.target.id === "repo-name-divided") {
    document.querySelector("#repo-name-divided-note").hidden = !event.target.checked;
    return;
  }
  if (event.target.matches("[data-repo-divided]")) {
    toggleRepoDivided(event.target);
    return;
  }
  if (event.target.matches("[data-service-guide-shared]")) {
    const block = event.target.closest("[data-service-guide]");
    const own = block && block.querySelector(".service-guide-own");
    if (own) own.hidden = event.target.checked;
    syncSave();
    return;
  }
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
  if (event.target.id === "task-done-from" || event.target.id === "task-done-to") {
    state.taskDoneFrom = document.querySelector("#task-done-from").value;
    state.taskDoneTo = document.querySelector("#task-done-to").value;
    renderTasks();
    return;
  }
  const taskTool = event.target.closest("[data-task-tool]");
  if (taskTool) {
    const next = document.querySelector(`[data-task-model="${CSS.escape(taskTool.dataset.taskTool)}"]`);
    next.dataset.previousModel = next.value;
    refreshModels(next, taskTool.value);
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
  document.querySelector("#clone-title").textContent = "Clone " + repoTitle(kind);
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

document.querySelector("#settings-fields").addEventListener("click", (event) => {
  if (event.target.closest("#team-browse")) browseTeamFolder();
});
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
document.querySelector("#agent-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  saveAgent();
});
document.querySelector("#agent-prompt-fields").addEventListener("input", () => {
  syncAgentPromptMirror();
});
document.querySelector("#agent-prompt-fields").addEventListener("change", (event) => {
  const box = event.target.closest("[data-prompt-required]");
  if (!box) return;
  const row = box.closest("[data-prompt-item]");
  const field = row && row.querySelector("[data-prompt-default]");
  if (field) field.hidden = box.checked;
  const toggle = row && row.querySelector("[data-prompt-preview='item']");
  if (toggle && toggle.getAttribute("aria-pressed") === "true") refreshPromptItemPreview(row);
  syncAgentPromptMirror();
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

document.querySelector("#repo-name-input").addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  saveRepoName();
});

document.querySelector("#repo-name-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  saveRepoName();
});

document.querySelector("#repo-delete-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  confirmDeleteRepo();
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
document.querySelector("#agent-dialog").addEventListener("input", (event) => {
  if (state.creatingAgent && event.target.matches("#agent-name, #agent-category-custom")) paintNewAgentId();
  if (event.target.closest(".compose textarea, .compose input")) placeGenerateButtons();
  if (event.target.matches("textarea[data-part='reply']")) {
    const row = event.target.closest(".handoff");
    if (row) syncHandoffRow(row);
  }
  resetGenerateUndo(event.target);
});
document.querySelector("#agent-category").addEventListener("change", () => {
  const custom = document.querySelector("#agent-category-custom");
  custom.hidden = !!document.querySelector("#agent-category").value;
  if (!custom.hidden) custom.focus();
  paintNewAgentId();
});
document.querySelector("#agent-delete").addEventListener("click", () => {
  if (!state.editingAgentId || state.creatingAgent) return;
  deleteAgent(state.editingAgentId).catch((error) => {
    const note = document.querySelector("#agent-error");
    note.hidden = false;
    note.textContent = error.message;
  });
});
document.querySelector("#agent-dialog").addEventListener("close", () => {
  stopGenerateRuns();
  state.creatingAgent = false;
});
function syncTaskLaunchPreview(target) {
  if (!target) return;
  if (target.id === "task-group-search") paintTaskGroups();
}

function taskGroupMenuOpen() {
  const menu = document.querySelector("#task-group-menu");
  return !!(menu && !menu.hidden);
}

function setTaskGroupMenu(open) {
  const menu = document.querySelector("#task-group-menu");
  const toggle = document.querySelector("#task-group-toggle");
  if (!menu || !toggle) return;
  const task = state.taskId ? taskById(state.taskId) : null;
  if (task && task.started) open = false;
  menu.hidden = !open;
  toggle.setAttribute("aria-expanded", open ? "true" : "false");
  if (!open) return;
  const search = document.querySelector("#task-group-search");
  if (!search) return;
  search.value = "";
  paintTaskGroups();
  search.focus();
}

function chooseTaskGroup(id) {
  const hidden = document.querySelector("#task-group");
  if (hidden) hidden.value = id || "";
  const search = document.querySelector("#task-group-search");
  if (search) search.value = "";
  setTaskGroupMenu(false);
  paintTaskGroups();
  syncTaskGroup();
}

document.querySelector("#task-group-toggle").addEventListener("click", () => {
  setTaskGroupMenu(!taskGroupMenuOpen());
});

document.querySelector("#task-group-search").addEventListener("keydown", (event) => {
  if (event.key === "Escape") return;
  if (event.key !== "Enter") return;
  event.preventDefault();
  const choices = [...document.querySelectorAll("#task-group-list [data-task-group]:not(:disabled)")];
  const picked = choices.find((choice) => choice.getAttribute("aria-selected") === "true") || (choices.length === 1 ? choices[0] : null);
  if (!picked) return;
  chooseTaskGroup(picked.dataset.taskGroup || "");
});

document.querySelector("#task-group-list").addEventListener("click", (event) => {
  const choice = event.target.closest("[data-task-group]");
  if (!choice || choice.disabled) return;
  chooseTaskGroup(choice.dataset.taskGroup || "");
});

document.querySelector("#task-dialog").addEventListener("click", (event) => {
  if (!taskGroupMenuOpen()) return;
  if (event.target.closest("#task-group-field")) return;
  setTaskGroupMenu(false);
});

document.querySelector("#task-dialog").addEventListener("cancel", (event) => {
  if (!taskGroupMenuOpen()) return;
  event.preventDefault();
  setTaskGroupMenu(false);
  document.querySelector("#task-group-toggle").focus();
});
document.querySelector("#task-dialog").addEventListener("input", (event) => {
  if (event.target.closest(".compose textarea")) placeGenerateButtons();
  resetGenerateUndo(event.target);
  syncTaskLaunchPreview(event.target);
});
document.querySelector("#task-dialog").addEventListener("change", (event) => {
  syncTaskLaunchPreview(event.target);
});
document.querySelector("#task-dialog").addEventListener("close", stopGenerateRuns);
document.querySelector("#launch-repo").addEventListener("change", () => {
  showWorktree();
  if (!document.querySelector("#launch-prompt").hidden) showPrompt();
});

document.querySelector("#team-share-dialog").addEventListener("click", (event) => {
  const diff = event.target.closest("[data-agent-diff]");
  if (diff) {
    openAgentDiff(diff.dataset.agentDiff, diff.dataset.folder, diff.dataset.part).catch((error) => banner(error.message, true));
    return;
  }
  const arrow = event.target.closest(".team-arrows button");
  if (!arrow || arrow.disabled) return;
  const side = arrow.dataset.sync === "push" ? "app" : "folder";
  syncFolder(arrow.dataset.folder, arrow.dataset.sync, arrow.dataset.part, sourceDiffs(arrow.closest(".team-compare"), side)).catch((error) => banner(error.message, true));
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
  }
});

const taskColumns = [
  { id: "todo", label: "TODO" },
  { id: "progress", label: "In Progress" },
  { id: "done", label: "Done" },
];
const taskStatuses = [
  { id: "pending", label: "Pending", badge: "" },
  { id: "doing", label: "Doing", badge: "warn" },
  { id: "review", label: "Waiting for review", badge: "wait" },
  { id: "completed", label: "Completed", badge: "ok" },
];
let taskSuppressClick = false;
let taskDragging = false;
let taskSyncTimer = 0;
let taskSyncGeneration = 0;

function orcaMark(run) {
  if (!run) return null;
  if (run.status === "idle" || run.status === "exited") return { label: "Finished", badge: "ok" };
  if (run.status === "running") return { label: "Running", badge: "warn" };
  return { label: "Checking", badge: "" };
}

function taskLogText(run) {
  const text = String((run && run.log) || "").trim();
  return text || "Waiting for output from Orca.";
}

function taskLogNode(run, agentId) {
  const log = el("pre", "task-log");
  if (agentId) log.dataset.taskLog = agentId;
  log.textContent = taskLogText(run);
  return log;
}

function runsNeedWatch() {
  for (const task of (state.data && state.data.tasks) || []) {
    for (const run of task.runs || []) {
      if (run.status !== "exited") return true;
    }
  }
  return false;
}

function watchTaskRuns() {
  if (taskSyncTimer || !runsNeedWatch()) return;
  const generation = ++taskSyncGeneration;
  const tick = async () => {
    if (generation !== taskSyncGeneration) return;
    if (!runsNeedWatch()) {
      clearInterval(taskSyncTimer);
      taskSyncTimer = 0;
      return;
    }
    try {
      const data = await api("/api/tasks/sync", { method: "POST" });
      if (generation !== taskSyncGeneration) return;
      applySyncedTasks(data.tasks || []);
    } catch (error) {}
  };
  tick();
  taskSyncTimer = setInterval(tick, 4000);
}

function applySyncedTasks(tasks) {
  if (!state.data || !tasks.length) return;
  const byID = new Map(tasks.map((task) => [task.id, task]));
  state.data.tasks = (state.data.tasks || []).map((task) => byID.get(task.id) || task);
  const dialog = document.querySelector("#task-dialog");
  if (dialog && dialog.open && byID.has(state.taskId)) {
    const open = byID.get(state.taskId);
    paintTaskColumn(open.column);
    paintTaskLogs(open);
  }
  const search = document.querySelector("#task-search");
  if ((search && document.activeElement === search) || taskDragging) return;
  renderTasks();
}

function followKey(agent, run) {
  const options = (run && run.askOptions) || [];
  const questions = (run && run.askQuestions) || [];
  return agent.status + "\n" + ((run && run.askPrompt) || "") + "\n" + options.join("\n") + "\n" + JSON.stringify(questions);
}

function paintTaskColumn(column) {
  const node = document.querySelector("#task-column");
  if (!node) return;
  const item = taskColumns.find((entry) => entry.id === column) || taskColumns[0];
  node.textContent = item.label;
  const tone = column === "done" ? "ok" : column === "progress" ? "warn" : "";
  node.className = "badge" + (tone ? " " + tone : "");
}

function paintTaskLogs(task) {
  const runs = new Map((task.runs || []).map((run) => [run.agentId, run]));
  for (const agent of task.agents || []) {
    const run = runs.get(agent.agentId) || null;
    const selector = CSS.escape(agent.agentId);
    let host = document.querySelector(`#task-dialog [data-task-panel="${selector}"]`);
    if (!host) {
      const row = document.querySelector(`#task-dialog [data-task-agent="${selector}"]`);
      host = row && row.closest(".feature-agent");
    }
    if (!host) continue;
    let node = host.querySelector(`[data-task-log="${selector}"]`);
    if (run) {
      if (!node) host.append(taskLogNode(run, agent.agentId));
      else node.textContent = taskLogText(run);
    }
    const tabBadge = document.querySelector(`#task-dialog [data-task-tab-state="${selector}"]`);
    const badge = host.querySelector(`[data-task-state="${selector}"]`);
    if (badge) {
      const status = taskStatus(agent.status);
      badge.className = "badge" + (status.badge ? " " + status.badge : "");
      badge.textContent = status.label;
      if (tabBadge) {
        tabBadge.className = badge.className;
        tabBadge.textContent = status.label;
      }
    }
    const key = followKey(agent, run);
    const follow = host.querySelector(`[data-task-follow="${selector}"]`);
    if (agent.status !== "review") {
      if (follow) follow.remove();
      continue;
    }
    if (follow && follow.dataset.key === key) continue;
    const next = taskFollowup(agent.agentId, run);
    next.dataset.key = key;
    if (follow) follow.replaceWith(next);
    else host.append(next);
  }
}

function taskAnswerText(agentId) {
  const host = document.querySelector(`#task-dialog [data-task-follow="${CSS.escape(agentId)}"]`);
  if (!host) return "";
  const questions = host.querySelectorAll("[data-task-question]");
  if (questions.length) {
    const lines = [];
    for (const item of questions) {
      const chosen = item.querySelector('input[type="radio"]:checked');
      const area = item.querySelector("textarea");
      const value = chosen ? chosen.value.trim() : (area ? area.value.trim() : "");
      if (!value) return "";
      lines.push(item.dataset.taskQuestion + ": " + value);
    }
    return lines.join("\n");
  }
  const radios = host.querySelectorAll('input[type="radio"]');
  if (radios.length) {
    const chosen = host.querySelector('input[type="radio"]:checked');
    return chosen ? chosen.value : "";
  }
  const area = host.querySelector("[data-task-answer]");
  return area ? area.value : "";
}

async function sendTaskAgent(agentId, text, button, choosing) {
  const value = String(text || "").trim();
  if (!value) {
    const host = document.querySelector(`#task-dialog [data-task-follow="${CSS.escape(agentId)}"]`);
    const questions = host && host.querySelector("[data-task-question]");
    const radios = host && host.querySelector('input[type="radio"]');
    taskError(choosing ? (questions ? "Answer every question." : (radios ? "Choose an answer." : "Enter an answer.")) : "Enter a comment.");
    return;
  }
  if (!state.taskId) return;
  if (button) button.disabled = true;
  try {
    const data = await api("/api/tasks/" + state.taskId + "/agents/" + encodeURIComponent(agentId) + "/reply", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text: value }),
    });
    rememberTask(data.task);
    paintTaskLogs(data.task);
    watchTaskRuns();
    const error = document.querySelector("#task-error");
    error.hidden = true;
    error.textContent = "";
  } catch (err) {
    taskError(err.message);
    if (button) button.disabled = false;
  }
}

async function approveTaskAgent(agentId, button) {
  if (!state.taskId) return;
  if (button) button.disabled = true;
  try {
    const data = await api("/api/tasks/" + state.taskId + "/agents/" + encodeURIComponent(agentId) + "/approve", {
      method: "POST",
    });
    rememberTask(data.task);
    paintTaskLogs(data.task);
    const error = document.querySelector("#task-error");
    error.hidden = true;
    error.textContent = "";
  } catch (err) {
    taskError(err.message);
    if (button) button.disabled = false;
  }
}

function taskStatus(id) {
  return taskStatuses.find((item) => item.id === id) || taskStatuses[0];
}

function isoDate(date) {
  const part = (value) => String(value).padStart(2, "0");
  return date.getFullYear() + "-" + part(date.getMonth() + 1) + "-" + part(date.getDate());
}

function defaultDoneRange() {
  const end = new Date();
  const start = new Date();
  start.setDate(end.getDate() - 6);
  return { from: isoDate(start), to: isoDate(end) };
}

function doneRange() {
  const base = defaultDoneRange();
  const from = state.taskDoneFrom || base.from;
  const to = state.taskDoneTo || base.to;
  if (from <= to) return { from, to };
  return { from: to, to: from };
}

function taskDay(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return isoDate(date);
}

function taskInDoneRange(task, range) {
  const day = taskDay(task.doneAt || task.updatedAt || task.createdAt);
  if (!day) return false;
  return day >= range.from && day <= range.to;
}

function taskById(id) {
  return (state.data.tasks || []).find((task) => task.id === id);
}

function taskIdentity(task) {
  if (!task) return "";
  const parts = ["Task ID " + task.id];
  if (task.groupId) parts.push("Group ID " + task.groupId);
  return parts.join(" · ");
}

function groupById(id) {
  return ((state.data && state.data.groups) || []).find((group) => group.id === id);
}

function taskNote() {
  const name = document.querySelector("#task-name").value.trim();
  const description = document.querySelector("#task-description").value.trim();
  return ["Task: " + name, "", description].join("\n");
}

function jiraSettings() {
  const cfg = (state.data && state.data.atlassian) || {};
  const jira = cfg.jira || {};
  return {
    enabled: !!(cfg.enabled && jira.enabled),
    boardId: String(jira.boardId || ""),
    boardName: String(jira.boardName || ""),
    url: String(jira.url || "").replace(/\/$/, ""),
  };
}

function boardLabel(name, id) {
  name = String(name || "").trim();
  id = String(id || "").trim();
  if (!name) return id;
  if (!id || id === name) return name;
  return name + " (" + id + ")";
}

function epicBrowseURL(key) {
  const settings = jiraSettings();
  if (!settings.url || !key) return "";
  return settings.url + "/browse/" + encodeURIComponent(key);
}

function jiraTicketLink(key) {
  const url = epicBrowseURL(key);
  if (!url) return el("span", "badge", key);
  const link = el("a", "badge", key);
  link.href = url;
  link.setAttribute("aria-label", "Open " + key + " in Jira");
  bindJiraOpen(link, key);
  return link;
}

function bindJiraOpen(link, key) {
  link.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    openJiraTicket(key);
  });
}

async function openJiraTicket(key) {
  try {
    await api("/api/jira/open", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ key }),
    });
  } catch (error) {
    banner(error.message, true);
  }
}

function jiraTicketOrder(key) {
  const match = /^([A-Z][A-Z0-9]{1,9})-(\d+)$/.exec(String(key || "").trim().toUpperCase());
  if (!match) return null;
  return { project: match[1], number: Number(match[2]) };
}

function compareGroupsByTicket(a, b) {
  const left = jiraTicketOrder(a.epicKey);
  const right = jiraTicketOrder(b.epicKey);
  if (left && right) {
    if (left.project !== right.project) return left.project < right.project ? -1 : 1;
    if (left.number !== right.number) return left.number - right.number;
  } else if (left || right) {
    return left ? -1 : 1;
  }
  return String(a.name || "").localeCompare(String(b.name || ""), undefined, { sensitivity: "base" });
}

function taskGroupChoice(id, name, epicKey, selected, started) {
  const button = el("button", "task-group-choice");
  button.type = "button";
  button.dataset.taskGroup = id;
  button.setAttribute("role", "option");
  button.setAttribute("aria-selected", id === selected ? "true" : "false");
  button.disabled = started;
  if (epicKey) button.append(el("span", "badge", epicKey));
  button.append(el("span", "", name));
  return button;
}

function paintTaskGroupToggle() {
  const hidden = document.querySelector("#task-group");
  const value = document.querySelector("#task-group-value");
  const toggle = document.querySelector("#task-group-toggle");
  if (!hidden || !value || !toggle) return;
  const task = state.taskId ? taskById(state.taskId) : null;
  toggle.disabled = !!(task && task.started);
  value.replaceChildren();
  const group = groupById(hidden.value);
  if (group) {
    if (group.epicKey) value.append(el("span", "badge", group.epicKey));
    value.append(el("span", "", group.name || group.id));
    return;
  }
  value.append(el("span", hidden.value ? "" : "task-group-placeholder", hidden.value || "None"));
}

function fillTaskGroups(selected) {
  const hidden = document.querySelector("#task-group");
  const search = document.querySelector("#task-group-search");
  if (hidden) hidden.value = selected || "";
  if (search) search.value = "";
  setTaskGroupMenu(false);
  paintTaskGroups();
}

function paintTaskGroups() {
  const hidden = document.querySelector("#task-group");
  const search = document.querySelector("#task-group-search");
  const list = document.querySelector("#task-group-list");
  if (!hidden || !search || !list) return;
  const task = state.taskId ? taskById(state.taskId) : null;
  const started = !!(task && task.started);
  search.disabled = started;
  paintTaskGroupToggle();
  const selected = hidden.value;
  const query = search.value.trim().toLowerCase();
  const groups = [...((state.data && state.data.groups) || [])].filter((group) => !group.deleted || group.id === selected).sort(compareGroupsByTicket);
  const visible = groups.filter((group) => {
    if (!query) return true;
    const name = String(group.name || "").toLowerCase();
    const ticket = String(group.epicKey || "").toLowerCase();
    return name.includes(query) || ticket.includes(query);
  });
  list.replaceChildren();
  if (!query) list.append(taskGroupChoice("", "None", "", selected, started));
  if (!visible.length) list.append(el("p", "hint", query ? "No groups match this search." : "No groups yet."));
  for (const group of visible) list.append(taskGroupChoice(group.id, group.name, group.epicKey, selected, started));
  if (selected && !query && !groups.some((group) => group.id === selected)) {
    const missing = groupById(selected);
    list.append(taskGroupChoice(selected, (missing && missing.name) || selected, missing && missing.epicKey, selected, started));
  }
}

function formEpic() {
  const select = document.querySelector("#task-group");
  const group = select ? groupById(select.value) : null;
  if (!group || !group.epicKey) return null;
  return { key: group.epicKey, boardId: group.epicBoardId || "", boardName: group.epicBoardName || "", pending: false };
}

function agentJiraBoard(agent) {
  const item = ((agent && agent.mcps) || []).find((mcp) => mcp.id === "atlassian_jira");
  if (!item) return null;
  const shared = sharedPlace("atlassian_jira");
  return {
    boardId: item.boardId || shared.boardId || "",
    boardName: item.boardName || shared.boardName || "",
  };
}

function epicBlock(epic) {
  if (!epic || !epic.key) return "";
  return [
    "Jira epic: " + epic.key,
    "Board: " + boardLabel(epic.boardName, epic.boardId),
    "This task belongs to this Jira epic. Do the work under this epic.",
    "If you create Jira tickets that are not sub-tasks, create them under this epic.",
    "If you create sub-tasks, create those sub-task tickets as children of the ticket this agent needs.",
  ].join("\n");
}

function taskSendNote(epic, include) {
  let note = taskNote();
  if (include && epic && epic.key) note = note.replace(/\s+$/, "") + "\n\n" + epicBlock(epic);
  return note;
}

function orcaEpicReason(epic, jira, include) {
  if (!epic) return "No Jira epic is included.";
  const epicBoard = boardLabel(epic.boardName, epic.boardId);
  const agentBoard = jira ? boardLabel(jira.boardName, jira.boardId) : "";
  if (epic.pending) {
    if (!jira) return "Jira is off for this agent, so the new epic is left out.";
    if (!epic.boardId || jira.boardId !== epic.boardId) return "The new epic is on " + epicBoard + ". This agent's board is " + (agentBoard || "not set") + ", so the epic is left out.";
    return "A Jira epic is created on " + epicBoard + " when you start. Its key is added to this message, with instructions to put new tickets under that epic and sub-tasks under the ticket this agent needs.";
  }
  if (!jira) return "Jira is off for this agent, so epic " + epic.key + " is left out.";
  if (include) return "Jira epic " + epic.key + " on " + epicBoard + " is included.";
  return "Jira epic " + epic.key + " is on " + epicBoard + ". This agent's board is " + (agentBoard || "not set") + ", so the epic is left out.";
}

function taskManualSet() {
  if (!state.taskManual) state.taskManual = new Set();
  return state.taskManual;
}

// taskReach is every agent that turning on this agent also turns on. It follows
// Agents Coordination from agent to agent and skips agents that are off.
function taskReach(id) {
  const seen = new Set([id]);
  const queue = [id];
  const out = [];
  while (queue.length) {
    const agent = agentById(queue.shift());
    for (const peerId of (agent && agent.worksWith) || []) {
      if (seen.has(peerId)) continue;
      seen.add(peerId);
      const peer = agentById(peerId);
      if (!peer || peer.enabled === false) continue;
      out.push(peerId);
      queue.push(peerId);
    }
  }
  return out;
}

// taskDependents maps each agent that was turned on by another agent to the
// turned-on agents that list it under Agents Coordination.
function taskDependents() {
  const manual = taskManualSet();
  const on = [...document.querySelectorAll("[data-task-agent][aria-pressed='true']")].map((button) => button.dataset.taskAgent);
  const forced = new Map();
  for (const id of on) {
    if (manual.has(id)) continue;
    const requesters = on.filter((other) => other !== id && ((agentById(other) || {}).worksWith || []).includes(id));
    forced.set(id, requesters);
  }
  return forced;
}

function syncTaskForced() {
  const manual = taskManualSet();
  const forced = new Set();
  for (const id of manual) {
    for (const peer of taskReach(id)) {
      if (!manual.has(peer)) forced.add(peer);
    }
  }
  for (const button of document.querySelectorAll("[data-task-agent]")) {
    const id = button.dataset.taskAgent;
    const locked = forced.has(id);
    const on = locked || manual.has(id);
    button.setAttribute("aria-pressed", on ? "true" : "false");
    button.disabled = locked;
    button.removeAttribute("title");
    const picks = document.querySelector(`[data-task-picks="${CSS.escape(id)}"]`);
    if (picks) picks.hidden = !on;
    const prompt = document.querySelector(`[data-task-prompt="${CSS.escape(id)}"]`);
    if (prompt) prompt.hidden = !on;
  }
  const dependents = taskDependents();
  for (const [id, requesters] of dependents) {
    const button = document.querySelector(`[data-task-agent="${CSS.escape(id)}"]`);
    if (!button) continue;
    button.title = "Turned on by " + requesters.map(agentTitle).join(", ") + " through Agents Coordination. It waits for their request.";
  }
  paintSubAgents(manual, forced);
}

// paintSubAgents puts each agent that another agent turned on inside that main
// agent's border. An agent reached by more than one main sits in the first one.
function paintSubAgents(manual, forced) {
  const list = document.querySelector("#task-agents");
  if (!list) return;
  const reach = new Map([...manual].map((id) => [id, taskReach(id)]));
  const rows = [...list.querySelectorAll(".feature-agent")];
  for (const row of rows) clearTaskSubMark(row);
  for (const row of rows) {
    const section = list.querySelector(`.feature-group[data-feature-home="${CSS.escape(row.dataset.featureHome || "")}"]`);
    if (section) section.append(row);
  }
  for (const nest of list.querySelectorAll(".feature-subs")) nest.remove();
  for (const section of list.querySelectorAll(".feature-group")) {
    const agents = [...section.querySelectorAll(":scope > .feature-agent")];
    agents.sort((a, b) => Number(a.dataset.featureOrder) - Number(b.dataset.featureOrder));
    for (const row of agents) section.append(row);
  }
  for (const id of manual) {
    const subs = (reach.get(id) || []).filter((peer) => {
      if (!forced.has(peer)) return false;
      const mains = [...manual].filter((main) => (reach.get(main) || []).includes(peer));
      return mains[0] === id;
    });
    const mainRow = taskAgentCard(id);
    if (!mainRow || !subs.length) continue;
    const nest = el("div", "feature-subs");
    for (const peer of subs) {
      const subRow = taskAgentCard(peer);
      if (!subRow || subRow === mainRow) continue;
      nest.append(subRow);
      markTaskSub(subRow, [...manual].filter((main) => (reach.get(main) || []).includes(peer)));
    }
    if (!nest.childElementCount) continue;
    mainRow.classList.add("is-main");
    mainRow.append(nest);
  }
}

function taskAgentCard(id) {
  const button = document.querySelector(`#task-agents [data-task-agent="${CSS.escape(id)}"]`);
  return button ? button.closest(".feature-agent") : null;
}

function clearTaskSubMark(row) {
  row.classList.remove("is-sub", "is-main");
  const label = row.querySelector(":scope > .feature-toggle .feature-label");
  if (label) {
    for (const node of [...label.children]) {
      if (node.classList.contains("feature-sub")) node.remove();
    }
  }
  const controls = row.querySelector(":scope > .feature-toggle .feature-controls");
  if (controls) {
    for (const node of [...controls.children]) {
      if (node.classList.contains("feature-sub-badge")) node.remove();
    }
  }
}

function markTaskSub(row, mains) {
  row.classList.add("is-sub");
  const controls = row.querySelector(":scope > .feature-toggle .feature-controls");
  const label = row.querySelector(":scope > .feature-toggle .feature-label");
  if (controls) controls.insertBefore(el("span", "badge sub feature-sub-badge", "Sub agent"), controls.firstChild);
  if (label) label.append(el("span", "feature-sub", "Sub agent of " + mains.map(agentTitle).join(", ") + ". Waits for their request."));
}

// taskInitialManual picks the agents the user turned on from a saved list. An agent
// that another one in the list turns on is not one of them.
function taskInitialManual(ids) {
  const manual = new Set();
  const forced = new Set();
  for (const id of ids) {
    if (forced.has(id)) continue;
    manual.add(id);
    for (const peer of taskReach(id)) {
      forced.add(peer);
      manual.delete(peer);
    }
  }
  return manual;
}

function taskAgentChoice(agent) {
  const tool = document.querySelector(`[data-task-tool="${CSS.escape(agent.id)}"]`);
  const model = document.querySelector(`[data-task-model="${CSS.escape(agent.id)}"]`);
  if (tool && model) return { tool: tool.value, model: model.value };
  return { tool: agent.tool || "", model: agent.model || "" };
}

function taskSendAgents() {
  const task = state.taskId ? taskById(state.taskId) : null;
  if (task && task.started) {
    return (task.agents || []).map((item) => agentById(item.agentId) || { id: item.agentId, title: item.agentId, mcps: [] });
  }
  const chosen = [];
  for (const button of document.querySelectorAll("[data-task-agent][aria-pressed='true']")) {
    const agent = agentById(button.dataset.taskAgent);
    if (agent && agent.enabled !== false) chosen.push(agent);
  }
  return chosen;
}

function syncTaskGroup() {
  const note = document.querySelector("#task-group-note");
  const select = document.querySelector("#task-group");
  if (!select || !note) return;
  paintTaskGroups();
  note.hidden = false;
  note.replaceChildren();
  const group = groupById(select.value);
  if (group && group.epicKey) {
    note.append(document.createTextNode("This task belongs to Jira epic "));
    const url = epicBrowseURL(group.epicKey);
    if (url) {
      const link = el("a", "", group.epicKey);
      link.href = url;
      bindJiraOpen(link, group.epicKey);
      note.append(link);
    } else note.append(document.createTextNode(group.epicKey));
    const board = boardLabel(group.epicBoardName, group.epicBoardId);
    note.append(document.createTextNode(board ? " on " + board + "." : "."));
    return;
  }
  if (group) {
    note.textContent = "This group has no Jira epic.";
    return;
  }
  const groups = (state.data && state.data.groups) || [];
  note.textContent = groups.length
    ? "Choose a group to put this task under that group's Jira epic."
    : "Create a group first. A Jira epic is created when Jira is on in Settings.";
}

function paintOrcaSend() {
  const host = document.querySelector("#task-orca-body");
  if (!host) return;
  const label = orchestratorLabel();
  const agents = taskSendAgents();
  host.replaceChildren();
  if (!agents.length) {
    host.append(el("p", "hint", "Turn on an agent to see what Start sends to " + label + "."));
    return;
  }
  const epic = formEpic();
  for (const agent of agents) {
    const block = el("section", "task-orca-agent");
    const choice = taskAgentChoice(agent);
    block.append(el("h4", "", agent.title || agent.id));
    const meta = [choice.tool, choice.model].filter(Boolean).join(" · ");
    if (meta) block.append(el("p", "hint", meta));
    const jira = agentJiraBoard(agent);
    const include = !!(epic && epic.key && jira && epic.boardId && jira.boardId === epic.boardId);
    block.append(el("p", "hint", orcaEpicReason(epic, jira, include)));
    const waitFor = taskDependents().get(agent.id);
    if (waitFor && waitFor.length) {
      block.append(el("p", "hint", "Starts first and waits for a request from " + waitFor.map(agentTitle).join(", ") + " before it works."));
    }
    const field = el("div", "field");
    const label = el("span", "", "Prompt");
    label.id = "task-send-label-" + agent.id;
    const compose = el("div", "compose");
    const area = el("textarea");
    area.rows = 8;
    area.dataset.taskSendNote = agent.id;
    area.value = taskSendNote(epic, include);
    area.setAttribute("aria-labelledby", label.id);
    compose.append(area, generateButton("prompt", agent.title || agent.id));
    field.append(label, compose);
    block.append(field);
    host.append(block);
  }
}

function taskSections(task) {
  const assigned = new Map((task.agents || []).map((item) => [item.agentId, item.status || "pending"]));
  const runs = new Map((task.runs || []).map((run) => [run.agentId, run]));
  const sections = [];
  const seen = new Set();
  for (const group of groups(state.data.agents || [])) {
    const rows = [];
    for (const agent of group.agents) {
      if (!assigned.has(agent.id)) continue;
      seen.add(agent.id);
      const status = taskStatus(assigned.get(agent.id));
      rows.push({ id: agent.id, title: agent.title, status, run: runs.get(agent.id) || null });
    }
    if (rows.length) sections.push({ name: group.name, rows });
  }
  const missing = [];
  for (const [id, raw] of assigned) {
    if (seen.has(id)) continue;
    missing.push({ id, title: id, status: taskStatus(raw), run: runs.get(id) || null });
  }
  if (missing.length) sections.push({ name: "", rows: missing });
  return sections;
}

function renderTasks() {
  const panel = document.querySelector("#panel-tasks");
  if (!panel || !state.data) return;
  const range = doneRange();
  const tasks = state.data.tasks || [];
  const query = state.taskQuery.trim().replace(/^#/, "").toLowerCase();
  panel.replaceChildren();
  panel.append(taskViewTabs());
  if (state.taskView === "groups") {
    panel.append(renderGroupList());
    return;
  }
  const head = el("div", "tasks-head");
  const search = el("input");
  search.type = "search";
  search.id = "task-search";
  search.placeholder = "Group ID";
  search.value = state.taskQuery;
  search.setAttribute("aria-label", "Search by group ID");
  search.autocomplete = "off";
  const actions = el("div", "tasks-actions");
  const addGroup = el("button", "", "New group");
  addGroup.type = "button";
  addGroup.id = "group-new";
  const create = el("button", "primary", "New task");
  create.type = "button";
  create.id = "task-new";
  actions.append(addGroup, create);
  head.append(search, actions);
  panel.append(head);
  const board = el("div", "task-board");
  for (const column of taskColumns) {
    const inColumn = tasks.filter((task) => task.column === column.id && (column.id !== "done" || taskInDoneRange(task, range)));
    const visible = inColumn.filter((task) => !query || String(task.groupId || "").toLowerCase().includes(query));
    const section = el("section", "task-column");
    section.dataset.column = column.id;
    const title = el("div", "task-column-head");
    title.append(el("h2", "", column.label));
    title.append(el("span", "task-count", String(visible.length)));
    section.append(title);
    if (column.id === "done") section.append(taskDoneRange(range));
    const cards = el("div", "task-column-cards");
    if (!visible.length) {
      const empty = query ? "No tasks match this group ID." : (column.id === "done" ? "No tasks in this range." : "No tasks.");
      cards.append(el("p", "hint", empty));
    }
    for (const task of visible) cards.append(taskCard(task));
    section.append(cards);
    board.append(section);
  }
  panel.append(board);
}

function taskViewTabs() {
  const nav = el("div", "task-tabs");
  nav.setAttribute("role", "tablist");
  nav.setAttribute("aria-label", "Tasks");
  nav.append(taskViewTab("board", "Board"), taskViewTab("groups", "Group"));
  return nav;
}

function taskViewTab(id, label) {
  const button = el("button", "task-tab", label);
  button.type = "button";
  button.dataset.taskView = id;
  button.setAttribute("role", "tab");
  button.setAttribute("aria-selected", state.taskView === id ? "true" : "false");
  return button;
}

function activeGroups() {
  return ((state.data && state.data.groups) || []).filter((group) => !group.deleted);
}

function renderGroupList() {
  const wrap = el("div", "group-list");
  const head = el("div", "tasks-head");
  const actions = el("div", "tasks-actions");
  const add = el("button", "primary", "New group");
  add.type = "button";
  add.id = "group-new";
  actions.append(add);
  head.append(actions);
  wrap.append(head);
  const groups = [...activeGroups()].sort(compareGroupsByTicket);
  if (!groups.length) {
    wrap.append(el("p", "hint", "No groups yet. Create a group to make a Jira epic, then choose that group on a task."));
    return wrap;
  }
  for (const group of groups) wrap.append(groupRow(group));
  return wrap;
}

function groupRow(group) {
  const row = el("article", "card group-row");
  const main = el("div", "group-row-main");
  const title = el("div", "group-row-title");
  title.append(el("h3", "", group.name));
  if (group.epicKey) title.append(jiraTicketLink(group.epicKey));
  main.append(title);
  main.append(el("p", "task-id", group.id));
  main.append(el("p", group.description ? "summary" : "hint", group.description || "No description."));
  const remove = el("button", "", "Delete");
  remove.type = "button";
  remove.dataset.deleteGroup = group.id;
  row.append(main, remove);
  return row;
}

function openDeleteGroup(id) {
  const group = groupById(id);
  if (!group) return;
  const dialog = document.querySelector("#group-delete-dialog");
  dialog.dataset.groupId = id;
  document.querySelector("#group-delete-title").textContent = "Delete " + group.name;
  document.querySelector("#group-delete-text").textContent = "This removes the group from the list. Tasks that already use it stay on the board.";
  const line = document.querySelector("#group-delete-jira-line");
  const box = document.querySelector("#group-delete-jira");
  box.checked = false;
  if (group.epicKey) {
    line.hidden = false;
    document.querySelector("#group-delete-jira-label").textContent = "Also delete Jira ticket " + group.epicKey;
  } else {
    line.hidden = true;
  }
  const error = document.querySelector("#group-delete-error");
  error.hidden = true;
  error.textContent = "";
  document.querySelector("#group-delete-go").disabled = false;
  dialog.showModal();
}

async function confirmDeleteGroup() {
  const dialog = document.querySelector("#group-delete-dialog");
  const id = dialog.dataset.groupId;
  const group = groupById(id);
  const jira = !document.querySelector("#group-delete-jira-line").hidden && document.querySelector("#group-delete-jira").checked;
  const error = document.querySelector("#group-delete-error");
  const button = document.querySelector("#group-delete-go");
  if (!id) {
    error.hidden = false;
    error.textContent = "Choose a group.";
    return;
  }
  button.disabled = true;
  error.hidden = true;
  try {
    await api("/api/groups/" + encodeURIComponent(id), {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jira }),
    });
    if (group) group.deleted = true;
    dialog.close();
    renderTasks();
    const name = (group && group.name) || "the group";
    banner(jira && group && group.epicKey ? "Deleted " + name + " and Jira ticket " + group.epicKey + "." : "Deleted " + name + ".");
  } catch (err) {
    error.hidden = false;
    error.textContent = err.message;
    button.disabled = false;
  }
}

function taskDoneRange(range) {
  const box = el("div", "task-range");
  box.append(taskDateField("From", "task-done-from", range.from), taskDateField("To", "task-done-to", range.to));
  if (state.taskDoneFrom || state.taskDoneTo) {
    const reset = el("button", "task-range-reset", "Last 7 days");
    reset.type = "button";
    reset.id = "task-done-reset";
    box.append(reset);
  }
  return box;
}

function taskDateField(label, id, value) {
  const field = el("label");
  field.append(el("span", "", label));
  const input = el("input");
  input.type = "date";
  input.id = id;
  input.value = value;
  field.append(input);
  return field;
}

function taskCard(task) {
  const card = el("button", "card task-card");
  card.type = "button";
  card.dataset.task = String(task.id);
  card.append(el("p", "task-id", task.id));
  const heading = el("div", "task-card-title");
  heading.append(el("h3", "", task.name));
  card.append(heading);
  if (task.groupId) {
    const group = groupById(task.groupId);
    const name = (group && group.name) || "";
    const ticket = (group && group.epicKey) || task.epicKey || "";
    const label = ["Group:", name, ticket].filter(Boolean).join(" ");
    if (name || ticket) card.append(el("p", "hint", label));
  }
  card.append(el("p", task.description ? "summary" : "hint", task.description || "No description."));
  const sections = taskSections(task);
  if (!sections.length) card.append(el("p", "hint", "No agents are assigned."));
  for (const section of sections) {
    const block = el("div", "task-category");
    block.append(el("h4", "", categoryLabel(section.name)));
    for (const row of section.rows) {
      const line = el("div", "task-agent");
      line.append(el("span", "task-agent-name", row.title));
      const marks = el("span", "task-agent-marks");
      const live = orcaMark(row.run);
      if (live) marks.append(el("span", "badge " + live.badge, live.label));
      marks.append(el("span", "badge" + (row.status.badge ? " " + row.status.badge : ""), row.status.label));
      line.append(marks);
      block.append(line);
      if (row.run) block.append(taskLogNode(row.run));
    }
    card.append(block);
  }
  return card;
}

function rememberTask(task) {
  state.data.tasks = (state.data.tasks || []).filter((item) => item.id !== task.id);
  state.data.tasks.unshift(task);
  renderTasks();
}

function forgetTask(id) {
  state.data.tasks = (state.data.tasks || []).filter((item) => item.id !== id);
  renderTasks();
}

function taskError(message) {
  const error = document.querySelector("#task-error");
  error.hidden = false;
  error.textContent = message;
}

function taskFormBody() {
  const task = state.taskId ? taskById(state.taskId) : null;
  return {
    name: document.querySelector("#task-name").value.trim(),
    description: document.querySelector("#task-description").value.trim(),
    column: task ? task.column : "todo",
    groupId: document.querySelector("#task-group").value,
    agents: [...document.querySelectorAll("[data-task-agent][aria-pressed='true']")].map((button) => {
      const id = button.dataset.taskAgent;
      const task = state.taskId ? taskById(state.taskId) : null;
      const stored = ((task && task.agents) || []).find((agent) => agent.agentId === id);
      return { agentId: id, status: stored ? stored.status : "pending" };
    }),
  };
}

async function saveTaskForm() {
  const body = taskFormBody();
  if (!body.name) {
    taskError("Enter a task name.");
    return null;
  }
  const creating = !state.taskId;
  const data = await api(creating ? "/api/tasks" : "/api/tasks/" + state.taskId, {
    method: creating ? "POST" : "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  rememberTask(data.task);
  state.taskId = data.task.id;
  document.querySelector("#task-delete").hidden = false;
  document.querySelector("#task-dialog-title").textContent = data.task.name;
  const idLine = document.querySelector("#task-id");
  idLine.hidden = false;
  idLine.textContent = taskIdentity(data.task);
  fillTaskGroups(data.task.groupId || "");
  syncTaskGroup();
  return data.task;
}

function taskQuestionItem(agentId, question, index) {
  const item = el("div", "task-question");
  const name = (question.prompt || "").trim() || "Question " + (index + 1);
  item.dataset.taskQuestion = name;
  item.append(el("p", "task-ask-prompt", name));
  if ((question.detail || "").trim()) item.append(el("p", "hint", question.detail.trim()));
  const suggestion = (question.suggestion || "").trim();
  if (suggestion) item.append(el("p", "hint", "Suggested: " + suggestion));
  const options = question.options || [];
  if (options.length) {
    for (const option of options) {
      const label = el("label", "task-choice");
      const input = el("input");
      input.type = "radio";
      input.name = "ask-" + agentId + "-" + index;
      input.value = option;
      input.checked = suggestion === option;
      label.append(input, document.createTextNode(" " + option));
      item.append(label);
    }
  } else {
    const area = el("textarea");
    area.rows = 2;
    area.placeholder = "Your answer";
    area.value = suggestion;
    item.append(area);
  }
  return item;
}

function taskFollowup(agentId, run, locked) {
  const box = el("div", "task-followup");
  box.dataset.taskFollow = agentId;
  const questions = (run && run.askQuestions) || [];
  const options = (run && run.askOptions) || [];
  if (questions.length) {
    const ask = el("div", "task-ask");
    questions.forEach((question, index) => ask.append(taskQuestionItem(agentId, question, index)));
    const submit = el("button", "", "Submit");
    submit.type = "button";
    submit.dataset.taskAskSubmit = agentId;
    ask.append(submit);
    box.append(ask);
  } else if (run && run.askPrompt) {
    const ask = el("div", "task-ask");
    ask.append(el("p", "task-ask-prompt", run.askPrompt));
    if (options.length) {
      for (const option of options) {
        const label = el("label", "task-choice");
        const input = el("input");
        input.type = "radio";
        input.name = "ask-" + agentId;
        input.value = option;
        label.append(input, document.createTextNode(" " + option));
        ask.append(label);
      }
    } else {
      const area = el("textarea");
      area.dataset.taskAnswer = agentId;
      area.rows = 3;
      area.placeholder = "Your answer";
      ask.append(area);
    }
    const submit = el("button", "", "Submit");
    submit.type = "button";
    submit.dataset.taskAskSubmit = agentId;
    ask.append(submit);
    box.append(ask);
  }
  if (!locked) {
    const comment = el("label", "field");
    comment.append(el("span", "", "Comments"));
    const area = el("textarea");
    area.dataset.taskComment = agentId;
    area.rows = 3;
    area.placeholder = "Ask this agent to continue";
    comment.append(area);
    box.append(comment);
  }
  const actions = el("div", "task-follow-actions");
  if (!locked) {
    const send = el("button", "", "Send");
    send.type = "button";
    send.dataset.taskSend = agentId;
    actions.append(send);
  }
  const approve = el("button", "primary", "Approve");
  approve.type = "button";
  approve.dataset.taskApprove = agentId;
  actions.append(approve);
  box.append(actions);
  return box;
}

function taskPicks(agent) {
  const picks = el("div", "agent-picks task-picks");
  picks.dataset.taskPicks = agent.id;
  if (agent.enabled === false) {
    picks.hidden = true;
    return picks;
  }
  const choice = choiceFor(agent.id);
  const toolField = el("label", "field");
  toolField.append(el("span", "", "AI Tool"));
  const toolSelect = el("select");
  toolSelect.dataset.taskTool = agent.id;
  fillToolSelect(toolSelect, choice.tool);
  toolField.append(toolSelect);
  const modelField = el("label", "field");
  modelField.append(el("span", "", "Model"));
  const modelSelect = el("select");
  modelSelect.dataset.taskModel = agent.id;
  fillModels(modelSelect, choice.tool, choice.model);
  modelField.append(modelSelect);
  picks.append(toolField, modelField);
  return picks;
}

function taskAgentRow(agent, status, on, run) {
  const row = el("div", "feature-agent");
  const toggle = el("div", "feature-toggle");
  const text = el("span", "feature-label");
  text.append(el("span", "feature-title", agent.title));
  text.append(el("span", "feature-id", "ID: " + agent.id));
  const controls = el("div", "feature-controls");
  if (on) {
    const statusInfo = taskStatus(status);
    const badge = el("span", "badge" + (statusInfo.badge ? " " + statusInfo.badge : ""), statusInfo.label);
    badge.dataset.taskState = agent.id;
    controls.append(badge);
  }
  if (agent.enabled === false) controls.append(el("span", "badge bad", "Off"));
  else {
    const prompt = el("button", "icon-button feature-prompt");
    prompt.type = "button";
    prompt.hidden = !on;
    prompt.dataset.taskPrompt = agent.id;
    prompt.setAttribute("aria-label", "Show the prompt for " + agent.title);
    prompt.append(promptIcon());
    controls.append(prompt);
  }
  const button = el("button", "switch");
  button.type = "button";
  button.dataset.taskAgent = agent.id;
  button.setAttribute("aria-pressed", on ? "true" : "false");
  button.setAttribute("aria-label", agent.title);
  button.append(el("span", "switch-knob"));
  controls.append(button);
  toggle.append(text, controls);
  const picks = taskPicks(agent);
  picks.hidden = !on;
  row.append(toggle, picks);
  if (run) row.append(taskLogNode(run, agent.id));
  if (on && status === "review") {
    const follow = taskFollowup(agent.id, run);
    follow.dataset.key = followKey({ status }, run);
    row.append(follow);
  }
  return row;
}

function taskAgentPanel(agent, status, run) {
  const panel = el("div", "task-agent-panel");
  panel.dataset.taskPanel = agent.id;
  panel.setAttribute("role", "tabpanel");
  const marks = el("div", "task-agent-marks");
  const live = orcaMark(run);
  if (live) marks.append(el("span", "badge " + live.badge, live.label));
  const statusInfo = taskStatus(status);
  const badge = el("span", "badge" + (statusInfo.badge ? " " + statusInfo.badge : ""), statusInfo.label);
  badge.dataset.taskState = agent.id;
  marks.append(badge);
  panel.append(marks);
  if (run) panel.append(taskLogNode(run, agent.id));
  else panel.append(el("p", "hint", "Waiting for output from Orca."));
  if (status === "review") {
    const follow = taskFollowup(agent.id, run, true);
    follow.dataset.key = followKey({ status }, run);
    panel.append(follow);
  }
  return panel;
}

function renderStartedAgents(task) {
  const list = document.querySelector("#task-agents");
  const runs = new Map((task.runs || []).map((run) => [run.agentId, run]));
  const tabs = el("div", "task-tabs");
  tabs.setAttribute("role", "tablist");
  const panels = [];
  (task.agents || []).forEach((item, index) => {
    const agent = agentById(item.agentId) || { id: item.agentId, title: item.agentId, enabled: true };
    const status = item.status || "pending";
    const statusInfo = taskStatus(status);
    const tab = el("button", "task-tab");
    tab.type = "button";
    tab.dataset.taskTab = agent.id;
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-selected", index === 0 ? "true" : "false");
    tab.append(document.createTextNode(agent.title + " "));
    const mark = el("span", "badge" + (statusInfo.badge ? " " + statusInfo.badge : ""), statusInfo.label);
    mark.dataset.taskTabState = agent.id;
    tab.append(mark);
    tabs.append(tab);
    const panel = taskAgentPanel(agent, status, runs.get(item.agentId));
    panel.hidden = index !== 0;
    panels.push(panel);
  });
  list.replaceChildren(tabs);
  if (!panels.length) list.append(el("p", "hint", "No agents are assigned."));
  for (const panel of panels) list.append(panel);
}

function openTask(id) {
  const task = id ? taskById(id) : null;
  state.taskId = task ? task.id : "";
  const started = !!(task && task.started);
  document.querySelector("#task-dialog-title").textContent = task ? task.name : "New task";
  const idLine = document.querySelector("#task-id");
  idLine.hidden = !task;
  idLine.textContent = taskIdentity(task);
  const name = document.querySelector("#task-name");
  const description = document.querySelector("#task-description");
  name.value = task ? task.name : "";
  description.value = task ? task.description : "";
  name.readOnly = started;
  description.readOnly = started;
  for (const button of document.querySelectorAll("#task-dialog [data-generate]")) button.hidden = started;
  paintTaskColumn(task ? task.column : "todo");
  const assigned = new Map((task ? task.agents : []).map((item) => [item.agentId, item.status || "pending"]));
  const runs = new Map((task && task.runs ? task.runs : []).map((run) => [run.agentId, run]));
  const list = document.querySelector("#task-agents");
  list.replaceChildren();
  if (started) {
    renderStartedAgents(task);
  } else {
    const seen = new Set();
    let home = 0;
    for (const group of groups(state.data.agents || [])) {
      const agents = group.agents.filter((agent) => agent.enabled !== false || assigned.has(agent.id));
      if (!agents.length) continue;
      const section = el("section", "feature-group");
      section.dataset.featureHome = String(home++);
      section.append(el("h3", "", categoryLabel(group.name)));
      agents.forEach((agent, index) => {
        seen.add(agent.id);
        const row = taskAgentRow(agent, assigned.get(agent.id) || "pending", assigned.has(agent.id), runs.get(agent.id));
        row.dataset.featureHome = section.dataset.featureHome;
        row.dataset.featureOrder = String(index);
        section.append(row);
      });
      list.append(section);
    }
    const missing = [...assigned.keys()].filter((agentId) => !seen.has(agentId));
    if (missing.length) {
      const section = el("section", "feature-group");
      section.dataset.featureHome = String(home++);
      section.append(el("h3", "", "Agents"));
      missing.forEach((agentId, index) => {
        const row = taskAgentRow({ id: agentId, title: agentId, enabled: false }, assigned.get(agentId) || "pending", true, runs.get(agentId));
        row.dataset.featureHome = section.dataset.featureHome;
        row.dataset.featureOrder = String(index);
        section.append(row);
      });
      list.append(section);
    }
    if (!list.childElementCount) list.append(el("p", "hint", "No agents are turned on."));
    const ordered = (state.data.agents || []).map((agent) => agent.id).filter((agentId) => assigned.has(agentId));
    for (const agentId of assigned.keys()) if (!ordered.includes(agentId)) ordered.push(agentId);
    state.taskManual = taskInitialManual(ordered);
    syncTaskForced();
  }
  const error = document.querySelector("#task-error");
  error.hidden = true;
  error.textContent = "";
  const result = document.querySelector("#task-result");
  result.hidden = true;
  result.textContent = "";
  result.classList.remove("error");
  const label = orchestratorLabel();
  document.querySelector("#task-lead").textContent = started
    ? "This task is running in " + label + ". Each tab is one agent. Answer a question and submit it. Approve marks that agent Completed. When every agent is Completed, the task moves to Done."
    : "Turn on the agents for this task. An agent also turns on the agents in its Agents Coordination, and those cannot be turned off on their own. They start first and wait for a request before they work. Start sends the name and description to the agents that are on, groups them in " + label + " by this group ID, and moves this task to In Progress. Kiki shows each agent's thinking and replies. When an agent asks a question or finishes, its status becomes Waiting for review. Approve marks that agent Completed. When every agent is Completed, the task moves to Done.";
  document.querySelector("#task-delete").hidden = !task;
  const go = document.querySelector("#task-go");
  const save = document.querySelector("#task-save");
  go.hidden = started;
  save.hidden = started;
  go.disabled = false;
  go.textContent = "Start on " + label;
  save.disabled = false;
  fillTaskGroups(task ? task.groupId : "");
  syncTaskGroup();
  document.querySelector("#task-dialog").showModal();
  placeGenerateButtons();
  if (!started) name.focus();
}

async function openTaskPrompt(id) {
  const agent = agentById(id);
  const tool = document.querySelector(`[data-task-tool="${CSS.escape(id)}"]`).value;
  const model = document.querySelector(`[data-task-model="${CSS.escape(id)}"]`).value;
  const dialog = document.querySelector("#prompt-reader");
  document.querySelector("#prompt-reader-title").textContent = agent.title + " — prompt sent to " + orchestratorLabel();
  document.querySelector("#prompt-reader-body").textContent = "Loading…";
  dialog.showModal();
  try {
    const data = await api("/api/agents/" + encodeURIComponent(id) + "/preview", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        note: taskNote(),
        repo: agent.repos[0],
        agent: tool,
        model,
        taskId: state.taskId || "",
        waitFor: taskDependents().get(id) || [],
      }),
    });
    document.querySelector("#prompt-reader-body").innerHTML = renderMarkdown(data.prompt);
  } catch (error) {
    dialog.close();
    taskError(error.message);
  }
}

async function saveTask() {
  const current = state.taskId ? taskById(state.taskId) : null;
  if (current && current.started) return;
  const error = document.querySelector("#task-error");
  error.hidden = true;
  const button = document.querySelector("#task-save");
  button.disabled = true;
  try {
    const task = await saveTaskForm();
    if (!task) return;
    document.querySelector("#task-dialog").close();
    banner("Saved " + task.name + ".");
  } catch (err) {
    taskError(err.message);
  } finally {
    button.disabled = false;
  }
}

async function deleteTask() {
  if (!state.taskId) return;
  const button = document.querySelector("#task-delete");
  button.disabled = true;
  try {
    await api("/api/tasks/" + state.taskId, { method: "DELETE" });
    forgetTask(state.taskId);
    state.taskId = "";
    document.querySelector("#task-dialog").close();
    banner("Deleted the task.");
  } catch (err) {
    taskError(err.message);
  } finally {
    button.disabled = false;
  }
}

function taskStartAgents() {
  const chosen = [];
  for (const button of document.querySelectorAll("[data-task-agent][aria-pressed='true']")) {
    const agent = agentById(button.dataset.taskAgent);
    if (!agent || agent.enabled === false) continue;
    chosen.push(button);
  }
  return chosen;
}

function reviewTaskStart() {
  const current = state.taskId ? taskById(state.taskId) : null;
  if (current && current.started) return;
  const error = document.querySelector("#task-error");
  error.hidden = true;
  const name = document.querySelector("#task-name").value.trim();
  const description = document.querySelector("#task-description").value.trim();
  if (!name) {
    taskError("Enter a task name.");
    return;
  }
  if (!description) {
    taskError("Describe this task.");
    return;
  }
  if (!document.querySelector("#task-group").value) {
    taskError("Choose a group before starting this task.");
    return;
  }
  if (taskNote().length > 4000) {
    taskError("Description is longer than 4000 characters.");
    return;
  }
  if (!taskStartAgents().length) {
    taskError("Turn on at least one agent.");
    return;
  }
  const label = orchestratorLabel();
  document.querySelector("#task-start-title").textContent = "Start on " + label;
  document.querySelector("#task-start-go").textContent = "Start on " + label;
  const startError = document.querySelector("#task-start-error");
  startError.hidden = true;
  startError.textContent = "";
  paintOrcaSend();
  document.querySelector("#task-start-dialog").showModal();
  requestAnimationFrame(placeGenerateButtons);
}

function taskStartPrompts() {
  const notes = new Map();
  for (const box of taskStartAgents()) {
    const id = box.dataset.taskAgent;
    const area = document.querySelector(`#task-start-dialog [data-task-send-note="${CSS.escape(id)}"]`);
    const text = area ? area.value.trim() : "";
    if (!text) return { error: "Enter the prompt." };
    if (text.length > 8000) return { error: "Prompt is longer than 8000 characters." };
    notes.set(id, text);
  }
  return { notes };
}

function taskStartError(message) {
  const error = document.querySelector("#task-start-error");
  if (!error) return;
  error.hidden = false;
  error.textContent = message;
}

async function startTask(notes) {
  const current = state.taskId ? taskById(state.taskId) : null;
  if (current && current.started) return;
  const error = document.querySelector("#task-error");
  error.hidden = true;
  const name = document.querySelector("#task-name").value.trim();
  const description = document.querySelector("#task-description").value.trim();
  if (!name) {
    taskError("Enter a task name.");
    return;
  }
  if (!description) {
    taskError("Describe this task.");
    return;
  }
  if (!document.querySelector("#task-group").value) {
    taskError("Choose a group before starting this task.");
    return;
  }
  const note = taskNote();
  if (note.length > 4000) {
    taskError("Description is longer than 4000 characters.");
    return;
  }
  const prompts = notes || taskStartPrompts().notes;
  const checked = taskStartAgents();
  if (!checked.length) {
    taskError("Turn on at least one agent.");
    return;
  }
  const go = document.querySelector("#task-go");
  const result = document.querySelector("#task-result");
  const label = orchestratorLabel();
  go.disabled = true;
  go.textContent = "Starting…";
  result.hidden = false;
  result.classList.remove("error");
  result.textContent = "Saving…";
  let task;
  try {
    task = await saveTaskForm();
  } catch (err) {
    result.hidden = true;
    taskError(err.message);
    go.disabled = false;
    go.textContent = "Start on " + label;
    return;
  }
  if (!task) {
    result.hidden = true;
    go.disabled = false;
    go.textContent = "Start on " + label;
    return;
  }
  result.textContent = "Naming the branch…";
  let named;
  try {
    named = await api("/api/tasks/" + task.id + "/branch", { method: "POST" });
  } catch (err) {
    result.hidden = true;
    taskError(err.message);
    go.disabled = false;
    go.textContent = "Start on " + label;
    return;
  }
  const lines = [];
  const started = [];
  let failed = 0;
  const dependents = taskDependents();
  const ordered = checked.slice().sort((a, b) => Number(dependents.has(b.dataset.taskAgent)) - Number(dependents.has(a.dataset.taskAgent)));
  for (const box of ordered) {
    const id = box.dataset.taskAgent;
    const agent = agentById(id);
    const tool = document.querySelector(`[data-task-tool="${CSS.escape(id)}"]`).value;
    const model = document.querySelector(`[data-task-model="${CSS.escape(id)}"]`).value;
    result.textContent = "Starting " + id + " on " + label + "…";
    try {
      const data = await api("/api/agents/" + encodeURIComponent(id) + "/launch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          note: (prompts && prompts.get(id)) || note,
          noteFinal: !!(prompts && prompts.get(id)),
          repo: agent.repos[0],
          agent: tool,
          model,
          slug: named.slug,
          taskId: String(task.id),
          ticket: named.ticket || "",
          waitFor: dependents.get(id) || [],
        }),
      });
      if (data.launch.status === "ok") {
        started.push(id);
        lines.push(id + ": started on " + label);
      } else {
        failed += 1;
        lines.push(id + ": " + data.launch.detail);
      }
    } catch (err) {
      failed += 1;
      lines.push(id + ": " + err.message);
    }
  }
  try {
    const fresh = await api("/api/state");
    state.data = fresh;
    renderTasks();
    watchTaskRuns();
  } catch (err) {
    lines.push(err.message);
  }
  result.classList.toggle("error", failed > 0);
  result.textContent = lines.join("\n");
  if (failed === 0) {
    document.querySelector("#task-dialog").close();
    banner("Started " + started.length + (started.length === 1 ? " agent" : " agents") + " on " + label + " for " + name + ".");
    return;
  }
  const running = taskById(task.id);
  if (running && running.started) {
    openTask(task.id);
    const shown = document.querySelector("#task-result");
    shown.hidden = false;
    shown.classList.add("error");
    shown.textContent = lines.join("\n");
    return;
  }
  go.disabled = false;
  go.textContent = "Start on " + label;
}

document.querySelector("#task-form").addEventListener("submit", (event) => {
  event.preventDefault();
  reviewTaskStart();
});
document.querySelector("#task-start-dialog").addEventListener("input", (event) => {
  if (event.target.closest(".compose textarea")) placeGenerateButtons();
  resetGenerateUndo(event.target);
});
document.querySelector("#task-start-dialog").addEventListener("close", stopGenerateRuns);
document.querySelector("#task-start-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const prompts = taskStartPrompts();
  if (prompts.error) {
    taskStartError(prompts.error);
    return;
  }
  document.querySelector("#task-start-dialog").close();
  startTask(prompts.notes);
});
document.querySelector("#task-start-back").addEventListener("click", () => {
  document.querySelector("#task-start-dialog").close();
});
document.querySelector("#task-cancel").addEventListener("click", () => {
  document.querySelector("#task-dialog").close();
});
document.querySelector("#task-save").addEventListener("click", () => {
  saveTask();
});
document.querySelector("#task-delete").addEventListener("click", () => {
  deleteTask();
});
function openGroup() {
  document.querySelector("#group-name").value = "";
  document.querySelector("#group-description").value = "";
  const error = document.querySelector("#group-error");
  error.hidden = true;
  error.textContent = "";
  const settings = jiraSettings();
  const note = document.querySelector("#group-note");
  const go = document.querySelector("#group-go");
  if (!settings.enabled) {
    note.textContent = "Jira is off, so this group is saved without a Jira epic.";
    go.disabled = false;
  } else if (!settings.boardId) {
    note.textContent = "Choose a Jira board in Settings before creating a group.";
    go.disabled = true;
  } else {
    note.textContent = "Creating this group creates a Jira epic on " + boardLabel(settings.boardName, settings.boardId) + ".";
    go.disabled = false;
  }
  document.querySelector("#group-dialog").showModal();
  if (!go.disabled) document.querySelector("#group-name").focus();
}

function rememberGroup(group) {
  state.data.groups = [group, ...((state.data.groups || []).filter((item) => item.id !== group.id))];
  renderTasks();
}

async function saveGroup() {
  const name = document.querySelector("#group-name").value.trim();
  const description = document.querySelector("#group-description").value.trim();
  const error = document.querySelector("#group-error");
  error.hidden = true;
  if (!name) {
    error.hidden = false;
    error.textContent = "Enter a group name.";
    return;
  }
  const button = document.querySelector("#group-go");
  button.disabled = true;
  try {
    const data = await api("/api/groups", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, description }),
    });
    rememberGroup(data.group);
    document.querySelector("#group-dialog").close();
    banner("Created " + data.group.name + (data.group.epicKey ? ". Jira epic " + data.group.epicKey + "." : "."));
  } catch (err) {
    error.hidden = false;
    error.textContent = err.message;
    button.disabled = false;
  }
}

document.querySelector("#group-form").addEventListener("submit", (event) => {
  event.preventDefault();
  saveGroup();
});
document.querySelector("#group-cancel").addEventListener("click", () => {
  document.querySelector("#group-dialog").close();
});
document.querySelector("#group-delete-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  confirmDeleteGroup();
});
document.querySelector("#panel-tasks").addEventListener("click", (event) => {
  if (taskSuppressClick) return;
  const view = event.target.closest("[data-task-view]");
  if (view) {
    state.taskView = view.dataset.taskView;
    renderTasks();
    return;
  }
  const remove = event.target.closest("[data-delete-group]");
  if (remove) {
    openDeleteGroup(remove.dataset.deleteGroup);
    return;
  }
  if (event.target.closest("#task-done-reset")) {
    state.taskDoneFrom = "";
    state.taskDoneTo = "";
    renderTasks();
    return;
  }
  if (event.target.closest("#group-new")) {
    openGroup();
    return;
  }
  if (event.target.closest("#task-new")) {
    openTask("");
    return;
  }
  const card = event.target.closest("[data-task]");
  if (card) openTask(card.dataset.task);
});

paintGenerateIcons();

api("/api/state")
  .then((data) => {
    render(data);
    refreshAssignedRepoIndexes();
  })
  .catch((error) => {
    const tasks = document.querySelector("#panel-tasks");
    if (tasks) tasks.replaceChildren();
    document.querySelector("#panel-agents").replaceChildren();
    banner(error.message, true);
  });
