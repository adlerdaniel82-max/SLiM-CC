import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { backend, type Backend } from "./core/api";
import { closeMenus, installMenuBehavior } from "./core/menu";
import { escapeHtml, pathName } from "./core/dom";
import { Store } from "./core/store";
import { normalizeSelection, selectionIsValid } from "./features/fomod/selection";
import type { AppSettings, FomodPackagePreview, FomodSelectionEntry, GameInstance, ImportedModReport, ModConflictSummary, ModDependencyStatus, ModDependencySummary, ModDownloadCandidate, ModRecord, NexusRequirementStatus, Profile, ProfileModEntry, ProfilePluginEntry, ToolExecutableCandidate, ToolProfile, View } from "./types";
import { aboutDialog, collectionDialog, collectionResultDialog, deleteInstanceDialog, deleteModDialog, deleteProfileDialog, dependencyDetailsDialog, editInstanceDialog, fomodDialog, importDialog, instancesDialog, profilesDialog, settingsDialog, toolsDialog } from "./ui/dialogs";
import { renderShell } from "./ui/shell";

export class SlimApp {
  private readonly store = new Store();
  private readonly api: Backend;
  private downloadTimer: number | null = null;
  private logVisible = true;
  private pendingImport: { sourcePath: string; name: string; targetModId?: string } | null = null;
  private fomodSelections: FomodSelectionEntry[] = [];
  private readonly externalActivities = new Map<symbol, string>();

  private readonly useDeepLinks: boolean;

  constructor(private readonly root: HTMLElement, api?: Backend) {
    this.api = api ?? backend();
    this.useDeepLinks = api === undefined && !(window as Window & { __SLIMCC_BACKEND__?: Backend }).__SLIMCC_BACKEND__;
    this.store.addEventListener("change", () => this.render());
  }

  async start(): Promise<void> {
    this.render(); this.bind(); installMenuBehavior(this.root);
    await this.refreshAll(); this.startDownloadWatcher();
    if (this.useDeepLinks) {
      await this.initializeDeepLinks();
    }
  }

  private render(): void {
    const currentShell = this.root.querySelector<HTMLElement>(":scope > .app-shell");
    if (!currentShell) this.root.innerHTML = renderShell(this.store.state);
    else {
      const openMenus = [...currentShell.querySelectorAll<HTMLDetailsElement>(".menu")].map((menu) => menu.open);
      const scrollPositions = new Map(
        [...currentShell.querySelectorAll<HTMLElement>("[data-scroll-key]")]
          .map((element) => [element.dataset.scrollKey ?? "", element.scrollTop] as const)
      );
      const template = document.createElement("template"); template.innerHTML = renderShell(this.store.state);
      const freshShell = template.content.firstElementChild as HTMLElement;
      const currentChildren = [...currentShell.children]; const freshChildren = [...freshShell.children];
      freshChildren.forEach((fresh, index) => {
        if (fresh.id === "modal-host" || fresh.id === "context-menu-host") return;
        currentChildren[index]?.replaceWith(fresh);
      });
      currentShell.querySelectorAll<HTMLDetailsElement>(".menu").forEach((menu, index) => { menu.open = openMenus[index] ?? false; });
      currentShell.querySelectorAll<HTMLElement>("[data-scroll-key]").forEach((element) => {
        const scrollTop = scrollPositions.get(element.dataset.scrollKey ?? "");
        if (scrollTop !== undefined) element.scrollTop = scrollTop;
      });
    }
    const activity = this.root.querySelector<HTMLElement>("[data-role=activity]");
    if (activity) activity.hidden = !this.logVisible;
  }

  private bind(): void {
    this.root.addEventListener("click", (event) => void this.onClick(event));
    this.root.addEventListener("contextmenu", (event) => this.onContextMenu(event));
    this.root.addEventListener("change", (event) => void this.onChange(event));
    this.root.addEventListener("input", (event) => this.onInput(event));
    this.root.addEventListener("submit", (event) => void this.onSubmit(event));
    window.addEventListener("focus", () => void this.refreshDownloads(true));
    document.addEventListener("visibilitychange", () => { if (!document.hidden) void this.refreshDownloads(true); });
    document.addEventListener("keydown", (event) => {
      if (event.key === "F5") { event.preventDefault(); void this.refreshAll(); }
      if (event.ctrlKey && event.key.toLowerCase() === "m") { event.preventDefault(); this.openDialog(importDialog(this.store.state)); }
      if (event.ctrlKey && event.key === ",") { event.preventDefault(); this.openDialog(settingsDialog(this.store.state)); }
    });
  }

