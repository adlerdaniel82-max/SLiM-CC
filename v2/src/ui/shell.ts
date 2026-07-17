import { escapeHtml, formatBytes } from "../core/dom";
import type { AppState } from "../core/store";

const icon = (name: string) => `<span class="icon" aria-hidden="true">${name}</span>`;

export function renderShell(state: AppState): string {
  const instance = state.instances.find((item) => item.id === state.activeInstanceId);
  const profile = state.profiles.find((item) => item.id === state.activeProfileId);
  return `<div class="app-shell">
    <header class="titlebar"><span class="brand-mark">▰</span><span>${escapeHtml(instance?.name ?? "SLiM-CC v2")}</span><span class="version">Mod ControlCenter ${__APP_VERSION__}</span></header>
    ${menuBar()}
    ${toolBar(state)}
    <main class="workspace">
      <section class="mods-pane">${profileBar(state, profile?.name)}${modTable(state)}</section>
      <section class="detail-pane">${runner(state)}${tabBar(state)}${detailView(state)}</section>
    </main>
    ${activity(state)}
    <footer class="statusbar"><span>${escapeHtml(instance?.game_type ?? "Keine Instanz")}</span><span>${escapeHtml(profile?.name ?? "Kein Profil")}</span><span>${state.mods.length} Mods</span><span class="grow"></span><span class="status-${state.loading ? "busy" : "ok"}">${state.loading ? "Wird aktualisiert …" : "Bereit"}</span></footer>
    <div id="modal-host"></div>
    <div id="context-menu-host"></div>
  </div>`;
}

function menuBar(): string {
  return `<nav class="menubar" aria-label="Hauptmenü">
    <details class="menu"><summary>Datei</summary><div class="menu-popover">
      <button data-action="manage-instances">Instanzen verwalten …</button><button data-action="install-mod">Mod installieren … <kbd>Ctrl+M</kbd></button>
      <button data-action="open-nexus">Nexus besuchen <kbd>Ctrl+N</kbd></button><hr><button data-action="quit">Beenden</button>
    </div></details>
    <details class="menu"><summary>Ansicht</summary><div class="menu-popover">
      <button data-action="refresh">Neu laden <kbd>F5</kbd></button><button data-action="toggle-log">Aktivitätsprotokoll</button>
      <hr><button data-view="downloads">Downloads</button><button data-view="conflicts">Konflikte</button><button data-view="diagnostics">Diagnose</button>
    </div></details>
    <details class="menu"><summary>Werkzeuge</summary><div class="menu-popover">
      <button data-action="manage-profiles">Profile …</button><button data-action="manage-tools">Anwendungen …</button>
      <button data-action="run-loot">LOOT ausführen</button><button data-action="reconfigure-fomod">FOMOD neu konfigurieren …</button><hr><button data-action="settings">Einstellungen … <kbd>Ctrl+,</kbd></button>
    </div></details>
    <details class="menu"><summary>Hilfe</summary><div class="menu-popover"><button data-action="about">Über SLiM-CC</button></div></details>
  </nav>`;
}

function toolBar(state: AppState): string {
  return `<div class="toolbar">
    <button class="tool" data-action="install-mod" title="Mod installieren">${icon("＋")}<span>Installieren</span></button>
    <button class="tool" data-action="refresh" title="Alles aktualisieren">${icon("↻")}<span>Aktualisieren</span></button>
    <button class="tool" data-action="deploy" title="Staging erstellen">${icon("⇧")}<span>Staging</span></button>
    <span class="separator"></span>
    <button class="tool" data-view="downloads">${icon("⇩")}<span>Downloads</span>${state.downloads.filter((item) => !item.installed).length ? `<b class="badge">${state.downloads.filter((item) => !item.installed).length}</b>` : ""}</button>
    <button class="tool" data-view="conflicts">${icon("⚡")}<span>Konflikte</span></button>
    <button class="tool" data-view="diagnostics">${icon("◇")}<span>Diagnose</span></button>
  </div>`;
}

function profileBar(state: AppState, profileName?: string): string {
  return `<div class="profilebar"><label>Profil <select data-field="profile">${state.profiles.map((profile) => `<option value="${profile.id}" ${profile.id === state.activeProfileId ? "selected" : ""}>${escapeHtml(profile.name)}</option>`).join("")}</select></label><span class="grow"></span><label class="search">⌕ <input data-field="mod-search" placeholder="Mods filtern" aria-label="Mods filtern"></label></div>`;
}

function modTable(state: AppState): string {
  const statusById = new Map(state.dependencies.map((item) => [item.mod_id, item]));
  const profileById = new Map(state.profileMods.map((item) => [item.mod_id, item]));
  return `<div class="table-wrap"><table class="data-table mods-table"><thead><tr><th class="check"></th><th>Mod Name</th><th>Status</th><th>Version</th><th>Größe</th><th>Priorität</th></tr></thead><tbody>
    ${state.mods.map((mod) => { const entry = profileById.get(mod.id); const dep = statusById.get(mod.id); return `<tr data-mod-id="${mod.id}" class="${mod.id === state.activeModId ? "selected" : ""}" data-search="${escapeHtml(mod.name.toLowerCase())}">
      <td><input type="checkbox" data-action="toggle-mod" data-id="${mod.id}" ${entry?.enabled ?? mod.enabled_default ? "checked" : ""} aria-label="${escapeHtml(mod.name)} aktiv"></td>
      <td class="primary">${escapeHtml(mod.name)}</td><td><span class="dot ${dep?.missing_count ? "bad" : "good"}" title="${dep?.missing_count ?? 0} fehlende Abhängigkeiten"></span></td>
      <td>${escapeHtml(mod.version ?? "—")}</td><td>${formatBytes(mod.size_bytes)}</td><td>${entry?.priority ?? "—"}</td></tr>`; }).join("") || `<tr><td colspan="6" class="empty">Noch keine Mods installiert.</td></tr>`}
  </tbody></table></div>`;
}

