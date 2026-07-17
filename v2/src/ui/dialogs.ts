import { escapeHtml, pathName } from "../core/dom";
import { initialSelections } from "../features/fomod/selection";
import type { AppState } from "../core/store";
import type { FomodPackagePreview, FomodSelectionEntry, GameInstance, Profile } from "../types";

export function instancesDialog(instances: GameInstance[]): string {
  const rows = instances.map((instance) => `<tr><td class="primary">${escapeHtml(instance.name)}</td><td>${escapeHtml(instance.runner_type)}</td><td title="${escapeHtml(instance.install_path)}">${escapeHtml(instance.install_path)}</td></tr>`).join("");
  return dialog("Instanzen verwalten", `<div class="table-wrap compact-table"><table class="data-table"><thead><tr><th>Name</th><th>Runner</th><th>Spielverzeichnis</th></tr></thead><tbody>${rows || `<tr><td colspan="3" class="empty">Noch keine Instanz vorhanden.</td></tr>`}</tbody></table></div><form data-form="create-instance"><h3>Neue Skyrim-SE-Instanz</h3><label>Name<input name="name" required placeholder="Skyrim Special Edition"></label><label>Spielverzeichnis<div class="path-picker"><input name="install_path" required><button type="button" data-action="pick-install-folder">Ordner …</button></div></label><label>Data-Verzeichnis<div class="path-picker"><input name="data_path" required><button type="button" data-action="pick-data-folder">Ordner …</button></div></label><label>Starter<div class="path-picker"><input name="game_starter_path" placeholder="SkyrimSELauncher.exe oder Startscript"><button type="button" data-action="pick-game-starter">Datei …</button></div></label><div class="form-columns"><label>Runner<select name="runner_type"><option value="wine">Wine</option><option value="native">Nativ</option><option value="proton">Proton</option><option value="manual">Manuell</option></select></label><label>Wine Prefix<input name="wine_prefix"></label></div><footer><button type="button" data-action="close-dialog">Schließen</button><button class="primary-button" type="submit">Instanz anlegen</button></footer></form>`, "wide");
}

export function profilesDialog(instance: GameInstance, profiles: Profile[]): string {
  return dialog(`Profile · ${escapeHtml(instance.name)}`, `<div class="profile-list">${profiles.map((profile) => `<span>${escapeHtml(profile.name)}</span>`).join("") || `<p class="hint">Noch kein Profil vorhanden.</p>`}</div><form data-form="create-profile"><label>Neues Profil<input name="name" required placeholder="z. B. Vanilla+, Test oder Survival"></label><footer><button type="button" data-action="close-dialog">Schließen</button><button class="primary-button" type="submit">Profil anlegen</button></footer></form>`);
}

export function importDialog(state: AppState, sourcePath = ""): string {
  return dialog("Mod installieren", `<form data-form="import-mod"><label>Quelle<div class="path-picker"><input name="source_path" value="${escapeHtml(sourcePath)}" placeholder="Archiv oder Mod-Ordner"><button type="button" data-action="pick-archive">Datei …</button><button type="button" data-action="pick-folder">Ordner …</button></div></label><div class="choice-separator">oder</div><label>Nexus-Download-Link<input name="nexus_url" placeholder="nxm://… oder Nexus-Dateilink"></label><label>Name<input name="name" value="${escapeHtml(sourcePath ? pathName(sourcePath).replace(/\.(zip|7z|rar|fomod)$/i, "") : "")}" placeholder="Wird aus der Quelle übernommen"></label><details><summary>Erweiterte Angaben</summary><label><input type="checkbox" name="copy_into_workspace" checked> In den sicheren Workspace kopieren</label></details><footer><button type="button" data-action="close-dialog">Abbrechen</button><button class="primary-button" type="submit">Weiter</button></footer></form>`);
}

export function settingsDialog(state: AppState): string {
  const s = state.settings;
  return dialog("Einstellungen", `<form data-form="settings"><label>Downloadordner<div class="path-picker"><input name="mod_download_path" value="${escapeHtml(s?.mod_download_path ?? "")}"><button type="button" data-action="pick-download-folder">Ordner …</button></div></label><label>Spielverzeichnis<input name="install_path" value="${escapeHtml(s?.install_path ?? "")}"></label><label>Data-Verzeichnis<input name="data_path" value="${escapeHtml(s?.data_path ?? "")}"></label><label>Wine Prefix<input name="wine_prefix" value="${escapeHtml(s?.wine_prefix ?? "")}"></label><details><summary>Nexus API</summary><label>API-Schlüssel<input type="password" name="nexus_api_key" autocomplete="off" placeholder="${s?.nexus_api_key_masked ?? "Nicht eingerichtet"}"></label></details><footer><button type="button" data-action="close-dialog">Abbrechen</button><button class="primary-button" type="submit">Speichern</button></footer></form>`);
}