  private async onClick(event: Event): Promise<void> {
    const target = (event.target as Element).closest<HTMLElement>("[data-action],[data-view],[data-mod-id]");
    if (!target) { this.closeContextMenu(); return; }
    const view = target.dataset.view as View | undefined;
    if (view) { this.store.patch({ view }); closeMenus(this.root); if (view === "downloads") await this.refreshDownloads(); return; }
    if (target.dataset.modId && !target.dataset.action) { this.store.patch({ activeModId: target.dataset.modId }); return; }
    const action = target.dataset.action; closeMenus(this.root); this.closeContextMenu();
    try {
      switch (action) {
        case "refresh": await this.refreshAll(); break;
        case "refresh-downloads": await this.refreshDownloads(); break;
        case "refresh-conflicts": await this.refreshProfileDetails(); break;
        case "install-mod": this.openDialog(importDialog(this.store.state)); break;
        case "import-download": this.openDialog(importDialog(this.store.state, target.dataset.path)); break;
        case "analyze-collection": this.openDialog(collectionDialog()); break;
        case "use-collection-item": this.openDialog(importDialog(this.store.state)); { const input = this.root.querySelector<HTMLInputElement>("[name=nexus_url]"); if (input) input.value = target.dataset.url ?? ""; } break;
        case "pick-archive": await this.pick("pick_file", "source_path", "Mod-Archiv auswählen"); break;
        case "pick-folder": await this.pick("pick_directory", "source_path", "Mod-Ordner auswählen"); break;
        case "pick-download-folder": await this.pick("pick_directory", "mod_download_path", "Downloadordner auswählen"); break;
        case "pick-install-folder": await this.pick("pick_directory", "install_path", "Spielverzeichnis auswählen"); break;
        case "pick-data-folder": await this.pick("pick_directory", "data_path", "Data-Verzeichnis auswählen"); break;
        case "pick-game-starter": await this.pick("pick_file", "game_starter_path", "Spielstarter auswählen"); break;
        case "pick-tool-executable": await this.pickToolExecutable(target); break;
        case "use-tool-candidate": this.useToolCandidate(target); break;
        case "close-dialog": this.closeDialog(); break;
        case "cancel-fomod": await this.cancelFomod(); break;
        case "delete-mod": { const mod = this.store.state.mods.find((item) => item.id === target.dataset.modId); if (mod) this.openDialog(deleteModDialog(mod.id, mod.name)); break; }
        case "confirm-delete-mod": await this.deleteMod(target.dataset.modId ?? ""); break;
        case "mod-details": await this.showModDetails(target.dataset.modId ?? ""); break;
        case "edit-instance": { const instance = this.store.state.instances.find((item) => item.id === target.dataset.instanceId); if (instance) this.openDialog(editInstanceDialog(instance)); break; }
        case "request-delete-instance": { const instance = this.store.state.instances.find((item) => item.id === target.dataset.instanceId); if (instance) this.openDialog(deleteInstanceDialog(instance.id, instance.name)); break; }
        case "confirm-delete-instance": await this.deleteInstance(target.dataset.instanceId ?? ""); break;
        case "request-delete-profile": { const profile = this.store.state.profiles.find((item) => item.id === target.dataset.profileId); if (profile) this.openDialog(deleteProfileDialog(profile.id, profile.name)); break; }
        case "confirm-delete-profile": await this.deleteProfile(target.dataset.profileId ?? ""); break;
        case "open-dependency-url": await this.api.call("open_external_url", { url: target.dataset.url }); break;
        case "open-about-site": await this.api.call("open_external_url", { url: "https://schnueddels.de" }); break;
        case "settings": this.openDialog(settingsDialog(this.store.state)); break;
        case "toggle-log": this.logVisible = !this.logVisible; this.render(); break;
        case "deploy": await this.deploy(); break;
        case "launch": await this.launch(); break;
        case "run-loot": await this.runLoot(); break;
        case "launch-vfs-tool": await this.launchVfsTool(target.dataset.toolKey ?? ""); break;
        case "reconfigure-fomod": await this.reconfigureFomod(); break;
        case "diagnose": await this.diagnose(); break;
        case "open-nexus": await this.api.call("open_external_url", { url: "https://www.nexusmods.com/skyrimspecialedition" }); break;
        case "manage-instances": this.openDialog(instancesDialog(this.store.state.instances)); break;
        case "manage-profiles": { const instance = this.store.state.instances.find((item) => item.id === this.store.state.activeInstanceId); if (!instance) throw new Error("Bitte zuerst eine Instanz anlegen."); this.openDialog(profilesDialog(instance, this.store.state.profiles)); break; }
        case "manage-tools": await this.openToolsDialog(); break;
        case "about": this.openDialog(aboutDialog()); break;
        case "quit": window.close(); break;
      }
    } catch (error) { this.fail(error); }
  }

