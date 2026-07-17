import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    let downloads = 0;
    (window as any).__commands = [];
    (window as any).__SLIMCC_BACKEND__ = { call: async (command: string, args?: Record<string, unknown>) => {
      (window as any).__commands.push(command);
      const data: Record<string, unknown> = {
        list_instances:[{id:"i",name:"Skyrim Special Edition",game_type:"skyrimse",install_path:"/game",data_path:"/game/Data",game_starter_path:"/game/skse64_loader.exe",runner_type:"wine",wine_prefix:null}],
        get_app_settings:{install_path:"/game",data_path:"/game/Data",wine_prefix:null,loot_executable_path:null,mod_download_path:"/downloads",language:"de",nexus_api_key_configured:true,nexus_api_key_masked:"abcd********wxyz"},
        list_profiles:[{id:"p",instance_id:"i",name:"Default"}],
        list_mods:[{id:"m",instance_id:"i",name:"SkyUI",version:"5.2",size_bytes:1024,source_path:"/downloads/SkyUI.7z",installed_path:"/mods/m",enabled_default:true,tags:[],notes:null,rule_type:null,rule_target_mod_id:null,rule_weight:0}],
        list_instance_dependency_summary:[{mod_id:"m",dependency_count:2,missing_count:2,status:"missing"}],
        list_mod_dependencies:[{id:"dep",mod_id:"m",dependency_type:"plugin",target_mod_id:null,target_value:"MissingMaster.esp",notes:"Plugin-Master",satisfied:false,status:"missing"}],
        list_nexus_requirement_status:[{id:"req",mod_id:"m",required_game_domain:"skyrimspecialedition",required_nexus_mod_id:32444,required_name:"Address Library for SKSE Plugins",requirement_type:"required",source:"nexus",notes:"Benötigte Laufzeitbibliothek",fetched_at:"2026-07-17",matched_mod_id:null,matched_mod_name:null,satisfied:false,status:"missing"}],
        list_profile_mods:[{mod_id:"m",mod_name:"SkyUI",version:"5.2",source_path:"/downloads/SkyUI.7z",installed_path:"/mods/m",enabled:true,priority:10,plugin_count:1,rule_type:null,rule_target_mod_id:null,rule_weight:0}],
        list_profile_plugins:[{plugin_id:"pl",mod_id:"m",mod_name:"SkyUI",filename:"SkyUI_SE.esp",plugin_type:"esp",normalized_rel_path:"skyui_se.esp",mod_enabled:true,enabled:true,priority:10,dependency_count:0,missing_dependency_count:0,dependency_status:"ok"}], list_mod_conflict_summary:[]
      };
      if(command === "list_mod_download_candidates") { downloads++; const length = (window as any).__extraDownload ? 41 : 40; return downloads > 1 ? Array.from({length}, (_, index) => ({name:index === 0 ? "New Mod" : `New Mod ${index}`,path:`/downloads/New Mod ${index}.7z`,entry_type:"archive",importable:true,installed:false,note:null})) : []; }
      if(command === "preview_fomod_package") {
        await new Promise((resolve) => setTimeout(resolve, 500));
        if(String(args?.packageRoot).includes("fomod")) return {has_fomod:true,source_path:"/tmp/preview",module_name:"Test Installer",validation_notes:[],module_dependencies:null,required_files:[],conditional_file_installs:[],saved_selection:null,steps:[{name:"Komponenten",visible:null,groups:[{name:"Variante",selection_mode:"SelectExactlyOne",visible:null,options:[{id:"main",name:"Hauptdatei",description:"Testauswahl",image_path:null,default_selected:true,file_count:1,condition_flags:[],dependencies:null,dependency_context_available:true,files:[]}]}]}]};
        return {has_fomod:false,source_path:"/tmp/preview",module_name:null,steps:[],validation_notes:[]};
      }
      if(command === "import_mod_folder") return {mod_record:{name:"Test Mod"},files_scanned:1,plugins_discovered:0};
      return data[command];
    }};
  });
  await page.goto("/");
});

test("compact manager exposes primary workflows", async ({ page }) => {
  await expect(page.getByText("SkyUI", { exact:true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name:/Starten/ })).toBeVisible();
  await page.getByRole("button", { name:"Downloads" }).first().click();
  await page.getByRole("button", { name:/Aktualisieren/ }).last().click();
  await expect(page.getByText("New Mod", { exact:true })).toBeVisible();
  const bounds = await page.locator(".downloads").evaluate((element) => ({ clientHeight:element.clientHeight, scrollHeight:element.scrollHeight, bottom:element.getBoundingClientRect().bottom, viewport:window.innerHeight }));
  expect(bounds.scrollHeight).toBeGreaterThan(bounds.clientHeight);
  expect(bounds.bottom).toBeLessThanOrEqual(bounds.viewport);
  await expect(page.locator(".download-card small")).toHaveCount(0);
  await page.locator(".downloads").evaluate((element) => { element.scrollTop = 180; });
  await page.evaluate(() => { (window as any).__extraDownload = true; });
  await page.getByRole("button", { name:/Aktualisieren/ }).last().click();
  expect(await page.locator(".downloads").evaluate((element) => element.scrollTop)).toBe(180);
});

