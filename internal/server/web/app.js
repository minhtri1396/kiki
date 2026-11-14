const state = { data: null, agentId: null, editingAgentId: "", busySetup: "", checkingIndexes: new Set(), tab: "agents", workflowSelection: null };
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
  renderIndexes(data);
  renderWorkflow(data);
  renderHistory(data);
  syncSave();
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
  body.append(el("p", "hint", "This app's agents sit beside your folder in the team folder, with an arrow each way. Choosing another member also shows that member beside this app. That comparison only copies the member's agents into this app. Choosing a member asks before replacing this app's agents with that member's agents."));
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
  body.append(renderCompare("user-compare", "This app and " + (team.user || "your folder")));
  const memberCompare = renderCompare("member-compare", "This app and a member", false);
  if (chosenMember(team) === String(team.user || "").trim()) memberCompare.hidden = true;
  body.append(memberCompare);
  return card;
}

function renderCompare(id, title, push) {
  const block = el("section", "team-compare-block");
  block.id = id;
  const head = el("div", "team-share-title");
  head.append(el("h3", "", title));
  const badge = el("span", "badge", "");
  badge.hidden = true;
  head.append(badge);
  block.append(head);
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
        body.append(el("p", "hint", "Teamwork: " + names.join(", ")));
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
  panel.append(el("p", "hint", "Each repository is a folder inside " + data.root + ". Backend, website, and mobile also have service names. Document is the docs folder. KIKI saves each repository in " + data.root + "/.kiki/repos."));

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
    head.append(el("p", "repo-path", repo.folder || ""));
    card.append(head);
    const indexLine = el("p", "hint", "Index: " + indexLabel(repo.index));
    indexLine.dataset.repoIndex = repo.kind;
    card.append(indexLine);
    if (repo.folder && !repo.exists) card.append(el("p", "hint", "Folder is missing."));
    if (repo.empty) card.append(el("p", "hint", "This folder is empty."));
    if (repo.sparse) card.append(el("p", "hint", "Checkout: " + repo.sparse + ". Other folders are not in this working tree."));
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
      card.append(services);
    }
    if (repo.empty) {
      const actions = el("div", "actions");
      const clone = el("button", "", "Clone repository");
      clone.type = "button";
      clone.dataset.clone = repo.kind;
      actions.append(clone);
      card.append(actions);
    }
    grid.append(card);
  }
  panel.append(grid);
  panel.append(saveButton());
}

function renderConfig(data) {
  document.querySelector("#settings-hint").textContent =
    "Press Save to write Workspace, Agent Orchestration, Generate text, and Team folder to " + data.setupFile + ". Close discards unsaved changes.";
  const settings = el("div", "setup-row");
  settings.append(field("Workspace", "text", "workspace-id", data.workspaceId));
  settings.append(renderOrchestration(data));
  settings.append(renderGenerate(data));
  settings.append(renderTeam(data.teamFolder || {}));
  document.querySelector("#settings-fields").replaceChildren(settings);
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
  section.append(el("p", "hint", "AI Tool and model used when Generate rewrites a prompt, a repository note, or a teamwork note in Edit agent. KIKI runs that tool's CLI."));
  const grid = el("div", "agent-grid");
  const toolField = el("label", "field");
  toolField.append(el("span", "", "AI Tool"));
  const tool = el("select");
  tool.id = "generate-tool";
  const toolName = usableTool(data.generateTool || data.orcaAgent);
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
    const nameError = el("p", "field-error", mode === "existing" ? "Choose a folder name." : "Enter your name.");
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
    const nameError = el("p", "field-error", "Enter your name.");
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
    || model.value !== model.dataset.saved;
}

function syncSave() {
  const repos = document.querySelector("#panel-repos [data-save]");
  const config = document.querySelector("#settings-save");
  if (repos) repos.parentElement.hidden = !reposDirty();
  if (config && !config.classList.contains("busy")) config.disabled = !configDirty();
}

