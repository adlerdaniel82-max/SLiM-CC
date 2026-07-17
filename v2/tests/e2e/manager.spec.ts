import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    let downloads = 0;
    (window as any).__SLIMCC_BACKEND__ = { call: async (command: string) => {
      const data: Record<string, unknown> = {
        list_instances:[{id:"i",name:"Skyrim Special Edition",game_type:"skyrimse",install_path:"/game",data_path:"/game/Data",game_starter_path:"/game/skse64_loader.exe",runner_type:"wine",wine_prefix:null}],
        get_app_settings:{install_path:"/game",data_path:"/game/Data",wine_prefix:null,loot_executable_path:null,mod_download_path:"/downloads",language:"de",nexus_api_key_configured:true,nexus_api_key_masked:"abcd********wxyz"},
        list_profiles:[{id:"p",instance_id:"i",name:"Default"}],
        list_mods:[{id:"m",instance_id:"i",name:"SkyUI",version:"5.2",size_bytes:1024,source_path:"/downloads/SkyUI.7z",installed_path:"/mods/m",enabled_default:true,tags:[],notes:null,rule_type:null,rule_target_mod_id:null,rule_weight:0}],
        list_instance_dependency_summary:[{mod_id:"m",dependency_count:0,missing_count:0,status:"ok"}],
        list_profile_mods:[{mod_id:"m",mod_name:"SkyUI",version:"5.2",source_path:"/downloads/SkyUI.7z",installed_path:"/mods/m",enabled:true,priority:10,plugin_count:1,rule_type:null,rule_target_mod_id:null,rule_weight:0}],
        list_profile_plugins:[{plugin_id:"pl",mod_id:"m",mod_name:"SkyUI",filename:"SkyUI_SE.esp",plugin_type:"esp",normalized_rel_path:"skyui_se.esp",mod_enabled:true,enabled:true,priority:10,dependency_count:0,missing_dependency_count:0,dependency_status:"ok"}], list_mod_conflict_summary:[]
      };
      if(command === "list_mod_download_candidates") { downloads++; return downloads > 1 ? [{name:"New Mod",path:"/downloads/New Mod.7z",entry_type:"archive",importable:true,installed:false,note:null}] : []; }
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
});

test("mod import offers archive, folder and Nexus paths", async ({ page }) => {
  await page.getByRole("button", { name:/Installieren/ }).first().click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByRole("button", { name:"Datei …" })).toBeVisible();
  await expect(page.getByRole("button", { name:"Ordner …" })).toBeVisible();
  await expect(page.getByLabel("Nexus-Download-Link")).toBeVisible();
});