  private onContextMenu(event: MouseEvent): void {
    const row = (event.target as Element).closest<HTMLElement>("[data-mod-id]");
    if (!row?.dataset.modId) return;
    event.preventDefault(); closeMenus(this.root);
    if (this.store.state.activeModId !== row.dataset.modId) this.store.patch({ activeModId: row.dataset.modId });
    const host = this.root.querySelector<HTMLElement>("#context-menu-host"); if (!host) return;
    const left = Math.min(event.clientX, window.innerWidth - 210); const top = Math.min(event.clientY, window.innerHeight - 120);
    host.innerHTML = `<div class="context-menu" role="menu" style="left:${Math.max(4, left)}px;top:${Math.max(4, top)}px"><button role="menuitem" data-action="mod-details" data-mod-id="${escapeHtml(row.dataset.modId)}">Details …</button><button role="menuitem" data-action="delete-mod" data-mod-id="${escapeHtml(row.dataset.modId)}">Mod löschen …</button></div>`;
  }

  private closeContextMenu(): void { const host = this.root.querySelector<HTMLElement>("#context-menu-host"); if (host) host.innerHTML = ""; }

  private async onChange(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement | HTMLSelectElement;
    try {
      if (input.dataset.field === "instance") await this.selectInstance(input.value);
      if (input.dataset.field === "profile") await this.selectProfile(input.value);
      if (input.dataset.action === "toggle-mod") await this.toggleMod(input.dataset.id ?? "", (input as HTMLInputElement).checked);
      if (input.dataset.action === "toggle-plugin") await this.togglePlugin(input.dataset.id ?? "", (input as HTMLInputElement).checked);
      if (input.name.startsWith("fomod-")) this.readFomodSelections();
      if (input.name === "source_path") this.suggestModName(input.value);
    } catch (error) { this.fail(error); }
  }

  private onInput(event: Event): void {
    const input = event.target as HTMLInputElement;
    if (input.dataset.field === "mod-search") {
      const query = input.value.trim().toLowerCase();
      this.root.querySelectorAll<HTMLElement>("[data-search]").forEach((row) => { row.hidden = !row.dataset.search?.includes(query); });
    }
  }

  private async onSubmit(event: Event): Promise<void> {
    const form = event.target as HTMLFormElement; if (!form.dataset.form) return; event.preventDefault();
    try {
      if (form.dataset.form === "import-mod") await this.prepareImport(form);
      if (form.dataset.form === "fomod") await this.finishFomod();
      if (form.dataset.form === "settings") await this.saveSettings(form);
      if (form.dataset.form === "collection") await this.analyzeCollection(form);
      if (form.dataset.form === "create-instance") await this.createInstance(form);
      if (form.dataset.form === "update-instance") await this.updateInstance(form);
      if (form.dataset.form === "create-profile") await this.createProfile(form);
      if (form.dataset.form === "tool-profile") await this.saveToolProfile(form);
    } catch (error) { this.fail(error); }
  }

  async refreshAll(): Promise<void> {
    this.store.patch({ loading: true });
    try {
      const [instances, settings, downloads] = await Promise.all([
        this.api.call<GameInstance[]>("list_instances"), this.api.call<AppSettings>("get_app_settings"),
        this.api.call<ModDownloadCandidate[]>("list_mod_download_candidates", { instanceId: this.store.state.activeInstanceId })
      ]);
      const activeInstanceId = instances.some((item) => item.id === this.store.state.activeInstanceId) ? this.store.state.activeInstanceId : instances[0]?.id ?? null;
      this.store.patch({ instances, settings, downloads, activeInstanceId });
      if (activeInstanceId) await this.loadInstance(activeInstanceId);
      else this.store.patch({ profiles: [], mods: [], dependencies: [], profileMods: [], plugins: [], conflicts: [], activeProfileId: null, activeModId: null });
      this.store.log("success", "Daten wurden aktualisiert.");
    } finally { this.store.patch({ loading: false }); }
  }

  private async loadInstance(instanceId: string): Promise<void> {
    const [profiles, mods, dependencies] = await Promise.all([
      this.api.call<Profile[]>("list_profiles", { instanceId }), this.api.call<ModRecord[]>("list_mods", { instanceId }),
      this.api.call<ModDependencySummary[]>("list_instance_dependency_summary", { instanceId })
    ]);
    const activeProfileId = profiles.some((item) => item.id === this.store.state.activeProfileId) ? this.store.state.activeProfileId : profiles[0]?.id ?? null;
    this.store.patch({ profiles, mods, dependencies, activeProfileId, activeModId: mods[0]?.id ?? null });
    if (activeProfileId) await this.loadProfile(instanceId, activeProfileId);
    else this.store.patch({ profileMods: [], plugins: [], conflicts: [] });
  }

