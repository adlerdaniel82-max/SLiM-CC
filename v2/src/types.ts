export type View = "mods" | "downloads" | "plugins" | "conflicts" | "diagnostics";

export interface GameInstance {
  id: string; name: string; game_type: string; install_path: string; data_path: string;
  game_starter_path: string | null; runner_type: string; wine_prefix: string | null;
}
export interface Profile { id: string; instance_id: string; name: string }
export interface ModRecord {
  id: string; instance_id: string; name: string; version: string | null; size_bytes: number;
  source_path: string; installed_path: string; enabled_default: boolean; tags: string[];
  notes: string | null; rule_type: string | null; rule_target_mod_id: string | null; rule_weight: number;
}
export interface ProfileModEntry {
  mod_id: string; mod_name: string; version: string | null; source_path: string; installed_path: string;
  enabled: boolean; priority: number; plugin_count: number; rule_type: string | null;
  rule_target_mod_id: string | null; rule_weight: number;
}
export interface ProfilePluginEntry {
  plugin_id: string; mod_id: string; mod_name: string; filename: string; plugin_type: string;
  normalized_rel_path: string; mod_enabled: boolean; enabled: boolean; priority: number;
  dependency_count: number; missing_dependency_count: number; dependency_status: string;
}
export interface ModDownloadCandidate {
  name: string; path: string; entry_type: string; importable: boolean; installed: boolean;
  note: string | null; stable?: boolean;
}
export interface ModDependencySummary { mod_id: string; dependency_count: number; missing_count: number; status: string }
export interface ModDependencyStatus {
  id: string; mod_id: string; dependency_type: string; target_mod_id: string | null;
  target_value: string; notes: string; satisfied: boolean; status: string;
}
export interface NexusRequirementStatus {
  id: string; mod_id: string; required_game_domain: string; required_nexus_mod_id: number | null;
  required_name: string; requirement_type: string; source: string; notes: string; fetched_at: string;
  matched_mod_id: string | null; matched_mod_name: string | null; satisfied: boolean; status: string;
}
export interface ModConflictSummary {
  mod_id: string; overwrites_mod_ids: string[]; overwritten_by_mod_ids: string[];
  winning_file_count: number; losing_file_count: number; file_kinds: string[];
}
export interface AppSettings {
  install_path: string | null; data_path: string | null; wine_prefix: string | null;
  loot_executable_path: string | null; mod_download_path: string | null; language: string | null;
  nexus_api_key_configured: boolean; nexus_api_key_masked: string | null;
}
export interface ToolProfile {
  id: string; tool_key: string; display_name: string; executable_path: string | null;
  runner_type: string; arguments: string[]; working_directory: string | null;
  wine_prefix: string | null; log_path: string | null; enabled: boolean;
}
export interface ToolExecutableCandidate {
  tool_key: string; mod_id: string; mod_name: string; executable_name: string;
  virtual_path: string; source_path: string;
}
export interface FomodFileSpec { source: string; destination: string | null; is_folder: boolean; priority?: number }
export interface FomodDependencyGroup { operator: string; dependencies: unknown[] }
export interface FomodOptionPreview {
  id: string; name: string; description: string | null; image_path: string | null;
  default_selected: boolean; file_count: number; condition_flags: Array<{name: string; value: string}>;
  option_type?: string;
  dependencies: FomodDependencyGroup | null; dependency_context_available: boolean; files: FomodFileSpec[];
}
export interface FomodGroupPreview { name: string; selection_mode: string; visible: FomodDependencyGroup | null; options: FomodOptionPreview[] }
export interface FomodStepPreview { name: string; visible: FomodDependencyGroup | null; groups: FomodGroupPreview[] }
export interface FomodSelectionEntry { step_index: number; group_index: number; selected_option_ids: string[] }
export interface FomodPackagePreview {
  preview_id?: string; module_name: string | null; source_path: string; has_fomod: boolean;
  validation_notes: string[]; module_dependencies: FomodDependencyGroup | null;
  required_files: FomodFileSpec[]; conditional_file_installs: unknown[]; steps: FomodStepPreview[];
  saved_selection: { selections: FomodSelectionEntry[] } | null;
}
export interface ImportedModReport { mod_record: ModRecord; files_scanned: number; plugins_discovered: number; copied_into_workspace: boolean }
export interface StatusEvent { id: number; severity: "info" | "success" | "warning" | "error"; message: string; time: Date }
