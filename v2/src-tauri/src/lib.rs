mod app_state;
mod backup;
mod commands;
mod conflicts;
mod db;
mod deploy;
mod diagnostics;
mod error;
mod fomod;
mod instance;
mod models;
mod mods;
mod nexus;
mod paths;
mod plugin_metadata;
mod profiles;
mod scanner;
mod settings;
mod staging;
mod tools;
mod vfs;
mod workspace;

use app_state::AppState;
use tauri::Manager;
#[cfg(desktop)]
use tauri_plugin_deep_link::DeepLinkExt;

pub fn run() {
    let state = AppState::initialize().expect("failed to initialize SLiM-CC state");

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::workspace_info,
            commands::create_instance,
            commands::update_instance,
            commands::delete_instance,
            commands::list_instances,
            commands::list_profiles,
            commands::list_mods,
            commands::update_mod,
            commands::delete_mod,
            commands::list_mod_dependencies,
            commands::list_instance_dependency_summary,
            commands::update_mod_dependencies,
            commands::set_manual_dependency_satisfied,
            commands::list_nexus_mod_links,
            commands::upsert_nexus_mod_link,
            commands::parse_nexus_source_link,
            commands::analyze_nexus_collection,
            commands::download_nexus_file,
            commands::list_nexus_requirements,
            commands::list_nexus_requirement_status,
            commands::list_nexus_mod_cache,
            commands::validate_nexus_api,
            commands::sync_nexus_mod,
            commands::create_profile,
            commands::update_profile,
            commands::delete_profile,
            commands::list_profile_mods,
            commands::update_profile_mods,
            commands::list_profile_plugins,
            commands::update_profile_plugins,
            commands::import_mod_folder,
            commands::preview_fomod_package,
            commands::discard_fomod_preview,
            commands::preview_mod_fomod,
            commands::launch_tool,
            commands::launch_game,
            commands::prepare_profile_vfs,
            commands::unmount_profile_vfs,
            commands::profile_vfs_status,
            commands::list_tool_profiles,
            commands::upsert_tool_profile,
            commands::launch_tool_profile,
            commands::validate_tool_profiles,
            commands::list_recent_tool_runs,
            commands::scan_mod_files,
            commands::build_dry_run_plan,
            commands::execute_staging_plan,
            commands::execute_real_deploy_plan,
            commands::list_conflicts,
            commands::list_mod_conflict_summary,
            commands::build_diagnosis_report,
            commands::import_diagnostic_logs,
            commands::compare_profiles,
            commands::list_backup_snapshots,
            commands::restore_backup_snapshot,
            commands::pick_directory,
            commands::pick_file,
            commands::pick_save_file,
            commands::get_app_settings,
            commands::update_app_settings,
            commands::update_nexus_api_key,
            commands::list_mod_download_candidates,
            commands::open_external_url,
            commands::preview_loot_sort,
            commands::launch_loot,
        ])
        .setup(|app| {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            {
                app.deep_link().register_all()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running SLiM-CC");
}
