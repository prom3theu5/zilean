/* ---------------------------------------------------------------- *
 * Zilean admin portal — vanilla JS.                                 *
 *                                                                   *
 * No framework, no build step. Talks to /admin/api/* with the       *
 * operator's API key held in localStorage.                          *
 * ---------------------------------------------------------------- */

"use strict";

const KEY_STORAGE = "zilean:apikey";
const API = "api";

const state = {
  apiKey: localStorage.getItem(KEY_STORAGE) || "",
  tab: "stats",
  torrents: {
    page: 1,
    perPage: 50,
    search: "",
    total: 0,
  },
};

// --- dom helpers ----------------------------------------------------
const $ = (s, root = document) => root.querySelector(s);
const $$ = (s, root = document) => [...root.querySelectorAll(s)];

function setStatus(text, tone) {
  const el = $("#status");
  el.textContent = text;
  el.style.color = tone === "error" ? "var(--danger)" :
                   tone === "ok"    ? "var(--success)" :
                                      "var(--muted)";
}

// --- auth -----------------------------------------------------------

async function fetchJson(path, options = {}) {
  const headers = new Headers(options.headers || {});
  headers.set("X-API-KEY", state.apiKey);
  if (options.body && !(options.body instanceof FormData)) {
    headers.set("content-type", "application/json");
  }
  const response = await fetch(path, { ...options, headers });
  if (response.status === 401) {
    signOut("Invalid or missing API key.");
    throw new Error("unauthorized");
  }
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`HTTP ${response.status}: ${text}`);
  }
  const ct = response.headers.get("content-type") || "";
  if (ct.includes("application/json")) return response.json();
  return response.text();
}

function showApp() {
  $("#login-view").hidden = true;
  $("#app-view").hidden = false;
  $("#logout-btn").hidden = false;
  switchTab(state.tab);
}

function showLogin() {
  $("#login-view").hidden = false;
  $("#app-view").hidden = true;
  $("#logout-btn").hidden = true;
  setStatus("");
}

function signOut(reason) {
  state.apiKey = "";
  localStorage.removeItem(KEY_STORAGE);
  if (reason) {
    const err = $("#login-error");
    err.textContent = reason;
    err.hidden = false;
  }
  showLogin();
}

// --- tab wiring -----------------------------------------------------

function switchTab(name) {
  state.tab = name;
  $$(".tab").forEach(t => t.classList.toggle("active", t.dataset.tab === name));
  $$('[data-panel]').forEach(p => p.hidden = p.dataset.panel !== name);
  const loader = TAB_LOADERS[name];
  if (loader) loader().catch(err => setStatus(err.message, "error"));
}

// --- formatters -----------------------------------------------------