  private async loadProfile(instanceId: string, profileId: string): Promise<void> {
    const [profileMods, plugins, conflicts] = await Promise.all([
      this.api.call<ProfileModEntry[]>("list_profile_mods", { instanceId, profileId }),
      this.api.call<ProfilePluginEntry[]>("list_profile_plugins", { instanceId, profileId }),
      this.api.call<ModConflictSummary[]>("list_mod_conflict_summary", { instanceId, profileId })
    ]);
    this.store.patch({ profileMods, plugins, conflicts });
  }

  private async refreshProfileDetails(): Promise<void> {
    const { activeInstanceId, activeProfileId } = this.store.state;
    if (activeInstanceId && activeProfileId) await this.loadProfile(activeInstanceId, activeProfileId);
  }

  private async selectInstance(id: string): Promise<void> { this.store.patch({ activeInstanceId: id }); await this.loadInstance(id); }
  private async selectProfile(id: string): Promise<void> { this.store.patch({ activeProfileId: id }); if (this.store.state.activeInstanceId) await this.loadProfile(this.store.state.activeInstanceId, id); }

  private async refreshDownloads(silent = false): Promise<void> {
    if (!this.store.state.settings?.mod_download_path) return;
    const downloads = await this.api.call<ModDownloadCandidate[]>("list_mod_download_candidates", { instanceId: this.store.state.activeInstanceId });
    const before = new Set(this.store.state.downloads.map((item) => item.path));
    const added = downloads.filter((item) => !before.has(item.path)).length;
    const changed = !sameDownloads(this.store.state.downloads, downloads);
    if (changed) {
      const scrollTop = this.root.querySelector<HTMLElement>(".downloads")?.scrollTop ?? 0;
      this.store.patch({ downloads });
      const list = this.root.querySelector<HTMLElement>(".downloads"); if (list) list.scrollTop = scrollTop;
    }
    if (!silent || added) this.store.log(added ? "success" : "info", added ? `${added} neue Download-Datei(en) erkannt.` : "Downloadordner aktualisiert.");
  }

  private startDownloadWatcher(): void {
    if (this.downloadTimer !== null) window.clearInterval(this.downloadTimer);
    this.downloadTimer = window.setInterval(() => { if (!document.hidden) void this.refreshDownloads(true).catch((error) => this.fail(error)); }, 4000);
  }

  private async toggleMod(modId: string, enabled: boolean): Promise<void> {
    const { activeInstanceId, activeProfileId, profileMods } = this.store.state; if (!activeInstanceId || !activeProfileId) return;
    const entries = profileMods.map((entry) => ({ mod_id: entry.mod_id, enabled: entry.mod_id === modId ? enabled : entry.enabled, priority: entry.priority }));
    await this.api.call("update_profile_mods", { request: { instance_id: activeInstanceId, profile_id: activeProfileId, entries } });
    await this.refreshProfileDetails(); this.store.log("success", `Mod ${enabled ? "aktiviert" : "deaktiviert"}.`);
  }

  private async togglePlugin(pluginId: string, enabled: boolean): Promise<void> {
    const { activeInstanceId, activeProfileId } = this.store.state; if (!activeInstanceId || !activeProfileId) return;
    const entries = [{ plugin_id: pluginId, enabled }];
    await this.api.call("update_profile_plugins", { request: { instance_id: activeInstanceId, profile_id: activeProfileId, entries } });
    await this.refreshProfileDetails();
  }

  private async prepareImport(form: HTMLFormElement): Promise<void> {
    const data = new FormData(form); let sourcePath = String(data.get("source_path") ?? "").trim();
    const nexusUrl = String(data.get("nexus_url") ?? "").trim();
    if ((!sourcePath && !nexusUrl) || !this.store.state.activeInstanceId) throw new Error("Eine lokale Quelle oder ein Nexus-Download-Link wird benötigt.");
    const progress = this.showOperationProgress(form, nexusUrl && !sourcePath ? "Nexus-Datei wird heruntergeladen …" : "Modpaket wird analysiert …");
    this.store.patch({ loading: true });
    try {
      if (!sourcePath && nexusUrl) {
        this.store.log("info", "Nexus-Download wird gestartet …");
        const download = await this.api.call<{path: string; file_name: string; bytes_written: number}>("download_nexus_file", { request: { source_url: nexusUrl } });
        sourcePath = download.path; this.store.log("success", `${download.file_name} wurde vollständig heruntergeladen.`);
      }
      const name = String(data.get("name") ?? "").trim() || pathName(sourcePath).replace(/\.(zip|7z|rar|fomod)$/i, "");
      this.pendingImport = { sourcePath, name };
      progress.update("Archiv wird entpackt und auf FOMOD-Inhalte geprüft …");
      const preview = await this.api.call<FomodPackagePreview>("preview_fomod_package", { packageRoot: sourcePath, instanceId: this.store.state.activeInstanceId, profileId: this.store.state.activeProfileId });
      if (preview.has_fomod) { this.store.patch({ fomod: preview }); this.openDialog(fomodDialog(preview)); this.readFomodSelections(); }
      else { progress.update("Moddateien werden in den sicheren Workspace importiert …"); await this.importPrepared(null); }
    } finally { progress.finish(); this.store.patch({ loading: false }); }
  }