function runner(state: AppState): string {
  const instance = state.instances.find((item) => item.id === state.activeInstanceId);
  return `<div class="runner"><select data-field="instance" aria-label="Instanz">${state.instances.map((item) => `<option value="${item.id}" ${item.id === state.activeInstanceId ? "selected" : ""}>${escapeHtml(item.name)}</option>`).join("")}</select><button class="start" data-action="launch">▶ <span>${instance?.game_starter_path ? "Starten" : "Start konfigurieren"}</span></button></div>`;
}

function tabBar(state: AppState): string {
  return `<nav class="tabs">${(["plugins","downloads","conflicts","diagnostics"] as const).map((view) => `<button data-view="${view}" class="${state.view === view ? "active" : ""}">${({plugins:"Plugins",downloads:"Downloads",conflicts:"Konflikte",diagnostics:"Diagnose"})[view]}</button>`).join("")}</nav>`;
}

function detailView(state: AppState): string {
  if (state.view === "downloads") return downloads(state);
  if (state.view === "conflicts") return conflicts(state);
  if (state.view === "diagnostics") return diagnostics(state);
  return plugins(state);
}

function plugins(state: AppState): string {
  return `<div class="panel-head"><strong>Plugin-Reihenfolge</strong><span class="grow"></span><button data-action="run-loot">Sortieren</button></div><div class="table-wrap"><table class="data-table"><thead><tr><th></th><th>Name</th><th>Mod</th><th>Priorität</th></tr></thead><tbody>${state.plugins.map((plugin) => `<tr><td><input type="checkbox" data-action="toggle-plugin" data-id="${plugin.plugin_id}" ${plugin.enabled ? "checked" : ""}></td><td class="primary">${escapeHtml(plugin.filename)}</td><td>${escapeHtml(plugin.mod_name)}</td><td>${plugin.priority}</td></tr>`).join("") || `<tr><td colspan="4" class="empty">Keine Plugins im aktiven Profil.</td></tr>`}</tbody></table></div>`;
}

function downloads(state: AppState): string {
  return `<div class="panel-head"><strong>Downloadordner</strong><span class="path">${escapeHtml(state.settings?.mod_download_path ?? "Nicht konfiguriert")}</span><span class="grow"></span><button data-action="analyze-collection">Collection …</button><button class="primary-button" data-action="refresh-downloads">↻ Aktualisieren</button></div>
    <div class="downloads">${state.downloads.map((item) => `<article class="download-card ${item.installed ? "installed" : ""}"><div><strong>${escapeHtml(item.name)}</strong>${item.note ? `<small>${escapeHtml(item.note)}</small>` : ""}</div><span class="grow"></span>${item.installed ? `<span class="state-label">Installiert</span>` : `<button class="compact-button" data-action="import-download" data-path="${escapeHtml(item.path)}">Installieren</button>`}</article>`).join("") || `<div class="empty-state">Keine importierbaren Dateien gefunden.<button data-action="refresh-downloads">Jetzt aktualisieren</button></div>`}</div>`;
}

function conflicts(state: AppState): string {
  return `<div class="panel-head"><strong>Konflikte</strong><span class="grow"></span><button data-action="refresh-conflicts">Prüfen</button></div><div class="issue-list">${state.conflicts.map((item) => { const mod = state.mods.find((candidate) => candidate.id === item.mod_id); return `<article><span class="severity warning">!</span><div><strong>${escapeHtml(mod?.name ?? item.mod_id)}</strong><p>${item.losing_file_count} überschriebene, ${item.winning_file_count} gewinnende Dateien</p></div></article>`; }).join("") || `<div class="empty-state">Keine Dateikonflikte im aktiven Profil.</div>`}</div>`;
}

function diagnostics(state: AppState): string {
  const missing = state.dependencies.filter((item) => item.missing_count > 0);
  return `<div class="panel-head"><strong>Diagnose</strong><span class="grow"></span><button data-action="diagnose">Analyse starten</button></div><div class="issue-list">${missing.map((item) => { const mod = state.mods.find((candidate) => candidate.id === item.mod_id); return `<article><span class="severity error">×</span><div><strong>${escapeHtml(mod?.name ?? item.mod_id)}</strong><p>${item.missing_count} von ${item.dependency_count} Abhängigkeiten fehlen.</p></div></article>`; }).join("") || `<div class="empty-state">Keine fehlenden Abhängigkeiten erkannt.</div>`}</div>`;
}

function activity(state: AppState): string {
  const events = state.events.slice(-4);
  return `<section class="activity" data-role="activity"><header>Aktivität</header>${events.map((event) => `<p class="event-${event.severity}"><time>${event.time.toLocaleTimeString("de-DE")}</time>${escapeHtml(event.message)}</p>`).join("") || `<p class="muted">SLiM-CC v2 ist bereit.</p>`}</section>`;
}