function humanBytes(n) {
  if (n == null || Number.isNaN(n)) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0; let v = Number(n);
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(v < 10 && i > 0 ? 1 : 0)} ${units[i]}`;
}

function pill(label, yes) {
  return yes ? `<span class="pill yes">${label}</span>` : "";
}

function truncate(str, max = 80) {
  if (!str) return "";
  return str.length <= max ? str : `${str.slice(0, max)}…`;
}

function fmtDate(iso) {
  if (!iso) return "—";
  try {
    const d = new Date(iso);
    return d.toLocaleString();
  } catch { return iso; }
}

// --- stats tab -----------------------------------------------------

async function loadStats() {
  setStatus("Loading stats…");
  const data = await fetchJson(`${API}/stats`);
  const cards = $("#stats-cards");
  cards.innerHTML = `
    <article class="card"><h3>Torrents</h3><div class="value">${data.torrents.toLocaleString()}</div></article>
    <article class="card"><h3>IMDb titles</h3><div class="value">${data.imdb_files.toLocaleString()}</div></article>
    <article class="card"><h3>Parsed DMM pages</h3><div class="value">${data.parsed_pages.toLocaleString()}</div></article>
    <article class="card"><h3>Blacklisted</h3><div class="value">${data.blacklisted.toLocaleString()}</div></article>
    <article class="card"><h3>Last DMM import</h3><p>${fmtDate(data.last_dmm_import)}</p></article>
    <article class="card"><h3>Last IMDb import</h3>
      <p>${fmtDate(data.last_imdb_import && data.last_imdb_import.occured_at)}</p>
      ${data.last_imdb_import ? `<p class="muted">${Number(data.last_imdb_import.entry_count || 0).toLocaleString()} entries, status ${data.last_imdb_import.status || "?"}</p>` : ""}
    </article>
  `;
  setStatus("Ready", "ok");
}

// --- torrents tab --------------------------------------------------

async function loadTorrents() {
  setStatus("Loading torrents…");
  const { page, perPage, search } = state.torrents;
  const qs = new URLSearchParams({ page, per_page: perPage, search });
  const data = await fetchJson(`${API}/torrents?${qs}`);
  state.torrents.total = data.total;
  $("#torrents-count").textContent = `${data.total.toLocaleString()} torrent(s)`;
  $("#torrents-page").textContent = `Page ${page} / ${Math.max(1, Math.ceil(data.total / perPage))}`;
  $("#torrents-prev").disabled = page <= 1;
  $("#torrents-next").disabled = page * perPage >= data.total;

  const body = $("#torrents-body");
  body.innerHTML = data.items.map(t => `
    <tr data-hash="${t.info_hash}">
      <td><code>${t.info_hash}</code></td>
      <td>${t.category || ""}</td>
      <td title="${escapeAttr(t.raw_title || "")}">${escapeHtml(truncate(t.raw_title, 60))}</td>
      <td title="${escapeAttr(t.parsed_title || "")}">${escapeHtml(truncate(t.parsed_title, 60))}</td>
      <td>${t.imdb_id || ""}</td>
      <td>${t.year || ""}</td>
      <td>${humanBytes(t.size)}</td>
      <td>${pill("adult", t.adult)}</td>
      <td>${pill("trash", t.trash)}</td>
      <td class="actions">
        <button type="button" data-action="edit">Edit</button>
        <button type="button" data-action="blacklist">Blacklist</button>
        <button type="button" class="danger" data-action="delete">Delete</button>
      </td>
    </tr>
  `).join("");
  setStatus("Ready", "ok");
}

function escapeHtml(str) {
  return (str || "").replace(/[&<>"']/g, c =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}
function escapeAttr(str) { return escapeHtml(str); }

async function onTorrentAction(event) {
  const btn = event.target.closest("button[data-action]");
  if (!btn) return;
  const row = btn.closest("tr[data-hash]");
  const hash = row && row.dataset.hash;
  if (!hash) return;

  if (btn.dataset.action === "delete") {
    if (!confirm(`Delete torrent ${hash}? This does not blacklist it.`)) return;
    try {
      await fetchJson(`${API}/torrents/${hash}`, { method: "DELETE" });
      await loadTorrents();
    } catch (err) { setStatus(err.message, "error"); }
  } else if (btn.dataset.action === "blacklist") {
    const reason = prompt("Blacklist reason", "manual admin blacklist");
    if (!reason) return;
    try {
      await fetchJson(`${API}/torrents/${hash}/blacklist`, {
        method: "POST",
        body: JSON.stringify({ reason }),
      });
      await loadTorrents();
    } catch (err) { setStatus(err.message, "error"); }
  } else if (btn.dataset.action === "edit") {
    try {
      // Fetch the full row so the dialog sees every field — the table
      // only renders a display-sized subset.
      const data = await fetchJson(`${API}/torrents/${hash}`);
      openTorrentDialog("edit", data);
    } catch (err) { setStatus(err.message, "error"); }
  }
}

// --- torrent edit / create dialog ---------------------------------

const dialog = () => $("#torrent-dialog");
let dialogMode = "create";

function openTorrentDialog(mode, row) {
  dialogMode = mode;
  $("#dialog-title").textContent = mode === "create" ? "Add torrent" : "Edit torrent";

  const hashField = $("#dlg-info-hash");
  hashField.disabled = mode === "edit";
  hashField.value = mode === "edit" ? ((row && row.info_hash) || "") : "";
  $("#dlg-raw-title").value = mode === "edit" ? ((row && row.raw_title) || "") : "";

  // Size is stored as text in the DB; coerce to a number for the form.
  const rawSize = row && row.size ? Number.parseInt(row.size, 10) : NaN;
  $("#dlg-size").value = Number.isFinite(rawSize) ? rawSize : "";

  // Reset every override toggle to unchecked; seed the value inputs with
  // the row's current values so operators can tick a box without having
  // to re-type what's already there.
  $$('[data-override-toggle]').forEach((cb) => {
    cb.checked = false;
    const key = cb.dataset.overrideToggle;
    const input = $(`[data-override-value="${key}"]`);
    if (input) input.disabled = true;
  });

  $('[data-override-value="category"]').value = (row && row.category) || "movie";
  $('[data-override-value="year"]').value = (row && row.year) || "";
  $('[data-override-value="imdb_id"]').value = (row && row.imdb_id) || "";
  $('[data-override-value="adult"]').checked = !!(row && row.adult);
  $('[data-override-value="trash"]').checked = !!(row && row.trash);

  $("#dlg-error").hidden = true;
  dialog().showModal();
}

function closeTorrentDialog() {
  dialog().close();
}

async function onDialogSubmit(event) {
  event.preventDefault();
  const err = $("#dlg-error");
  err.hidden = true;

  const payload = {
    info_hash: $("#dlg-info-hash").value.trim().toLowerCase(),
    raw_title: $("#dlg-raw-title").value.trim(),
    size: String(Number.parseInt($("#dlg-size").value || "0", 10)),
  };

  // Client-side validation — server validates too.
  if (!/^[0-9a-f]{40}$/.test(payload.info_hash)) {
    err.textContent = "Info hash must be 40 hex characters.";
    err.hidden = false;
    return;
  }
  if (!payload.raw_title) {
    err.textContent = "Raw title is required.";
    err.hidden = false;
    return;
  }

  // Layer each ticked override on top. Unticked overrides are omitted so
  // the server keeps whatever parsett derives from the raw title.
  $$('[data-override-toggle]').forEach((cb) => {
    if (!cb.checked) return;
    const key = cb.dataset.overrideToggle;
    const el = $(`[data-override-value="${key}"]`);
    let value;
    if (el.type === "checkbox") value = !!el.checked;
    else if (el.type === "number") value = Number.parseInt(el.value || "0", 10);
    else value = el.value;
    payload[`${key}_override`] = value;
  });

  try {
    if (dialogMode === "create") {
      await fetchJson(`${API}/torrents`, {
        method: "POST",
        body: JSON.stringify(payload),
      });
    } else {
      const hash = payload.info_hash;
      const body = { ...payload };
      delete body.info_hash;
      await fetchJson(`${API}/torrents/${hash}`, {
        method: "PATCH",
        body: JSON.stringify(body),
      });
    }
    closeTorrentDialog();
    await loadTorrents();
    setStatus("Saved", "ok");
  } catch (e) {
    err.textContent = e.message;
    err.hidden = false;
  }
}

// --- blacklist tab -------------------------------------------------

async function loadBlacklist() {
  setStatus("Loading blacklist…");
  const items = await fetchJson(`${API}/blacklist`);
  $("#blacklist-count").textContent = `${items.length.toLocaleString()} item(s)`;
  $("#blacklist-body").innerHTML = items.map(b => `
    <tr data-hash="${b.info_hash}">
      <td><code>${b.info_hash}</code></td>
      <td>${escapeHtml(b.reason)}</td>
      <td>${fmtDate(b.blacklisted_at)}</td>
      <td class="actions">
        <button type="button" class="danger" data-action="unblacklist">Unblacklist</button>
      </td>
    </tr>
  `).join("");
  setStatus("Ready", "ok");
}

async function onBlacklistAction(event) {
  const btn = event.target.closest("button[data-action='unblacklist']");
  if (!btn) return;
  const row = btn.closest("tr[data-hash]");
  const hash = row && row.dataset.hash;
  if (!hash) return;
  if (!confirm(`Remove ${hash} from the blacklist?`)) return;
  try {
    await fetchJson(`${API}/blacklist/${hash}`, { method: "DELETE" });
    await loadBlacklist();
  } catch (err) { setStatus(err.message, "error"); }
}

// --- sync tab ------------------------------------------------------

function syncLog(line) {
  const pre = $("#sync-log");
  const ts = new Date().toLocaleTimeString();
  pre.textContent += `[${ts}] ${line}\n`;
  pre.scrollTop = pre.scrollHeight;
}

async function runSync(kind, extra = {}) {
  const btns = $$('[data-sync]');
  btns.forEach(b => b.disabled = true);
  try {
    syncLog(`${kind} sync requested…`);
    const response = await fetchJson(`${API}/sync/${kind}`, {
      method: "POST",
      body: JSON.stringify(extra),
    });
    syncLog(`${kind}: ${typeof response === "string" ? response : JSON.stringify(response)}`);
  } catch (err) {
    syncLog(`${kind} failed: ${err.message}`);
  } finally {
    btns.forEach(b => b.disabled = false);
  }
}

// --- boot ----------------------------------------------------------

const TAB_LOADERS = {
  stats: loadStats,
  torrents: loadTorrents,
  blacklist: loadBlacklist,
  sync: async () => {}, // sync tab is action-driven, nothing to load
};

function wireUp() {
  // Tab clicks.
  $$(".tab").forEach(t => t.addEventListener("click", () => switchTab(t.dataset.tab)));

  // Login form.
  $("#login-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    const key = $("#apikey").value.trim();
    if (!key) return;
    state.apiKey = key;
    try {
      // Probe /admin/api/stats to validate the key before persisting.
      await fetchJson(`${API}/stats`);
      localStorage.setItem(KEY_STORAGE, key);
      $("#login-error").hidden = true;
      showApp();
    } catch (err) {
      $("#login-error").textContent = `Sign-in failed: ${err.message}`;
      $("#login-error").hidden = false;
      state.apiKey = "";
    }
  });
  $("#logout-btn").addEventListener("click", () => signOut());

  // Torrents controls.
  $("#torrents-search").addEventListener("input", debounce(() => {
    state.torrents.search = $("#torrents-search").value.trim();
    state.torrents.page = 1;
    loadTorrents().catch(err => setStatus(err.message, "error"));
  }, 300));
  $("#torrents-per-page").addEventListener("change", (e) => {
    state.torrents.perPage = Number(e.target.value);
    state.torrents.page = 1;
    loadTorrents().catch(err => setStatus(err.message, "error"));
  });
  $("#torrents-prev").addEventListener("click", () => {
    if (state.torrents.page > 1) {
      state.torrents.page--;
      loadTorrents().catch(err => setStatus(err.message, "error"));
    }
  });
  $("#torrents-next").addEventListener("click", () => {
    state.torrents.page++;
    loadTorrents().catch(err => setStatus(err.message, "error"));
  });
  $("#torrents-body").addEventListener("click", onTorrentAction);

  // Add-torrent dialog: toolbar button, cancel, submit, and the per-
  // override toggles that enable/disable their value inputs.
  $("#torrents-add-btn").addEventListener("click", () => openTorrentDialog("create"));
  $("#dlg-cancel").addEventListener("click", closeTorrentDialog);
  $("#torrent-form").addEventListener("submit", onDialogSubmit);
  $$('[data-override-toggle]').forEach((cb) => {
    cb.addEventListener("change", () => {
      const key = cb.dataset.overrideToggle;
      const valueEl = $(`[data-override-value="${key}"]`);
      if (valueEl) valueEl.disabled = !cb.checked;
    });
  });
  // Native <dialog> closes on backdrop click if we wire it up explicitly.
  $("#torrent-dialog").addEventListener("click", (e) => {
    if (e.target === e.currentTarget) closeTorrentDialog();
  });

  // Blacklist.
  $("#blacklist-body").addEventListener("click", onBlacklistAction);

  // Sync triggers.
  $$('[data-sync]').forEach(b => b.addEventListener("click", () => {
    const kind = b.dataset.sync;
    const extra = {};
    if (kind === "imdb" && $("#imdb-force-download").checked) {
      extra.force_download = true;
    }
    runSync(kind, extra);
  }));

  // Keyboard: 1-4 for tabs.
  document.addEventListener("keydown", (e) => {
    if (e.target.matches("input, textarea, select")) return;
    const idx = ["stats", "torrents", "blacklist", "sync"].indexOf(
      ["1", "2", "3", "4"].indexOf(e.key) >= 0
        ? ["stats", "torrents", "blacklist", "sync"][Number(e.key) - 1]
        : null
    );
    if (idx >= 0) switchTab(["stats", "torrents", "blacklist", "sync"][idx]);
  });
}

function debounce(fn, ms) {
  let t;
  return (...args) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...args), ms);
  };
}

// --- service worker registration (PWA) -----------------------------

if ("serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    navigator.serviceWorker
      .register("sw.js")
      .catch((err) => console.warn("SW registration failed", err));
  });
}

// --- go ------------------------------------------------------------

document.addEventListener("DOMContentLoaded", () => {
  wireUp();
  if (!state.apiKey) {
    showLogin();
  } else {
    showApp();
  }
});