  private async importPrepared(selection: { selections: FomodSelectionEntry[] } | null): Promise<void> {
    if (!this.pendingImport || !this.store.state.activeInstanceId) return;
    const request = { instance_id: this.store.state.activeInstanceId, profile_id: this.store.state.activeProfileId,
      target_mod_id: this.pendingImport.targetModId ?? null, name: this.pendingImport.name,
      source_path: this.pendingImport.sourcePath, copy_into_workspace: true, fomod_selection: selection,
      preview_source_path: this.store.state.fomod?.source_path ?? null };
    const report = await this.api.call<ImportedModReport>("import_mod_folder", { request });
    this.store.log("success", `${report.mod_record.name}: ${report.files_scanned} Dateien, ${report.plugins_discovered} Plugins importiert.`);
    this.pendingImport = null; this.store.patch({ fomod: null }); this.closeDialog(); await this.loadInstance(this.store.state.activeInstanceId);
  }

  private readFomodSelections(): void {
    const preview = this.store.state.fomod; if (!preview) return; const selections: FomodSelectionEntry[] = [];
    preview.steps.forEach((step, si) => step.groups.forEach((group, gi) => {
      const selected = [...this.root.querySelectorAll<HTMLInputElement>(`[name="fomod-${si}-${gi}"]:checked`)].map((input) => input.value);
      selections.push({ step_index: si, group_index: gi, selected_option_ids: normalizeSelection(group, selected) });
    })); this.fomodSelections = selections;
  }

  private async finishFomod(): Promise<void> {
    const preview = this.store.state.fomod; if (!preview) return; this.readFomodSelections();
    if (!selectionIsValid(preview, this.fomodSelections)) throw new Error("Die FOMOD-Auswahl ist noch nicht vollständig.");
    const form = this.root.querySelector<HTMLFormElement>('form[data-form="fomod"]');
    const progress = form ? this.showOperationProgress(form, "FOMOD-Auswahl wird installiert …") : null;
    this.store.patch({ loading: true });
    try { await this.importPrepared({ selections: this.fomodSelections }); }
    finally { progress?.finish(); this.store.patch({ loading: false }); }
  }

  private showOperationProgress(form: HTMLFormElement, initialMessage: string): { update: (message: string) => void; finish: () => void } {
    const controls = [...form.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLButtonElement>("input, select, button")];
    const originalDisabled = controls.map((control) => control.disabled);
    controls.forEach((control) => { control.disabled = true; });
    const panel = document.createElement("div"); panel.className = "operation-progress"; panel.setAttribute("role", "status"); panel.setAttribute("aria-live", "polite");
    panel.innerHTML = '<span class="progress-spinner" aria-hidden="true"></span><span><strong></strong><small>Große Archive können mehrere Minuten benötigen. SLiM-CC arbeitet weiter.</small></span>';
    const message = panel.querySelector<HTMLElement>("strong"); if (message) message.textContent = initialMessage;
    const elapsed = document.createElement("small"); elapsed.className = "progress-elapsed"; panel.querySelector("span:last-child")?.append(elapsed);
    form.prepend(panel); const started = Date.now();
    const updateElapsed = () => { elapsed.textContent = `Verstrichen: ${Math.floor((Date.now() - started) / 1000)} s`; };
    updateElapsed(); const timer = window.setInterval(updateElapsed, 1000);
    return {
      update: (text) => { if (message) message.textContent = text; },
      finish: () => { window.clearInterval(timer); panel.remove(); controls.forEach((control, index) => { control.disabled = originalDisabled[index]; }); }
    };
  }

  private async reconfigureFomod(): Promise<void> {
    const mod = this.store.state.mods.find((item) => item.id === this.store.state.activeModId);
    if (!mod) throw new Error("Bitte zuerst einen Mod auswählen.");
    const preview = await this.api.call<FomodPackagePreview>("preview_mod_fomod", { modId: mod.id });
    if (!preview.has_fomod) throw new Error("Der ausgewählte Mod besitzt keine FOMOD-Konfiguration.");
    this.pendingImport = { sourcePath: mod.source_path, name: mod.name, targetModId: mod.id };
    this.store.patch({ fomod: preview }); this.openDialog(fomodDialog(preview)); this.readFomodSelections();
  }