test("large imports show responsive progress", async ({ page }) => {
  await page.getByRole("button", { name:/Installieren/ }).first().click();
  await page.getByLabel("Quelle").fill("/downloads/large-mod.7z");
  await page.getByRole("button", { name:"Weiter" }).click();
  await expect(page.getByRole("status")).toContainText("Archiv wird entpackt");
  await expect(page.getByRole("status")).toContainText("SLiM-CC arbeitet weiter");
  await expect(page.getByRole("dialog")).toBeHidden();
});

test("automatic download refresh preserves menus, focus and text selection", async ({ page }) => {
  await page.getByText("Datei", { exact:true }).click();
  await expect(page.locator("details.menu").first()).toHaveAttribute("open", "");
  await page.waitForTimeout(4500);
  await expect(page.locator("details.menu").first()).toHaveAttribute("open", "");
  await page.getByRole("button", { name:/Mod installieren/ }).first().click();
  const input = page.getByLabel("Nexus-Download-Link");
  await input.fill("nxm://selection-survives-refresh");
  await input.evaluate((element: HTMLInputElement) => { element.focus(); element.setSelectionRange(6, 15); });
  await page.waitForTimeout(4500);
  await expect(input).toBeFocused();
  await expect(input).toHaveJSProperty("selectionStart", 6);
  await expect(input).toHaveJSProperty("selectionEnd", 15);
});

test("FOMOD packages open their installer dialog", async ({ page }) => {
  await page.getByRole("button", { name:/Installieren/ }).first().click();
  await page.getByLabel("Quelle").fill("/downloads/test-fomod.7z");
  await page.getByRole("button", { name:"Weiter" }).click();
  await expect(page.getByRole("dialog", { name:/FOMOD · Test Installer/ })).toBeVisible();
  await expect(page.getByText("Hauptdatei", { exact:true })).toBeVisible();
});

test("mod import offers archive, folder and Nexus paths", async ({ page }) => {
  await page.getByRole("button", { name:/Installieren/ }).first().click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByRole("button", { name:"Datei …" })).toBeVisible();
  await expect(page.getByRole("button", { name:"Ordner …" })).toBeVisible();
  await expect(page.getByLabel("Nexus-Download-Link")).toBeVisible();
  await page.getByLabel("Nexus-Download-Link").fill("nxm://temporary-test-link");
  await page.waitForTimeout(4500);
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByLabel("Nexus-Download-Link")).toHaveValue("nxm://temporary-test-link");
});

test("installed mods expose a safe delete action in the context menu", async ({ page }) => {
  await page.getByText("SkyUI", { exact:true }).first().click({ button:"right" });
  await expect(page.getByRole("menuitem", { name:"Mod löschen …" })).toBeVisible();
  await page.waitForTimeout(4500);
  await expect(page.getByRole("menuitem", { name:"Mod löschen …" })).toBeVisible();
  await page.getByRole("menuitem", { name:"Mod löschen …" }).click();
  await expect(page.getByRole("dialog", { name:"Mod löschen" })).toBeVisible();
  await expect(page.getByText(/Downloadarchiv bleibt erhalten/)).toBeVisible();
  await page.getByRole("button", { name:"Mod löschen" }).click();
  await expect(page.getByRole("dialog")).toBeHidden();
  const commands = await page.evaluate(() => (window as any).__commands as string[]);
  expect(commands).toContain("delete_mod");
});

test("mod details name missing dependencies and provide Nexus links", async ({ page }) => {
  await page.getByText("SkyUI", { exact:true }).first().click({ button:"right" });
  await page.getByRole("menuitem", { name:"Details …" }).click();
  await expect(page.getByRole("dialog", { name:/Abhängigkeiten · SkyUI/ })).toBeVisible();
  await expect(page.getByText("MissingMaster.esp", { exact:true })).toBeVisible();
  await expect(page.getByText("Address Library for SKSE Plugins", { exact:true })).toBeVisible();
  await page.getByRole("button", { name:"Nexus öffnen" }).click();
  const commands = await page.evaluate(() => (window as any).__commands as string[]);
  expect(commands).toContain("open_external_url");
});

test("instances and profiles expose confirmed delete actions", async ({ page }) => {
  await page.getByText("Datei", { exact:true }).click();
  await page.getByRole("button", { name:"Instanzen verwalten …" }).click();
  await page.getByRole("button", { name:"Löschen …" }).click();
  await expect(page.getByRole("dialog", { name:"Instanz löschen" })).toBeVisible();
  await page.getByRole("button", { name:"Instanz löschen" }).click();
  await page.getByText("Werkzeuge", { exact:true }).click();
  await page.getByRole("button", { name:"Profile …" }).click();
  await page.getByRole("button", { name:"Löschen …" }).click();
  await expect(page.getByRole("dialog", { name:"Profil löschen" })).toBeVisible();
  await page.getByRole("button", { name:"Profil löschen" }).click();
  const commands = await page.evaluate(() => (window as any).__commands as string[]);
  expect(commands).toContain("delete_instance"); expect(commands).toContain("delete_profile");
});

test("about dialog includes banner and copyright", async ({ page }) => {
  await page.getByText("Hilfe", { exact:true }).click();
  await page.getByRole("button", { name:"Über SLiM-CC" }).click();
  await expect(page.getByRole("dialog", { name:"Über SLiM-CC v2" })).toBeVisible();
  await expect(page.getByRole("img", { name:"SLiM-CC" })).toBeVisible();
  await expect(page.getByText("(c)Schnüddel Media - https://schnueddels.de - Daniel Adler", { exact:true })).toBeVisible();
});
