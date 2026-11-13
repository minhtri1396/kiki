const state = { data: null, agentId: null, editingAgentId: "", busySetup: "", checkingIndexes: new Set() };
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
let teamStage = 0;

const repoLabel = { backend: "Backend", website: "Website", mobile: "Mobile", document: "Document" };
const indexToolURL = {
  obsidian: "https://obsidian.md/help/cli",
  codegraph: "https://github.com/colbymchenry/codegraph",
};

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
  renderHistory(data);
  syncSave();
}

function renderAgents(data) {
  const panel = document.querySelector("#panel-agents");
  panel.replaceChildren();
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
      if (agent.teamDrift) {
        const compare = el("button", "text-button warn", "Not Sync");
        compare.type = "button";
        compare.dataset.teamAgent = agent.id;
        compare.setAttribute("aria-label", "Not Sync. Compare " + agent.title + " with the team folder");
        actions.append(compare);
      }
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
    "Press Save to write Workspace, Default AI Tool, Generate text, and Team folder to " + data.setupFile + ". Close discards unsaved changes.";
  const settings = el("div", "setup-row");
  settings.append(field("Workspace", "text", "workspace-id", data.workspaceId));
  const runtime = el("label", "field");
  runtime.append(el("span", "", "Default AI Tool"));
  const select = el("select");
  select.id = "orca-agent";
  fillToolSelect(select, usableTool(data.orcaAgent));
  runtime.append(select);
  settings.append(runtime);
  settings.append(renderGenerate(data));
  settings.append(renderTeam(data.teamFolder || {}));
  document.querySelector("#settings-fields").replaceChildren(settings);
  const team = data.teamFolder || {};
  if (team.path && document.querySelector("#settings-dialog").open) loadTeamMembers(team.member, team.path);
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

function renderTeam(team) {
  const section = el("section", "team-setup wide");
  section.append(el("h3", "", "Team folder"));
  section.append(el("p", "hint", "Enter your name, then choose a folder on this computer or type its full path. Save copies this app's agents into your agents folder there. If it is already there, KIKI asks which way to copy. Member agents then stays on your folder."));
  const user = field("Your name", "text", "team-user", team.user || "");
  const userInput = user.querySelector("input");
  userInput.spellcheck = false;
  userInput.autocomplete = "off";
  userInput.placeholder = "Your folder name";
  section.append(user);
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
  if (team.path) {
    const member = el("label", "field");
    member.append(el("span", "", "Member agents"));
    const memberSelect = el("select");
    memberSelect.id = "team-member";
    memberSelect.disabled = true;
    const waiting = el("option", "", "Loading members…");
    waiting.value = "";
    memberSelect.append(waiting);
    member.append(memberSelect);
    section.append(member);
    const note = el("p", "hint", "Choose a member and press Save to replace the local agents folder with that member's agents folder. KIKI asks you to confirm before it overwrites .kiki/agents.");
    note.id = "team-member-note";
    section.append(note);
  }
  return section;
}

function mountTeam(team, members) {
  const typed = document.querySelector("#team-user");
  const next = renderTeam(Object.assign({}, team, { user: typed ? typed.value : (team.user || "") }));
  const current = document.querySelector(".team-setup");
  if (current) current.replaceWith(next);
  if (Array.isArray(members)) fillMemberSelect(members, team.member || "", team.path || "");
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
  const saved = (state.data && state.data.teamFolder) || {};
  const canonical = data.path || path;
  const member = canonical === (saved.path || "") ? (saved.member || "") : "";
  mountTeam({ path: canonical, member }, data.members || []);
  syncSave();
}

function hideTeamMembers() {
  teamStage += 1;
  document.querySelector("#team-member")?.closest(".field")?.remove();
  document.querySelector("#team-member-note")?.remove();
}

async function loadTeamMembers(selectedName, path) {
  const token = ++teamMemberLoad;
  const query = path ? "?path=" + encodeURIComponent(path) : "";
  try {
    const data = await api("/api/team/members" + query);
    if (token !== teamMemberLoad) return;
    fillMemberSelect(data.members || [], selectedName, data.path || path || "");
  } catch (error) {
    const note = document.querySelector("#team-member-note");
    if (note && token === teamMemberLoad) note.textContent = error.message;
  }
}

function fillMemberSelect(members, selectedName, path) {
  const select = document.querySelector("#team-member");
  if (!select) return;
  select.replaceChildren();
  const placeholder = el("option", "", "Choose a member");
  placeholder.value = "";
  select.append(placeholder);
  let found = false;
  for (const name of members) {
    const option = el("option", "", name);
    option.value = name;
    if (name === selectedName) {
      option.selected = true;
      found = true;
    }
    select.append(option);
  }
  select.disabled = false;
  select.dataset.membersPath = path || "";
  const note = document.querySelector("#team-member-note");
  if (note && !members.length) note.textContent = "The team folder has no member folders yet.";
  if (note && selectedName && !found) note.textContent = "The saved member folder is no longer in the team folder. Choose another.";
  syncSave();
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
  const agent = document.querySelector("#orca-agent");
  const tool = document.querySelector("#generate-tool");
  const model = document.querySelector("#generate-model");
  if (!workspace || !agent || !tool || !model) return false;
  const savedTeam = state.data.teamFolder || {};
  const teamPath = document.querySelector("#team-path");
  const teamUser = document.querySelector("#team-user");
  const member = document.querySelector("#team-member");
  const teamChanged = !!teamPath && teamPath.value.trim() !== (savedTeam.path || "");
  const userChanged = !!teamUser && teamUser.value.trim() !== (savedTeam.user || "");
  const memberChanged = !!member && !member.disabled && member.value !== (savedTeam.member || "");
  return teamChanged || userChanged || memberChanged
    || workspace.value !== (state.data.workspaceId || "")
    || agent.value !== state.data.orcaAgent
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

function showSideBySideDiff(lines) {
  if (!lines || !lines.length) {
    renderCompareRows([{ left: "No changes.", right: "No changes.", leftKind: "hunk", rightKind: "hunk" }]);
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
  renderCompareRows(rows);
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

function unlockNavigation() {
  for (const button of document.querySelectorAll("[data-tab='repos'], [data-tab='indexes'], #open-settings, #start-feature")) {
    button.disabled = false;
  }
}

function showTab(name) {
  if (!state.data && name !== "agents") return;
  const enterIndexes = name === "indexes" && state.tab !== "indexes";
  state.tab = name;
  for (const button of document.querySelectorAll("[data-tab]")) {
    button.setAttribute("aria-selected", button.dataset.tab === name ? "true" : "false");
  }
  document.querySelector("#panel-agents").hidden = name !== "agents";
  document.querySelector("#panel-repos").hidden = name !== "repos";
  document.querySelector("#panel-indexes").hidden = name !== "indexes";
  if (enterIndexes) checkIndexes();
}

function collect() {
  return {
    workspaceId: document.querySelector("#workspace-id").value,
    orcaAgent: document.querySelector("#orca-agent").value,
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
  document.querySelector("#launch-go").disabled = false;
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
  node.textContent = "Orca starts in " + repo.folder;
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
  document.querySelector("#prompt-reader-title").textContent = agent.title + " — prompt sent to Orca";
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
  document.querySelector("#prompt-reader-title").textContent = (agent ? agent.title : "Prompt") + " prompt";
  document.querySelector("#prompt-reader-body").innerHTML = state.promptHtml;
  document.querySelector("#prompt-reader").showModal();
}

async function launch() {
  const button = document.querySelector("#launch-go");
  button.disabled = true;
  const result = document.querySelector("#launch-result");
  result.hidden = false;
  result.classList.remove("error");
  result.textContent = "Starting on Orca…";
  try {
    const data = await api("/api/agents/" + state.agentId + "/launch", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: launchPayload(),
    });
    const launch = data.launch;
    result.classList.toggle("error", launch.status !== "ok");
    result.textContent = launch.status === "ok"
      ? "Started on Orca. Detail:\n" + launch.detail
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

async function saveSettings() {
  const button = document.querySelector("#settings-save");
  const teamPath = document.querySelector("#team-path").value.trim();
  const teamUser = document.querySelector("#team-user").value.trim();
  const savedTeam = (state.data && state.data.teamFolder) || {};
  const memberSelect = document.querySelector("#team-member");
  const membersPath = memberSelect ? (memberSelect.dataset.membersPath || "") : "";
  const member = memberSelect && !memberSelect.disabled ? memberSelect.value : "";
  const payload = collect();
  const selected = document.querySelector("[data-tab][aria-selected='true']");
  const tab = (selected && selected.dataset.tab) || "agents";
  const pathChanged = teamPath !== (savedTeam.path || "");
  const userChanged = teamUser !== (savedTeam.user || "");
  const copyingMember = !pathChanged && !userChanged && !!teamPath && !!member && membersPath === (savedTeam.path || "") && member !== (savedTeam.member || "");
  button.disabled = true;
  button.classList.add("busy");
  if (copyingMember) {
    const confirmed = await confirmMemberCopy(member);
    if (!confirmed) {
      button.classList.remove("busy");
      syncSave();
      return;
    }
  }
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
      if (teamPath && !teamUser) throw new Error("Enter your name.");
      const next = await putTeam({ path: teamPath, user: teamPath ? teamUser : "" });
      if (!next) throw new Error("Team folder was not saved.");
      data = next;
    } else {
      const currentMember = (data.teamFolder || {}).member || "";
      const sameFolder = membersPath && membersPath === ((data.teamFolder || {}).path || teamPath);
      if (teamPath && member && sameFolder && member !== currentMember) {
        const next = await postMember(member);
        if (!next) throw new Error("Member agents were not copied.");
        data = next;
      }
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
}

async function putTeam(body) {
  try {
    return await api("/api/team", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch (error) {
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

async function postMember(name, repos) {
  try {
    return await api("/api/team/member", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(repos ? { name, repos } : { name }),
    });
  } catch (error) {
    if (error.payload && Array.isArray(error.payload.unknown) && error.payload.unknown.length) {
      const choices = await askRepoChoices(error.payload.unknown, error.payload.repos || []);
      if (!choices) return null;
      return postMember(name, choices);
    }
    throw error;
  }
}

function confirmMemberCopy(name) {
  const dialog = document.querySelector("#team-member-dialog");
  document.querySelector("#team-member-text").textContent = "Replace .kiki/agents with " + name + "'s agents folder? This overwrites every agent file in this app.";
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
    area.placeholder = "What this agent does in the " + kind + " repository. Orca receives this with that repository.";
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
    area.placeholder = "How this agent communicates and coordinates with " + id + ". Orca receives this with that agent id.";
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

async function openTeamAgent(id) {
  const data = await api("/api/agents/" + encodeURIComponent(id) + "/team-diff");
  const dialog = document.querySelector("#team-agent-dialog");
  dialog.dataset.id = id;
  dialog.dataset.teamMissing = data.teamMissing ? "true" : "false";
  document.querySelector("#team-agent-title").textContent = data.title || id;
  const hint = document.querySelector("#team-agent-hint");
  hint.textContent = data.teamMissing
    ? "Your team folder does not have this agent. Use on This app copies it there. Use on Team folder removes it from this app."
    : "This app and your team folder have different copies of this agent. Use on a side replaces the other file.";
  const error = document.querySelector("#team-agent-error");
  error.hidden = true;
  error.textContent = "";
  if (data.diff && data.diff.length) showTeamDiff(data.diff);
  else renderCompareRows([{ left: "No changes.", right: "No changes.", leftKind: "hunk", rightKind: "hunk" }], document.querySelector("#team-agent-diff"), ["This app", "Team folder"], ["local", "team"]);
  dialog.showModal();
}

function showTeamDiff(lines) {
  const rows = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (line.kind === "hunk" || line.kind === "same") {
      rows.push({ left: line.text, right: line.text, leftKind: line.kind, rightKind: line.kind });
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
  renderCompareRows(rows, document.querySelector("#team-agent-diff"), ["This app", "Team folder"], ["local", "team"]);
}

function confirmTeamKeep(keep) {
  const source = document.querySelector("#team-agent-dialog");
  const missing = source.dataset.teamMissing === "true";
  const title = document.querySelector("#team-agent-title").textContent || "This agent";
  const dialog = document.querySelector("#team-keep-dialog");
  const heading = document.querySelector("#team-keep-title");
  const text = document.querySelector("#team-keep-text");
  const go = document.querySelector("#team-keep-go");
  if (keep === "local") {
    heading.textContent = missing ? "Copy agent" : "Overwrite agent";
    go.textContent = missing ? "Copy" : "Overwrite";
    go.className = missing ? "primary" : "danger";
    text.textContent = missing
      ? "Copy " + title + " from this app into your team folder. The file in .kiki/agents is written into your team folder."
      : "Overwrite " + title + " in your team folder with the copy in this app. The file in .kiki/agents replaces the same file in your team folder.";
  } else {
    heading.textContent = missing ? "Remove agent" : "Overwrite agent";
    go.textContent = missing ? "Remove" : "Overwrite";
    go.className = "danger";
    text.textContent = missing
      ? "Remove " + title + " from this app. Your team folder does not have this file, so the file in .kiki/agents is deleted."
      : "Overwrite " + title + " in this app with the copy in your team folder. The file in your team folder replaces the same file in .kiki/agents.";
  }
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
    go.onclick = () => {
      dialog.removeEventListener("close", onClose);
      dialog.close();
      finish(true);
    };
    dialog.showModal();
  });
}

async function keepTeamAgent(keep) {
  const dialog = document.querySelector("#team-agent-dialog");
  const id = dialog.dataset.id;
  if (!id) return;
  const buttons = dialog.querySelectorAll("button");
  buttons.forEach((button) => { button.disabled = true; });
  try {
    const fresh = await api("/api/agents/" + encodeURIComponent(id) + "/team-diff", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ keep }),
    });
    dialog.close();
    const selected = document.querySelector("[data-tab][aria-selected='true']");
    render(fresh);
    showTab((selected && selected.dataset.tab) || "agents");
    const removed = keep === "team" && dialog.dataset.teamMissing === "true";
    banner(removed ? "Removed this agent from the app." : keep === "local" ? "Copied this app's agent to the team folder." : "Copied the team folder's agent into this app.");
  } catch (error) {
    const node = document.querySelector("#team-agent-error");
    node.hidden = false;
    node.textContent = error.message;
  } finally {
    buttons.forEach((button) => { button.disabled = false; });
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
  const teamAgent = event.target.closest("[data-team-agent]");
  if (teamAgent) {
    openTeamAgent(teamAgent.dataset.teamAgent).catch((error) => banner(error.message, true));
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
  if (event.target.id === "workspace-id" || event.target.id === "team-path" || event.target.id === "team-user") syncSave();
  if (event.target.id === "team-path" && !event.target.value.trim()) hideTeamMembers();
  if (event.target.id === "feature-name") refreshFeatureFolder();
});

document.body.addEventListener("focusout", (event) => {
  if (event.target.id !== "team-path") return;
  const path = event.target.value.trim();
  if (!path) return;
  const listed = document.querySelector("#team-member")?.dataset.membersPath || "";
  if (path === listed) return;
  stageTeamPath(path).catch((error) => banner(error.message, true));
});

document.body.addEventListener("change", (event) => {
  if (event.target.id === "orca-agent") syncSave();
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
  const go = document.querySelector("#feature-go");
  go.disabled = false;
  go.textContent = "Start";
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
  document.querySelector("#prompt-reader-title").textContent = agent.title;
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
  const lines = [];
  let failed = 0;
  for (const box of checked) {
    const id = box.dataset.featureAgent;
    const agent = agentById(id);
    const tool = document.querySelector(`[data-feature-tool="${CSS.escape(id)}"]`).value;
    const model = document.querySelector(`[data-feature-model="${CSS.escape(id)}"]`).value;
    result.textContent = "Starting " + id + "…";
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
      if (data.launch.status === "ok") lines.push(id + ": started");
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
    banner("Started " + checked.length + (checked.length === 1 ? " agent" : " agents") + " for " + folder.slug + ".");
    return;
  }
  go.disabled = false;
  go.textContent = "Start";
}

document.querySelector("#settings-fields").addEventListener("click", (event) => {
  if (event.target.closest("#team-browse")) browseTeamFolder();
});
document.querySelector("#settings-fields").addEventListener("change", (event) => {
  if (event.target.id === "team-member") syncSave();
});
document.querySelector("#start-feature").addEventListener("click", openFeature);
document.querySelector("#open-settings").addEventListener("click", () => {
  if (!state.data) return;
  renderConfig(state.data);
  syncSave();
  document.querySelector("#settings-dialog").showModal();
  const team = (state.data && state.data.teamFolder) || {};
  if (team.path) loadTeamMembers(team.member, team.path);
});
document.querySelector("#settings-close").addEventListener("click", () => {
  document.querySelector("#settings-dialog").close();
});
document.querySelector("#settings-save").addEventListener("click", () => {
  saveSettings();
});
document.querySelector("#settings-dialog").addEventListener("close", () => {
  teamStage += 1;
  teamMemberLoad += 1;
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

document.querySelector("#team-agent-cancel").addEventListener("click", () => {
  document.querySelector("#team-agent-dialog").close();
});
document.querySelector("#team-agent-dialog").addEventListener("click", (event) => {
  const button = event.target.closest("[data-keep]");
  if (!button) return;
  confirmTeamKeep(button.dataset.keep).then((ok) => {
    if (!ok) return;
    return keepTeamAgent(button.dataset.keep);
  }).catch((error) => banner(error.message, true));
});
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

document.querySelector("#panel-agents").addEventListener("click", (event) => {
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