  private async cancelFomod(): Promise<void> {
    const previewPath = this.store.state.fomod?.source_path;
    if (previewPath) await this.api.call("discard_fomod_preview", { previewPath });
    this.pendingImport = null; this.store.patch({ fomod: null }); this.closeDialog();
  }

  private async saveSettings(form: HTMLFormElement): Promise<void> {
    const data = new FormData(form); const text = (name: string) => String(data.get(name) ?? "").trim() || null;
    const settings = await this.api.call<AppSettings>("update_app_settings", { request: { install_path: text("install_path"), data_path: text("data_path"), wine_prefix: text("wine_prefix"), loot_executable_path: this.store.state.settings?.loot_executable_path ?? null, mod_download_path: text("mod_download_path"), language: "de" } });
    const key = text("nexus_api_key"); if (key) await this.api.call("update_nexus_api_key", { request: { api_key: key } });
    this.store.patch({ settings }); this.closeDialog(); await this.refreshDownloads(); this.store.log("success", "Einstellungen gespeichert.");
  }

  private async createInstance(form: HTMLFormElement): Promise<void> {
    const data = new FormData(form); const text = (name: string) => String(data.get(name) ?? "").trim();
    const installPath = text("install_path"); const dataPath = text("data_path");
    if (!installPath || !dataPath) throw new Error("Spiel- und Data-Verzeichnis werden benötigt.");
    const instance = await this.api.call<GameInstance>("create_instance", { request: {
      name: text("name"), install_path: installPath, data_path: dataPath,
      game_starter_path: text("game_starter_path") || null, runner_type: text("runner_type") || "wine",
      wine_prefix: text("wine_prefix") || null
    } });
    await this.api.call<Profile>("create_profile", { request: { instance_id: instance.id, name: "Standard" } });
    this.closeDialog(); this.store.patch({ activeInstanceId: instance.id, activeProfileId: null });
    await this.refreshAll(); this.store.log("success", `Instanz ${instance.name} mit Standardprofil angelegt.`);
  }

  private async updateInstance(form: HTMLFormElement): Promise<void> {
    const data = new FormData(form); const text = (name: string) => String(data.get(name) ?? "").trim();
    const id = text("id"); const installPath = text("install_path"); const dataPath = text("data_path");
    if (!id || !installPath || !dataPath) throw new Error("Instanz, Spiel- und Data-Verzeichnis werden benötigt.");
    const instance = await this.api.call<GameInstance>("update_instance", { request: {
      id, name: text("name"), install_path: installPath, data_path: dataPath,
      game_starter_path: text("game_starter_path") || null, runner_type: text("runner_type") || "wine",
      wine_prefix: text("wine_prefix") || null
    } });
    this.closeDialog(); this.store.patch({ activeInstanceId: instance.id });
    await this.refreshAll(); this.store.log("success", `Instanz ${instance.name} wurde aktualisiert.`);
  }

  private async createProfile(form: HTMLFormElement): Promise<void> {
    const instanceId = this.store.state.activeInstanceId; const name = String(new FormData(form).get("name") ?? "").trim();
    if (!instanceId || !name) throw new Error("Instanz und Profilname werden benötigt.");
    const profile = await this.api.call<Profile>("create_profile", { request: { instance_id: instanceId, name } });
    this.closeDialog(); this.store.patch({ activeProfileId: profile.id }); await this.loadInstance(instanceId);
    this.store.log("success", `Profil ${profile.name} angelegt.`);
  }

  private async deleteMod(modId: string): Promise<void> {
    const mod = this.store.state.mods.find((item) => item.id === modId); if (!mod) return;
    await this.api.call("delete_mod", { modId }); this.closeDialog();
    if (this.store.state.activeInstanceId) await this.loadInstance(this.store.state.activeInstanceId);
    this.store.log("success", `${mod.name} wurde gelöscht. Das Downloadarchiv bleibt erhalten.`);
  }

  private async deleteInstance(instanceId: string): Promise<void> {
    const instance = this.store.state.instances.find((item) => item.id === instanceId); if (!instance) return;
    await this.api.call("delete_instance", { instanceId }); this.closeDialog();
    await this.refreshAll(); this.store.log("success", `Instanz ${instance.name} wurde gelöscht.`);
  }

  private async deleteProfile(profileId: string): Promise<void> {
    const profile = this.store.state.profiles.find((item) => item.id === profileId); if (!profile) return;
    await this.api.call("delete_profile", { profileId }); this.closeDialog();
    await this.refreshAll(); this.store.log("success", `Profil ${profile.name} wurde gelöscht.`);
  }

  private async showModDetails(modId: string): Promise<void> {
    const mod = this.store.state.mods.find((item) => item.id === modId); if (!mod) return;
    const [dependencies, nexusRequirements] = await Promise.all([
      this.api.call<ModDependencyStatus[]>("list_mod_dependencies", { modId }),
      this.api.call<NexusRequirementStatus[]>("list_nexus_requirement_status", { modId })
    ]);
    this.openDialog(dependencyDetailsDialog(mod.name, dependencies, nexusRequirements));
  }