export function collectionDialog(): string {
  return dialog("Nexus Collection analysieren", `<form data-form="collection"><label>Collection-Link<input name="collection_url" required placeholder="https://next.nexusmods.com/.../collections/..."></label><p class="hint">Die Collection wird über die Nexus-API geladen. Navigations- und Social-Media-Links werden nicht übernommen.</p><footer><button type="button" data-action="close-dialog">Abbrechen</button><button class="primary-button" type="submit">Analysieren</button></footer></form>`);
}

export function collectionResultDialog(result: {collection_title: string | null; items: Array<{title: string; item_type: string; url: string}>}): string {
  return dialog(result.collection_title ?? "Collection", `<div class="collection-results">${result.items.map((item) => `<article><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(item.item_type)}</small><span class="grow"></span><button data-action="use-collection-item" data-url="${escapeHtml(item.url)}">Übernehmen</button></article>`).join("") || `<p>Keine Mod-Dateien gefunden.</p>`}</div><footer><button class="primary-button" data-action="close-dialog">Schließen</button></footer>`, "wide");
}

export function fomodDialog(preview: FomodPackagePreview, selections: FomodSelectionEntry[] = initialSelections(preview)): string {
  const groups = preview.steps.flatMap((step, si) => step.groups.map((group, gi) => `<fieldset data-step="${si}" data-group="${gi}"><legend>${escapeHtml(step.name)} · ${escapeHtml(group.name)}</legend><p class="hint">${escapeHtml(group.selection_mode)}</p>${group.options.map((option) => { const selected = selections.find((entry) => entry.step_index === si && entry.group_index === gi)?.selected_option_ids.includes(option.id); const radio = ["selectexactlyone","selectatmostone"].includes(group.selection_mode.toLowerCase()); const required = option.option_type?.toLowerCase() === "required"; return `<label class="fomod-option"><input type="${radio ? "radio" : "checkbox"}" name="fomod-${si}-${gi}" value="${option.id}" ${selected ? "checked" : ""} ${required ? "disabled data-required=true" : ""}><span><strong>${escapeHtml(option.name)}</strong><small>${escapeHtml(required ? "Erforderlich" : option.description ?? `${option.file_count} Dateien`)}</small></span></label>`; }).join("")}</fieldset>`)).join("");
  return dialog(`FOMOD · ${escapeHtml(preview.module_name ?? "Installation")}`, `<form data-form="fomod">${preview.validation_notes.map((note) => `<p class="notice">${escapeHtml(note)}</p>`).join("")}${groups}<footer><button type="button" data-action="cancel-fomod">Abbrechen</button><button class="primary-button" type="submit">Installieren</button></footer></form>`, "wide");
}

export function infoDialog(title: string, text: string): string { return dialog(title, `<p>${escapeHtml(text)}</p><footer><button class="primary-button" data-action="close-dialog">OK</button></footer>`); }

export function deleteModDialog(modId: string, modName: string): string {
  return dialog("Mod löschen", `<p>Soll <strong>${escapeHtml(modName)}</strong> wirklich aus SLiM-CC entfernt werden?</p><p class="hint">Die verwaltete Modkopie wird gelöscht. Das ursprüngliche Downloadarchiv bleibt erhalten.</p><footer><button type="button" data-action="close-dialog">Abbrechen</button><button class="danger-button" type="button" data-action="confirm-delete-mod" data-mod-id="${escapeHtml(modId)}">Mod löschen</button></footer>`);
}

function dialog(title: string, body: string, className = ""): string {
  return `<div class="modal-backdrop"><section class="dialog ${className}" role="dialog" aria-modal="true" aria-labelledby="dialog-title"><header><h2 id="dialog-title">${title}</h2><button class="close" data-action="close-dialog" aria-label="Schließen">×</button></header><div class="dialog-body">${body}</div></section></div>`;
}
