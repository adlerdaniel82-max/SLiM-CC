import { beforeEach, describe, expect, it } from "vitest";
import { SlimApp } from "../../src/app";
import type { Backend } from "../../src/core/api";

class FakeBackend implements Backend {
  downloadCalls = 0;
  async call<T>(command: string): Promise<T> {
    const values: Record<string, unknown> = {
      list_instances: [{ id:"i", name:"Skyrim SE", game_type:"skyrimse", install_path:"/game", data_path:"/game/Data", game_starter_path:null, runner_type:"wine", wine_prefix:null }],
      get_app_settings: { install_path:"/game", data_path:"/game/Data", wine_prefix:null, loot_executable_path:null, mod_download_path:"/downloads", language:"de", nexus_api_key_configured:false, nexus_api_key_masked:null },
      list_profiles: [{ id:"p", instance_id:"i", name:"Default" }], list_mods: [], list_instance_dependency_summary: [],
      list_profile_mods: [], list_profile_plugins: [], list_mod_conflict_summary: []
    };
    if (command === "list_mod_download_candidates") { this.downloadCalls += 1; return (this.downloadCalls > 1 ? [{ name:"SkyUI", path:"/downloads/SkyUI.7z", entry_type:"archive", importable:true, installed:false, note:null }] : []) as T; }
    return values[command] as T;
  }
}

describe("SLiM-CC application integration", () => {
  beforeEach(() => { document.body.innerHTML = `<div id="app"></div>`; });
  it("loads the shell and refreshes newly arrived downloads", async () => {
    const fake = new FakeBackend(); const root = document.querySelector<HTMLElement>("#app")!;
    await new SlimApp(root, fake).start();
    (root.querySelector("[data-view=downloads]") as HTMLButtonElement).click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(root.textContent).toContain("SkyUI"); expect(fake.downloadCalls).toBeGreaterThan(1);
  });
});