  private async analyzeCollection(form: HTMLFormElement): Promise<void> {
    const url = String(new FormData(form).get("collection_url") ?? "").trim();
    const result = await this.api.call<{collection_title: string | null; items: Array<{title: string; item_type: string; url: string}>}>("analyze_nexus_collection", { request: { collection_url: url } });
    this.openDialog(collectionResultDialog(result)); this.store.log("success", `${result.items.length} Collection-Einträge geladen.`);
  }

  private async openToolsDialog(): Promise<void> {
    const { activeInstanceId: instanceId, activeProfileId: profileId } = this.store.state;
    const [profiles, candidates] = await Promise.all([
      this.api.call<ToolProfile[]>("list_tool_profiles"),
      instanceId && profileId ? this.api.call<ToolExecutableCandidate[]>("list_tool_executable_candidates", { instanceId, profileId }) : Promise.resolve([])
    ]);
    this.openDialog(toolsDialog(profiles, candidates));
  }

  private useToolCandidate(target: HTMLElement): void {
    const form = target.closest<HTMLFormElement>("form[data-form=tool-profile]");
    const candidate = form?.querySelector<HTMLSelectElement>("[data-role=tool-candidate]")?.value;
    const input = form?.querySelector<HTMLInputElement>("[name=executable_path]");
    if (candidate && input) input.value = candidate;
  }

  private async pickToolExecutable(target: HTMLElement): Promise<void> {
    const form = target.closest<HTMLFormElement>("form[data-form=tool-profile]");
    const input = form?.querySelector<HTMLInputElement>("[name=executable_path]");
    const value = await this.api.call<string | null>("pick_file", {
      title: "Werkzeug auswählen",
      defaultPath: input?.value || null
    });
    if (input && value) input.value = value;
  }

  private async saveToolProfile(form: HTMLFormElement): Promise<void> {
    const current = (await this.api.call<ToolProfile[]>("list_tool_profiles")).find((item) => item.tool_key === form.dataset.toolKey);
    if (!current) throw new Error("Werkzeugprofil wurde nicht gefunden.");
    const data = new FormData(form); const text = (name: string) => String(data.get(name) ?? "").trim();
    await this.api.call<ToolProfile>("upsert_tool_profile", { request: {
      tool_key: current.tool_key,
      display_name: current.display_name,
      executable_path: text("executable_path") || null,
      runner_type: text("runner_type") || "Wine",
      arguments: text("arguments").split(/\s+/).filter(Boolean),
      working_directory: text("working_directory") || null,
      wine_prefix: current.wine_prefix,
      log_path: current.log_path,
      enabled: data.get("enabled") === "on"
    } });
    await this.openToolsDialog();
    this.store.log("success", `${current.display_name} gespeichert.`);
  }

  private async launchVfsTool(toolKey: string): Promise<void> {
    const { activeInstanceId: instanceId, activeProfileId: profileId } = this.store.state;
    if (!instanceId || !profileId) throw new Error("Bitte zuerst Instanz und Profil auswählen.");
    const profiles = await this.api.call<ToolProfile[]>("list_tool_profiles");
    const profile = profiles.find((item) => item.tool_key === toolKey);
    const label = profile?.display_name ?? toolKey;
    const activity = this.beginExternalActivity(`${label} wird gestartet …`);
    try {
      const result = await this.api.call<{process_id: number | null}>("launch_tool_profile_vfs", { toolKey, instanceId, profileId });
      this.updateExternalActivity(activity, `${label} ist aktiv`);
      this.store.log("success", `${label} wurde im VFS gestartet.`);
      await this.monitorExternalProcess(result.process_id);
    } finally {
      this.endExternalActivity(activity);
    }
  }

  private async pick(command: "pick_file" | "pick_directory", field: string, title: string): Promise<void> {
    const input = this.root.querySelector<HTMLInputElement>(`[name="${field}"]`); const value = await this.api.call<string | null>(command, { title, defaultPath: input?.value || null });
    if (input && value) { input.value = value; input.dispatchEvent(new Event("change", { bubbles: true })); }
  }

  private suggestModName(sourcePath: string): void {
    const input = this.root.querySelector<HTMLInputElement>("[name=name]"); if (input && !input.dataset.edited) input.value = pathName(sourcePath).replace(/\.(zip|7z|rar|fomod)$/i, "");
  }