let generateIconSeq = 0;

function generateIcon() {
  const id = "ai-spark-" + (++generateIconSeq);
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  const gradient = document.createElementNS("http://www.w3.org/2000/svg", "linearGradient");
  gradient.id = id;
  gradient.setAttribute("x1", "0");
  gradient.setAttribute("y1", "1");
  gradient.setAttribute("x2", "1");
  gradient.setAttribute("y2", "0");
  for (const [offset, color] of [["0%", "#f472b6"], ["34%", "#a78bfa"], ["66%", "#60a5fa"], ["100%", "#22d3ee"]]) {
    const stop = document.createElementNS("http://www.w3.org/2000/svg", "stop");
    stop.setAttribute("offset", offset);
    stop.setAttribute("stop-color", color);
    gradient.append(stop);
  }
  defs.append(gradient);
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("fill", "url(#" + id + ")");
  path.setAttribute("d", "M12 1.2c.7 4.7 2.2 6.4 7.4 7.3-5.2.9-6.7 2.6-7.4 7.3-.7-4.7-2.2-6.4-7.4-7.3 5.2-.9 6.7-2.6 7.4-7.3zm6.6 13.2c.28 1.7.85 2.35 2.7 2.7-1.85.35-2.42 1-2.7 2.7-.28-1.7-.85-2.35-2.7-2.7 1.85-.35 2.42-1 2.7-2.7z");
  svg.append(defs, path);
  return svg;
}

function generateButton(kind, subject) {
  const button = el("button", "icon-button field-generate");
  button.type = "button";
  button.dataset.generate = kind;
  button.setAttribute("aria-label", "Generate");
  if (subject) button.dataset.generateSubject = subject;
  button.append(generateIcon());
  return button;
}

let generateAbort = null;
let generatePreviewAbort = null;
let generatePending = null;
let generateDraft = null;
let generateSawThink = false;

