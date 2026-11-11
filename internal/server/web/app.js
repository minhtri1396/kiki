const state = { data: null, agentId: null, choices: {} };

const repoLabel = { backend: "Backend", website: "Website", mobile: "Mobile", document: "Document" };

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

let bannerTimer = 0;

function banner(message, isError) {
  const node = document.querySelector("#banner");
  clearTimeout(bannerTimer);
  node.classList.remove("show");
  if (!message) {
    node.hidden = true;
    node.classList.remove("error");
    return;
  }
  node.hidden = false;
  node.classList.toggle("error", Boolean(isError));
  node.querySelector("#banner-text").textContent = message;
  void node.offsetWidth;
  node.classList.add("show");
  bannerTimer = setTimeout(() => {
    node.hidden = true;
    node.classList.remove("show");
  }, 3000);
}

async function api(url, options) {
  const res = await fetch(url, options);
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
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

function render(data) {
  state.data = data;
  seedChoices(data);
  document.querySelector("#root-path").textContent = data.root;
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
    section.append(el("h2", "", group.name));
    const cards = el("div", "cards");
    for (const agent of group.agents) {
      const card = el("article", "card agent-card");
      const head = el("div", "card-head");
      head.append(el("h3", "", agent.title));
      const prompt = el("button", "icon-button");
      prompt.type = "button";
      prompt.dataset.prompt = agent.id;
      prompt.setAttribute("aria-label", "Show the prompt for " + agent.title);
      prompt.append(promptIcon());
      head.append(prompt);
      card.append(head);
      card.append(el("p", "id", "ID: " + agent.id));
      const body = el("div", "card-copy");
      body.append(el("p", "", agent.summary));
      const repos = agent.repos.map((kind) => repoLabel[kind] || kind).join(", ");
      body.append(el("p", "hint", "Repo: " + repos));
      if (agent.peer) body.append(el("p", "hint", "Works with " + agent.peer));
      card.append(body);
      card.append(agentPicks(agent.id));
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
  panel.append(el("p", "hint", "Each repository is a folder inside " + data.root + ". Backend, website, and mobile also have service names. Document is the docs folder."));

  const grid = el("div", "repo-grid");
  for (const repo of data.repos) {
    const card = el("article", "card");
    const head = el("div", "repo-head");
    head.append(el("h3", "", repoLabel[repo.kind]));
    head.append(el("p", "repo-path", repo.folder || ""));
    card.append(head);
    if (repo.folder && !repo.exists) card.append(el("p", "hint", "Folder is missing."));
    if (repo.empty) {
      card.append(el("p", "hint", "This folder is empty."));
      const clone = el("button", "", "Clone repository");
      clone.type = "button";
      clone.dataset.clone = repo.kind;
      card.append(clone);
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
      card.append(services);
    }
    grid.append(card);
  }
  panel.append(grid);
  panel.append(saveButton());
}

function renderConfig(data) {
  document.querySelector("#settings-hint").textContent =
    "Workspace and Default AI Tool are saved to " + data.setupFile + ", in the app folder.";
  const settings = el("div", "setup-row");
  settings.append(field("Workspace", "text", "workspace-id", data.workspaceId));
  const runtime = el("label", "field");
  runtime.append(el("span", "", "Default AI Tool"));
  const select = el("select");
  select.id = "orca-agent";
  for (const name of data.orcaAgents) {
    const option = el("option", "", name);
    option.value = name;
    if (name === data.orcaAgent) option.selected = true;
    select.append(option);
  }
  runtime.append(select);
  settings.append(runtime);
  document.querySelector("#settings-fields").replaceChildren(settings);
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
  if (!workspace || !agent) return false;
  return workspace.value !== (state.data.workspaceId || "") || agent.value !== state.data.orcaAgent;
}

function syncSave() {
  const repos = document.querySelector("#panel-repos [data-save]");
  const config = document.querySelector("#settings-save");
  if (repos) repos.parentElement.hidden = !reposDirty();
  if (config) config.hidden = !configDirty();
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

function renderIndexes(data) {
  const panel = document.querySelector("#panel-indexes");
  let cards = panel.querySelector("#index-cards");
  let jobPanel = panel.querySelector("#job-panel");
  if (!cards) {
    panel.replaceChildren();
    panel.append(el("p", "hint", "Agents do not create these indexes themselves. The buttons below run the commands from the agent files."));
    cards = el("div", "index-grid");
    cards.id = "index-cards";
    jobPanel = el("section", "job-panel");
    jobPanel.id = "job-panel";
    panel.append(cards, jobPanel);
  }
  cards.replaceChildren();
  cards.append(indexCard(data.indexes.gitnexus, "GitNexus", "Set up GitNexus", data.root));
  for (const item of data.indexes.codegraph) {
    cards.append(indexCard(item, "CodeGraph " + repoLabel[item.kind], "Set up CodeGraph", data.root));
  }
  if (!jobPanel.dataset.ready) {
    const meta = el("p", "hint", "");
    meta.id = "job-meta";
    jobPanel.append(el("h2", "", "Command log"));
    jobPanel.append(meta);
    const log = el("pre", "", "No command has been run.");
    log.id = "job-log";
    jobPanel.append(log);
    const actions = el("div", "actions");
    const cancel = el("button", "", "Cancel");
    cancel.type = "button";
    cancel.id = "job-cancel";
    cancel.hidden = true;
    actions.append(cancel);
    jobPanel.append(actions);
    jobPanel.dataset.ready = "1";
  }
}

function indexCard(item, title, action, root) {
  const card = el("article", "card");
  const head = el("div", "repo-head");
  head.append(el("h3", "", title));
  head.append(el("span", "badge " + badgeClass(item), badgeText(item)));
  card.append(head);
  const command = el("p", "command", item.command);
  card.append(command);
  if (item.kind === "gitnexus") card.append(el("p", "hint", "Repository: " + root));
  if (item.summary && !item.summary.startsWith("Repository:")) card.append(el("p", "hint", item.summary));
  const actions = el("div", "actions");
  const check = el("button", "", "Check");
  check.type = "button";
  check.dataset.check = "1";
  const setup = el("button", "primary", action);
  setup.type = "button";
  if (item.kind === "gitnexus") setup.dataset.setup = "gitnexus";
  else setup.dataset.setup = "codegraph:" + item.kind;
  actions.append(check, setup);
  card.append(actions);
  return card;
}

function badgeClass(item) {
  if (!item.checked) return item.dirExists ? "wait" : "bad";
  return item.ready ? "ok" : "bad";
}

function badgeText(item) {
  if (!item.checked) return item.dirExists ? "Index folder exists" : "Not set up";
  return item.ready ? "Ready" : "Not ready";
}

function showTab(name) {
  state.tab = name;
  for (const button of document.querySelectorAll("[data-tab]")) {
    button.setAttribute("aria-selected", button.dataset.tab === name ? "true" : "false");
  }
  document.querySelector("#panel-agents").hidden = name !== "agents";
  document.querySelector("#panel-repos").hidden = name !== "repos";
  document.querySelector("#panel-indexes").hidden = name !== "indexes";
}

function collect() {
  return {
    workspaceId: document.querySelector("#workspace-id").value,
    orcaAgent: document.querySelector("#orca-agent").value,
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

function seedChoices(data) {
  for (const item of data.agentModels || []) {
    if (!state.choices[item.id]) state.choices[item.id] = { tool: item.tool, model: item.model };
  }
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

let modelsTimer = 0;

function rememberChoice(id, tool, model) {
  state.choices[id] = { tool, model };
  clearTimeout(modelsTimer);
  modelsTimer = setTimeout(() => {
    const agents = Object.entries(state.choices).map(([agentId, choice]) => ({
      id: agentId,
      tool: choice.tool,
      model: choice.model,
    }));
    api("/api/models", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ agents }),
    })
      .then(() => banner("Saved models.yaml."))
      .catch((error) => banner(error.message, true));
  }, 250);
}

function choiceFor(agentId) {
  const saved = state.choices[agentId];
  const tool = saved && saved.tool ? saved.tool : state.data.orcaAgent;
  let model = saved && saved.tool === tool ? saved.model : "";
  if (!modelsFor(tool).some((item) => item.id === model)) model = defaultModel(tool);
  return { tool, model };
}

function fillModels(select, tool, model) {
  select.replaceChildren();
  for (const item of modelsFor(tool)) {
    const option = el("option", "", item.label);
    option.value = item.id;
    if (item.id === model) option.selected = true;
    select.append(option);
  }
}

function agentPicks(agentId) {
  const choice = choiceFor(agentId);
  const picks = el("div", "agent-picks");
  const toolField = el("label", "field");
  toolField.append(el("span", "", "AI Tool"));
  const toolSelect = el("select");
  toolSelect.dataset.tool = agentId;
  for (const name of state.data.orcaAgents) {
    const option = el("option", "", name);
    option.value = name;
    if (name === choice.tool) option.selected = true;
    toolSelect.append(option);
  }
  toolField.append(toolSelect);
  const modelField = el("label", "field");
  modelField.append(el("span", "", "Model"));
  const modelSelect = el("select");
  modelSelect.dataset.model = agentId;
  fillModels(modelSelect, choice.tool, choice.model);
  modelField.append(modelSelect);
  picks.append(toolField, modelField);
  return picks;
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
    agent.id + " · " + choice.tool + " · " + (model ? model.label : choice.model);
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
    node.textContent = "This repo has no folder. Set it on the Repos tab before starting.";
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
  document.querySelector("#prompt-reader-title").textContent = agent.title;
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

let timer = 0;

function watchJob(id) {
  if (timer) clearInterval(timer);
  const tick = async () => {
    try {
      const job = await api("/api/jobs/" + id);
      showJob(job);
      if (job.status !== "running") {
        clearInterval(timer);
        if (job.status === "ok") {
          banner("Checking the indexes again…");
          const data = await api("/api/indexes/check", { method: "POST" });
          state.data = data;
          renderIndexes(data);
          banner("Setup finished.");
        } else {
          banner("Setup failed.", true);
        }
      }
    } catch (error) {
      banner(error.message, true);
      clearInterval(timer);
    }
  };
  tick();
  timer = setInterval(tick, 1000);
}

function showJob(job) {
  const meta = document.querySelector("#job-meta");
  const log = document.querySelector("#job-log");
  const cancel = document.querySelector("#job-cancel");
  if (!meta) return;
  meta.textContent = job.status + " · " + job.command;
  log.textContent = job.log || "(no output yet)";
  log.scrollTop = log.scrollHeight;
  cancel.hidden = job.status !== "running";
  cancel.dataset.job = job.id;
}

async function startSetup(which) {
  const card = state.data.indexes.gitnexus.kind === which || which === "gitnexus"
    ? state.data.indexes.gitnexus
    : state.data.indexes.codegraph.find((item) => "codegraph:" + item.kind === which);
  const command = card ? card.command : which;
  if (!window.confirm("Run this command?\n\n" + command)) return;
  showTab("indexes");
  try {
    const path = which === "gitnexus" ? "/api/indexes/gitnexus" : "/api/indexes/codegraph/" + which.split(":")[1];
    const job = await api(path, { method: "POST" });
    showJob(job);
    watchJob(job.id);
  } catch (error) {
    banner(error.message, true);
  }
}

document.body.addEventListener("click", (event) => {
  const tab = event.target.closest("[data-tab]");
  if (tab) {
    showTab(tab.dataset.tab);
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
  if (event.target.closest("[data-check]")) {
    banner("Checking…");
    api("/api/indexes/check", { method: "POST" })
      .then((data) => {
        state.data = data;
        renderIndexes(data);
        banner("Indexes checked.");
      })
      .catch((error) => banner(error.message, true));
    return;
  }
  const setup = event.target.closest("[data-setup]");
  if (setup) startSetup(setup.dataset.setup);
  const cancel = event.target.closest("#job-cancel");
  if (cancel && cancel.dataset.job) {
    api("/api/jobs/" + cancel.dataset.job + "/cancel", { method: "POST" }).catch((error) => banner(error.message, true));
  }
});

document.body.addEventListener("input", (event) => {
  if (event.target.id === "workspace-id") syncSave();
  if (event.target.id === "feature-name") refreshFeatureFolder();
});

document.body.addEventListener("change", (event) => {
  if (event.target.id === "orca-agent") syncSave();
  const tool = event.target.closest("[data-tool]");
  if (tool) {
    const model = document.querySelector(`[data-model="${CSS.escape(tool.dataset.tool)}"]`);
    const next = defaultModel(tool.value);
    fillModels(model, tool.value, next);
    rememberChoice(tool.dataset.tool, tool.value, model.value);
    return;
  }
  const model = event.target.closest("[data-model]");
  if (model) {
    const picked = document.querySelector(`[data-tool="${CSS.escape(model.dataset.model)}"]`);
    rememberChoice(model.dataset.model, picked.value, model.value);
    return;
  }
  const featureTool = event.target.closest("[data-feature-tool]");
  if (featureTool) {
    const next = document.querySelector(`[data-feature-model="${CSS.escape(featureTool.dataset.featureTool)}"]`);
    fillModels(next, featureTool.value, defaultModel(featureTool.value));
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
  const error = document.querySelector("#clone-error");
  const go = document.querySelector("#clone-go");
  error.hidden = true;
  go.disabled = true;
  go.textContent = "Cloning…";
  try {
    const data = await api("/api/repos/" + encodeURIComponent(kind) + "/clone", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url: input.value.trim() }),
    });
    dialog.close();
    render(data);
    const repo = data.repos.find((item) => item.kind === kind);
    banner("Cloned the repository into " + (repo ? repo.folder : "the folder") + ".");
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
  for (const name of state.data.orcaAgents) {
    const option = el("option", "", name);
    option.value = name;
    if (name === choice.tool) option.selected = true;
    toolSelect.append(option);
  }
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
    const section = el("section", "feature-group");
    section.append(el("h3", "", group.name));
    for (const agent of group.agents) {
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

document.querySelector("#start-feature").addEventListener("click", openFeature);
document.querySelector("#open-settings").addEventListener("click", () => {
  document.querySelector("#settings-dialog").showModal();
});
document.querySelector("#settings-close").addEventListener("click", () => {
  document.querySelector("#settings-dialog").close();
});
document.querySelector("#feature-cancel").addEventListener("click", () => {
  document.querySelector("#feature-dialog").close();
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

api("/api/state")
  .then(render)
  .catch((error) => {
    document.querySelector("#panel-agents").replaceChildren();
    banner(error.message, true);
  });