  private async deploy(): Promise<void> {
    const { activeInstanceId: instanceId, activeProfileId: profileId } = this.store.state; if (!instanceId || !profileId) throw new Error("Instanz und Profil wählen.");
    const plan = await this.api.call<{operations: unknown[]; warnings: unknown[]}>("execute_staging_plan", { instanceId, profileId });
    this.store.log(plan.warnings.length ? "warning" : "success", `Staging erstellt: ${plan.operations.length} Operationen, ${plan.warnings.length} Warnungen.`);
  }

  private async launch(): Promise<void> {
    const { activeInstanceId: instanceId, activeProfileId: profileId } = this.store.state; if (!instanceId || !profileId) return;
    const activity = this.beginExternalActivity("Skyrim wird gestartet …");
    try {
      const result = await this.api.call<{process_id: number | null}>("launch_game", { instanceId, profileId });
      this.updateExternalActivity(activity, "Skyrim ist aktiv");
      this.store.log("success", "Virtuelles Profil wurde eingehängt und das Spiel gestartet.");
      await this.monitorExternalProcess(result.process_id);
    } finally { this.endExternalActivity(activity); }
  }
  private async runLoot(): Promise<void> {
    const { activeInstanceId: instanceId, activeProfileId: profileId } = this.store.state; if (!instanceId || !profileId) return;
    const activity = this.beginExternalActivity("LOOT sortiert die Plugins und bleibt zur Kontrolle geöffnet …");
    try {
      await this.api.call("launch_loot", { request: { instance_id: instanceId, profile_id: profileId } });
      this.store.log("success", "LOOT wurde beendet und die Sortierung übernommen.");
    } finally { this.endExternalActivity(activity); }
  }
  private async diagnose(): Promise<void> { const { activeInstanceId: instanceId, activeProfileId: profileId } = this.store.state; if (!instanceId || !profileId) return; await this.api.call("build_diagnosis_report", { instanceId, profileId }); this.store.log("success", "Diagnose abgeschlossen."); }

  private async initializeDeepLinks(): Promise<void> {
    const process = async (urls: string[] | null) => { for (const url of urls ?? []) { if (url.startsWith("nxm://")) { this.openDialog(importDialog(this.store.state)); const input = this.root.querySelector<HTMLInputElement>("[name=nexus_url]"); if (input) input.value = url; this.store.log("info", "Nexus-Link empfangen."); } } };
    await process(await getCurrent()); await onOpenUrl((urls) => void process(urls));
  }

  private beginExternalActivity(label: string): symbol {
    const token = Symbol(label); this.externalActivities.set(token, label); this.renderExternalActivity(); return token;
  }
  private updateExternalActivity(token: symbol, label: string): void {
    if (this.externalActivities.has(token)) { this.externalActivities.set(token, label); this.renderExternalActivity(); }
  }
  private endExternalActivity(token: symbol): void { this.externalActivities.delete(token); this.renderExternalActivity(); }
  private renderExternalActivity(): void {
    let overlay = this.root.querySelector<HTMLElement>("#external-process-overlay");
    if (!this.externalActivities.size) { overlay?.remove(); return; }
    if (!overlay) {
      overlay = document.createElement("div"); overlay.id = "external-process-overlay"; overlay.className = "external-process-overlay";
      this.root.querySelector<HTMLElement>(".app-shell")?.append(overlay);
    }
    const label = [...this.externalActivities.values()].at(-1) ?? "Externes Programm ist aktiv";
    overlay.innerHTML = `<div class="external-process-status" role="status"><span class="progress-spinner"></span><strong>${escapeHtml(label)}</strong><small>SLiM-CC wartet, bis das externe Programm beendet wurde.</small></div>`;
  }
  private async monitorExternalProcess(processId: number | null): Promise<void> {
    if (!processId) return;
    while (await this.api.call<boolean>("process_is_running", { processId })) {
      await new Promise((resolve) => window.setTimeout(resolve, 750));
    }
  }

  private openDialog(html: string): void { const host = this.root.querySelector<HTMLElement>("#modal-host"); if (host) host.innerHTML = html; this.root.querySelector<HTMLElement>(".app-shell")?.classList.add("modal-open"); }
  private closeDialog(): void { const host = this.root.querySelector<HTMLElement>("#modal-host"); if (host) host.innerHTML = ""; this.root.querySelector<HTMLElement>(".app-shell")?.classList.remove("modal-open"); }
  private fail(error: unknown): void { const message = error instanceof Error ? error.message : String(error); this.store.log("error", message); this.store.patch({ loading: false }); }
}

function sameDownloads(left: ModDownloadCandidate[], right: ModDownloadCandidate[]): boolean {
  if (left.length !== right.length) return false;
  return left.every((item, index) => {
    const other = right[index];
    return other !== undefined && item.name === other.name && item.path === other.path && item.entry_type === other.entry_type
      && item.importable === other.importable && item.installed === other.installed && item.note === other.note;
  });
}