function generateTitle(button) {
  const subject = button.dataset.generateSubject || "";
  if (button.dataset.generate === "prompt") return "Generate prompt";
  if (button.dataset.generate === "repo") return "Generate " + (subject || "repository");
  return "Generate " + (subject || "teamwork");
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
  const tool = data.generateTool || data.orcaAgent || "";
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

function openGenerateDialog(title) {
  document.querySelector("#generate-dialog-title").textContent = title;
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

function paintRepoIndexes() {
  if (!state.data) return;
  for (const repo of state.data.repos || []) {
    const node = document.querySelector(`[data-repo-index="${repo.kind}"]`);
    if (node) node.textContent = "Index: " + indexLabel(repo.index);
  }
}

function renderIndexes(data) {
  const panel = document.querySelector("#panel-indexes");
  let cards = panel.querySelector("#index-cards");
  if (!cards) {
    panel.replaceChildren();
    panel.append(el("p", "hint", "Set up Obsidian or CodeGraph on the repository you choose. KIKI saves that choice in .kiki/repos. The launch prompt tells the model how to query the index on that repository."));
    cards = el("div", "index-grid");
    cards.id = "index-cards";
    panel.append(cards);
  }
  cards.replaceChildren();
  cards.append(indexToolCard(data, "obsidian", "Obsidian", "Add Repository"));
  cards.append(indexToolCard(data, "codegraph", "CodeGraph", "Add Repository"));
  if (state.busySetup) markSetupBusy(state.busySetup, true);
  if (state.checkingIndexes.size) markIndexesChecking();
}

function indexToolCard(data, tool, title, action) {
  const card = el("article", "card");
  card.dataset.tool = tool;
  const assigned = (data.indexes || []).filter((item) => item.index === tool);
  const head = el("div", "repo-head index-tool-head");
  const titleBlock = el("div", "index-tool-title");
  titleBlock.append(el("h3", "", title));
  const link = document.createElement("a");
  link.className = "index-tool-link";
  link.href = indexToolURL[tool];
  link.dataset.url = indexToolURL[tool];
  link.textContent = indexToolURL[tool];
  titleBlock.append(link);
  head.append(titleBlock);
  const tools = el("div", "index-tool-actions");
  const setup = el("button", "primary", action);
  setup.type = "button";
  setup.dataset.setup = tool;
  tools.append(setup);
  const refresh = el("button", "icon-button");
  refresh.type = "button";
  refresh.dataset.refreshIndex = tool;
  refresh.setAttribute("aria-label", "Check " + title + " folders");
  refresh.disabled = assigned.length === 0;
  refresh.append(refreshIcon());
  tools.append(refresh);
  head.append(tools);
  card.append(head);
  if (!assigned.length) card.append(el("p", "hint", "No repository uses this index."));
  for (const item of assigned) card.append(indexRepoRow(item));
  return card;
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

function indexRepoRow(item) {
  const row = el("div", "index-repo");
  row.dataset.index = item.kind;
  const head = el("div", "repo-head");
  head.append(el("h3", "", repoLabel[item.kind] || item.kind));
  const status = el("div", "index-repo-status");
  status.append(el("span", "badge " + badgeClass(item), badgeText(item)));
  status.append(indexRemoveButton(item));
  head.append(status);
  row.append(head);
  if (item.folder) row.append(el("p", "repo-path", item.folder));
  if (item.command) row.append(el("p", "command", item.command));
  if (item.summary) row.append(el("p", "hint", item.summary));
  return row;
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
  panel.append(el("p", "hint", "Teamwork is how each agent is set to work with another. A repository line uses the note stored on that agent. The index is the one saved for the repository."));
  const legend = el("div", "workflow-legend");
  legend.append(workflowSwatch("teamwork", "Teamwork"), workflowSwatch("repository", "Repository"), workflowSwatch("index", "Index"));
  panel.append(legend);
  const scroll = el("div", "workflow-scroll");
  const stage = el("div", "workflow-stage");
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("workflow-lines");
  svg.setAttribute("aria-hidden", "true");
  const cols = el("div", "workflow-cols");
  cols.append(workflowAgentColumn(data), workflowRepoColumn(data), workflowIndexColumn(data));
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
  let group = "";
  for (const agent of agents) {
    if (agent.group !== group) {
      group = agent.group;
      col.append(el("p", "workflow-group", categoryLabel(group)));
    }
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
        kicker: "Teamwork",
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
  }
  return col;
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
    const button = el("button", "workflow-node");
    button.type = "button";
    button.dataset.flow = "repo";
    button.dataset.id = kind;
    button.setAttribute("aria-pressed", "false");
    button.append(el("span", "workflow-name", repoLabel[kind] || kind));
    button.append(workflowPath(repo && repo.folder));
    const remote = workflowRemote(repo && repo.git);
    if (remote) button.append(remote);
    col.append(button);
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
    const open = el("button", "workflow-open plain");
    open.type = "button";
    open.setAttribute("aria-pressed", "false");
    open.append(el("span", "workflow-name", id === "none" ? "Not set" : indexLabel(id)));
    card.append(open);
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
  const svgNS = "http://www.w3.org/2000/svg";
  const pairCount = {};
  svg.replaceChildren(workflowMarkers());
  workflowEdges.forEach((edge, index) => {
    const from = stage.querySelector(`.workflow-node[data-flow="${edge.fromType}"][data-id="${CSS.escape(edge.fromId)}"]`);
    const to = stage.querySelector(`.workflow-node[data-flow="${edge.toType}"][data-id="${CSS.escape(edge.toId)}"]`);
    if (!from || !to) return;
    const start = workflowAnchor(stage, from, edge.fromSide);
    const end = workflowAnchor(stage, to, edge.toSide);
    let c1 = start.x + (end.x - start.x) * 0.45;
    let c2 = end.x - (end.x - start.x) * 0.45;
    if (edge.kind === "teamwork") {
      const pair = [edge.fromId, edge.toId].sort().join("\0");
      const n = pairCount[pair] || 0;
      pairCount[pair] = n + 1;
      const bend = -34 - n * 18;
      c1 = start.x + bend;
      c2 = end.x + bend;
    }
    const d = `M ${start.x} ${start.y} C ${c1} ${start.y}, ${c2} ${end.y}, ${end.x} ${end.y}`;
    const path = document.createElementNS(svgNS, "path");
    path.setAttribute("d", d);
    path.setAttribute("class", "workflow-edge " + edge.kind);
    path.dataset.edge = String(index);
    path.setAttribute("marker-end", "url(#workflow-arrow-" + edge.kind + ")");
    const hit = document.createElementNS(svgNS, "path");
    hit.setAttribute("d", d);
    hit.setAttribute("class", "workflow-hit");
    hit.dataset.edge = String(index);
    svg.append(path, hit);
  });
  paintWorkflowHighlight();
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
    ["teamwork", "var(--accent)"],
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
    return { nodes, edges };
  }
  if (sel.kind === "agent") {
    const key = "agent:" + sel.id;
    nodes.add(key);
    const repos = new Set();
    workflowEdges.forEach((edge, index) => {
      if (edge.kind === "teamwork" && (edge.fromKey === key || edge.toKey === key)) {
        edges.add(index);
        nodes.add(edge.fromKey);
        nodes.add(edge.toKey);
      }
      if (edge.kind === "repository" && edge.fromKey === key) {
        edges.add(index);
        nodes.add(edge.toKey);
        repos.add(edge.toKey);
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
  paintWorkflowHighlight();
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
    const on = related.edges.has(Number(path.dataset.edge));
    path.classList.toggle("is-selected", !!active && active.kind === "edge" && Number(path.dataset.edge) === active.id);
    path.classList.toggle("is-dim", !!active && !on);
  }
}

function paintWorkflowDetail() {
  const box = document.querySelector("#workflow-detail");
  if (!box) return;
  box.replaceChildren();
  const sel = state.workflowSelection;
  if (!sel) {
    box.append(el("p", "hint", "Select an agent, a repository, an index, or a line."));
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
  const teamwork = agentTeamwork(agent);
  const uses = agentRepositoryUses(agent);
  box.append(el("h4", "", "Teamwork"));
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
  if (event.target.closest("[data-prompt], .index-tool-link")) return;
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
  for (const button of document.querySelectorAll("[data-tab='repos'], [data-tab='indexes'], [data-tab='workflow'], #open-settings, #start-feature")) {
    button.disabled = false;
  }
}

function showTab(name) {
  if (!state.data && name !== "agents") return;
  const enterIndexes = name === "indexes" && state.tab !== "indexes";
  const enterWorkflow = name === "workflow" && state.tab !== "workflow";
  const enterAgents = name === "agents" && state.tab !== "agents";
  state.tab = name;
  for (const button of document.querySelectorAll("[data-tab]")) {
    button.setAttribute("aria-selected", button.dataset.tab === name ? "true" : "false");
  }
  document.querySelector("#panel-agents").hidden = name !== "agents";
  document.querySelector("#panel-repos").hidden = name !== "repos";
  document.querySelector("#panel-indexes").hidden = name !== "indexes";
  document.querySelector("#panel-workflow").hidden = name !== "workflow";
  if (enterIndexes) checkIndexes();
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
  return {
    workspaceId: document.querySelector("#workspace-id").value,
    orchestrator: document.querySelector("#orchestrator").value,
    orcaAgent: state.data ? state.data.orcaAgent : "cursor",
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
  if (tools.includes(state.data.orcaAgent)) return state.data.orcaAgent;
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
  const tool = usableTool(agent && agent.tool ? agent.tool : state.data.orcaAgent);
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
  note.textContent = mode === "existing" ? "Choose a folder name." : "Enter your name.";
  note.hidden = !missing;
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
  const choosingExisting = teamNameMode() === "existing";
  const teamUser = currentTeamUser();
  if (teamPath && !teamUser) {
    syncTeamName();
    const control = document.querySelector(choosingExisting ? "#team-member-name" : "#team-user");
    if (control) control.focus();
    banner(choosingExisting ? "Choose a folder name." : "Enter your name.", true);
    return;
  }
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
      state.busySetup = "";
      markSetupBusy(which, false);
      const failed = job.status !== "ok";
      try {
        const data = await api("/api/indexes/check", { method: "POST" });
        if (generation !== watchGeneration) return;
        state.data = data;
        renderIndexes(data);
        paintRepoIndexes();
      } catch (error) {
        if (generation !== watchGeneration) return;
        banner(error.message, true);
      }
      if (generation !== watchGeneration) return;
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

function markSetupBusy(which, busy, label) {
  if (busy) state.busySetup = which;
  else if (state.busySetup === which) state.busySetup = "";
  const button = document.querySelector(`[data-setup="${CSS.escape(which)}"]`);
  if (!button) return;
  if (!button.dataset.label) button.dataset.label = button.textContent;
  button.disabled = busy;
  button.classList.toggle("busy", busy);
  button.setAttribute("aria-busy", busy ? "true" : "false");
  button.textContent = busy ? (label || "Setting up…") : button.dataset.label;
}

const indexCheckGeneration = {};

function allIndexKeys() {
  return ((state.data && state.data.indexes) || [])
    .filter((item) => item.index === "obsidian" || item.index === "codegraph")
    .map((item) => item.kind);
}

function indexKeys(tool) {
  return ((state.data && state.data.indexes) || [])
    .filter((item) => item.index === tool)
    .map((item) => item.kind);
}

function markIndexesChecking() {
  const panel = document.querySelector("#panel-indexes");
  if (!panel) return;
  for (const card of panel.querySelectorAll("[data-index]")) {
    if (!state.checkingIndexes.has(card.dataset.index)) continue;
    for (const button of card.querySelectorAll("[data-clear-index]")) button.disabled = true;
    const badge = card.querySelector(".repo-head .badge");
    if (!badge) continue;
    badge.className = "badge checking";
    badge.replaceChildren();
    badge.setAttribute("aria-label", "Checking");
    const spin = el("span", "spinner");
    spin.setAttribute("aria-hidden", "true");
    badge.append(spin);
  }
  for (const button of panel.querySelectorAll("[data-refresh-index]")) {
    const keys = indexKeys(button.dataset.refreshIndex);
    const busy = keys.some((key) => state.checkingIndexes.has(key));
    button.disabled = busy || keys.length === 0;
    button.classList.toggle("spin", busy);
    button.setAttribute("aria-busy", busy ? "true" : "false");
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
    renderIndexes(state.data);
    if (options.announce) banner(keys.length === 1 ? "Index checked." : "Indexes checked.");
    return true;
  } catch (error) {
    if (!finishIndexCheck(keys, generation)) return false;
    if (state.data) renderIndexes(state.data);
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
    renderIndexes(state.data);
    if (options.announce) banner("Indexes checked.");
    return true;
  } catch (error) {
    if (keys.length && !finishIndexCheck(keys, generation)) return false;
    if (state.data) renderIndexes(state.data);
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

function refreshIndex(tool) {
  const keys = indexKeys(tool);
  if (!keys.length || keys.some((key) => state.checkingIndexes.has(key))) return Promise.resolve(false);
  return refreshIndexFolders(tool, keys);
}

async function refreshIndexFolders(tool, keys) {
  const generation = {};
  for (const key of keys) {
    indexCheckGeneration[key] = (indexCheckGeneration[key] || 0) + 1;
    generation[key] = indexCheckGeneration[key];
    state.checkingIndexes.add(key);
  }
  markIndexesChecking();
  try {
    const data = await api("/api/indexes/refresh/" + encodeURIComponent(tool), { method: "POST" });
    if (!finishIndexCheck(keys, generation)) return false;
    state.data = data;
    paintRepoIndexes();
    renderIndexes(state.data);
    const missing = (data.indexes || []).filter((item) => keys.includes(item.kind) && !item.dirExists);
    const folder = tool === "obsidian" ? ".obsidian" : ".codegraph";
    if (!missing.length) banner(indexLabel(tool) + " folders are still there.");
    else banner(missing.map((item) => (repoLabel[item.kind] || item.kind) + " is missing " + folder).join(". ") + ".", true);
    return true;
  } catch (error) {
    if (!finishIndexCheck(keys, generation)) return false;
    if (state.data) renderIndexes(state.data);
    banner(error.message, true);
    return false;
  }
}

function openIndexSetup(tool) {
  if (state.busySetup || !state.data) return;
  const dialog = document.querySelector("#index-dialog");
  dialog.dataset.tool = tool;
  document.querySelector("#index-title").textContent = tool === "obsidian" ? "Set up Obsidian" : "Set up CodeGraph";
  const select = document.querySelector("#index-repo");
  select.replaceChildren();
  for (const repo of state.data.repos || []) {
    const option = el("option", "", (repoLabel[repo.kind] || repo.kind) + " — " + indexLabel(repo.index));
    option.value = repo.kind;
    select.append(option);
  }
  syncIndexChoice();
  const error = document.querySelector("#index-error");
  error.hidden = true;
  error.textContent = "";
  dialog.showModal();
}

function syncIndexChoice() {
  const kind = document.querySelector("#index-repo").value;
  const repo = ((state.data && state.data.repos) || []).find((item) => item.kind === kind);
  const path = document.querySelector("#index-repo-path");
  if (!repo) {
    path.textContent = "";
    return;
  }
  const folder = repo.folder || "Folder is not set";
  path.textContent = folder + ". Current index: " + indexLabel(repo.index) + ".";
}

async function confirmIndexSetup() {
  const dialog = document.querySelector("#index-dialog");
  const tool = dialog.dataset.tool;
  const kind = document.querySelector("#index-repo").value;
  const error = document.querySelector("#index-error");
  if (!kind) {
    error.hidden = false;
    error.textContent = "Choose a repository.";
    return;
  }
  error.hidden = true;
  dialog.close();
  await startSetup(tool, kind);
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
  const button = document.querySelector(`[data-clear-index="${CSS.escape(kind)}"]`);
  if (button) {
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
    renderIndexes(data);
    paintRepoIndexes();
    const label = repoLabel[kind] || kind;
    banner(deleteFolder ? "Removed the index from " + label + " and deleted its index folder." : "Removed the index from " + label + ".");
  } catch (error) {
    banner(error.message, true);
    if (button && button.isConnected) {
      button.disabled = false;
      button.classList.remove("busy");
    }
  }
}

async function startSetup(tool, kind) {
  if (state.busySetup || !kind) return;
  markSetupBusy(tool, true);
  showTab("indexes");
  try {
    const path = tool === "obsidian" ? "/api/indexes/obsidian" : "/api/indexes/codegraph";
    const job = await api(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ repo: kind }),
    });
    watchJob(job.id, tool);
  } catch (error) {
    markSetupBusy(tool, false);
    banner(error.message, true);
    try {
      const data = await api("/api/state");
      state.data = data;
      renderIndexes(data);
      paintRepoIndexes();
    } catch (reloadError) {
      banner(reloadError.message, true);
    }
  }
}

function repoList(value) {
  if (!Array.isArray(value)) {
    if (!value) return [];
    return [{ kind: value, description: "" }];
  }
  return value.map((item) => {
    if (typeof item === "string") return { kind: item, description: "" };
    return { kind: item && (item.kind || item.id), description: (item && item.description) || "" };
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
  for (const area of document.querySelectorAll("#agent-repo-notes textarea")) {
    state.repoNotes[area.dataset.repo] = area.value;
  }
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
    area.placeholder = "What this agent does in the " + kind + " repository. " + orchestratorLabel() + " receives this with that repository.";
    area.value = state.repoNotes[kind] || "";
    const compose = el("div", "compose");
    compose.append(area, generateButton("repo", kind));
    const preview = el("div", "markdown repo-preview");
    preview.hidden = true;
    block.append(compose, preview);
    root.append(block);
  }
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
    area.placeholder = "How this agent communicates and coordinates with " + id + ". " + orchestratorLabel() + " receives this with that agent id.";
    area.value = state.peerNotes[id] || "";
    const compose = el("div", "compose");
    compose.append(area, generateButton("teamwork", id));
    const preview = el("div", "markdown collab-preview");
    preview.hidden = true;
    block.append(compose, preview);
    root.append(block);
  }
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

function fillAgentRepos(selected) {
  const repos = document.querySelector("#agent-repos");
  repos.replaceChildren();
  document.querySelector("#agent-repo-notes").replaceChildren();
  const items = repoList(selected);
  state.repoNotes = {};
  for (const repo of items) state.repoNotes[repo.kind] = repo.description;
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
  document.querySelector("#agent-dialog-title").textContent = data.name || data.id;
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
  }));
  rememberCollaborations();
  const worksWith = [...document.querySelectorAll("#agent-works-with button[aria-pressed='true']")].map((button) => ({
    id: button.dataset.peer,
    collaboration: (state.peerNotes[button.dataset.peer] || "").trim(),
  }));
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
  const refresh = event.target.closest("[data-refresh-index]");
  if (refresh) {
    refreshIndex(refresh.dataset.refreshIndex);
    return;
  }
  const setup = event.target.closest("[data-setup]");
  if (setup) {
    openIndexSetup(setup.dataset.setup);
    return;
  }
  const clear = event.target.closest("[data-clear-index]");
  if (clear) {
    openRemoveIndex(clear.dataset.clearIndex);
  }
});

document.body.addEventListener("input", (event) => {
  if (event.target.id === "team-path" || event.target.id === "team-user") syncTeamName();
  if (event.target.id === "workspace-id" || event.target.id === "team-path" || event.target.id === "team-user") syncSave();
  if (event.target.id === "team-path") {
    if (!event.target.value.trim()) cancelTeamStage();
    refreshTeamNameChoices(event.target);
  }
  if (event.target.id === "feature-name") refreshFeatureFolder();
});

document.body.addEventListener("focusout", (event) => {
  if (event.target.id !== "team-path" || !event.target.isConnected) return;
  const path = event.target.value.trim();
  if (!path) return;
  if (path === teamListedPath) return;
  stageTeamPath(path).catch((error) => banner(error.message, true));
});

document.body.addEventListener("change", (event) => {
  if (event.target.name === "team-name-mode") {
    syncTeamName();
    syncSave();
    const control = document.querySelector(event.target.value === "existing" ? "#team-member-name" : "#team-user");
    if (control) control.focus();
    return;
  }
  if (event.target.id === "team-member-name") {
    syncTeamName();
    syncSave();
    return;
  }
  if (event.target.id === "agent-member") {
    chooseMember(event.target).catch((error) => banner(error.message, true));
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

document.querySelector("#index-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  confirmIndexSetup();
});

document.querySelector("#index-remove-form").addEventListener("submit", (event) => {
  if (event.submitter && event.submitter.value === "cancel") return;
  event.preventDefault();
  confirmRemoveIndex();
});

document.querySelector("#index-repo").addEventListener("change", syncIndexChoice);

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

const promptGenerateIcon = document.querySelector("#agent-prompt")?.closest(".compose")?.querySelector("[data-generate]");
if (promptGenerateIcon && !promptGenerateIcon.querySelector("svg")) promptGenerateIcon.append(generateIcon());

api("/api/state")
  .then(render)
  .catch((error) => {
    document.querySelector("#panel-agents").replaceChildren();
    banner(error.message, true);
  });
