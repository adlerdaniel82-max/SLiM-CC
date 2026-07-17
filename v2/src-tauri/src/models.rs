use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub workspace_root: PathBuf,
    pub database_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsStatus {
    pub available: bool,
    pub mounted: bool,
    pub instance_id: String,
    pub profile_id: String,
    pub mount_path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInstance {
    pub id: String,
    pub name: String,
    pub game_type: String,
    pub install_path: PathBuf,
    pub data_path: PathBuf,
    pub game_starter_path: Option<PathBuf>,
    pub runner_type: String,
    pub wine_prefix: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInstanceRequest {
    pub name: String,
    pub install_path: PathBuf,
    pub data_path: PathBuf,
    pub game_starter_path: Option<PathBuf>,
    pub runner_type: String,
    pub wine_prefix: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInstanceRequest {
    pub id: String,
    pub name: String,
    pub install_path: PathBuf,
    pub data_path: PathBuf,
    pub game_starter_path: Option<PathBuf>,
    pub runner_type: String,
    pub wine_prefix: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub instance_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProfileRequest {
    pub instance_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    pub id: String,
    pub instance_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModRecord {
    pub id: String,
    pub instance_id: String,
    pub name: String,
    pub version: Option<String>,
    pub size_bytes: i64,
    pub source_path: PathBuf,
    pub installed_path: PathBuf,
    pub enabled_default: bool,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub rule_type: Option<String>,
    pub rule_target_mod_id: Option<String>,
    pub rule_weight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDownloadCandidate {
    pub name: String,
    pub path: PathBuf,
    pub entry_type: String,
    pub importable: bool,
    pub installed: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDependencyStatus {
    pub id: String,
    pub mod_id: String,
    pub dependency_type: String,
    pub target_mod_id: Option<String>,
    pub target_value: String,
    pub notes: String,
    pub satisfied: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDependencySummary {
    pub mod_id: String,
    pub dependency_count: i64,
    pub missing_count: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusModLink {
    pub mod_id: String,
    pub game_domain: String,
    pub nexus_mod_id: i64,
    pub nexus_file_id: Option<i64>,
    pub source_url: String,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNexusModLinkRequest {
    pub mod_id: String,
    pub game_domain: String,
    pub nexus_mod_id: Option<i64>,
    pub nexus_file_id: Option<i64>,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusSourceLink {
    pub game_domain: String,
    pub nexus_mod_id: i64,
    pub nexus_file_id: Option<i64>,
    pub sanitized_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusCollectionItem {
    pub title: String,
    pub url: String,
    pub game_domain: Option<String>,
    pub nexus_mod_id: Option<i64>,
    pub nexus_file_id: Option<i64>,
    pub item_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusCollectionAnalysis {
    pub source_url: String,
    pub collection_title: Option<String>,
    pub collection_description: Option<String>,
    pub resolver: String,
    pub items: Vec<NexusCollectionItem>,
    pub unresolved_links: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeNexusCollectionRequest {
    pub collection_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusDownloadRequest {
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusDownloadResult {
    pub path: PathBuf,
    pub file_name: String,
    pub bytes_written: u64,
    pub game_domain: String,
    pub nexus_mod_id: i64,
    pub nexus_file_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusRequirement {
    pub id: String,
    pub mod_id: String,
    pub required_game_domain: String,
    pub required_nexus_mod_id: Option<i64>,
    pub required_name: String,
    pub requirement_type: String,
    pub source: String,
    pub notes: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusRequirementStatus {
    pub id: String,
    pub mod_id: String,
    pub required_game_domain: String,
    pub required_nexus_mod_id: Option<i64>,
    pub required_name: String,
    pub requirement_type: String,
    pub source: String,
    pub notes: String,
    pub fetched_at: String,
    pub matched_mod_id: Option<String>,
    pub matched_mod_name: Option<String>,
    pub satisfied: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusApiStatus {
    pub configured: bool,
    pub user_id: Option<i64>,
    pub name: Option<String>,
    pub is_premium: Option<bool>,
    pub is_supporter: Option<bool>,
    pub hourly_remaining: Option<i64>,
    pub daily_remaining: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusModCache {
    pub mod_id: String,
    pub game_domain: String,
    pub nexus_mod_id: i64,
    pub name: String,
    pub version: String,
    pub updated_time: Option<String>,
    pub endorsement_count: Option<i64>,
    pub mod_downloads: Option<i64>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusSyncResult {
    pub link: NexusModLink,
    pub mod_cache: NexusModCache,
    pub files_cached: usize,
    pub requirements_cached: usize,
    pub hourly_remaining: Option<i64>,
    pub daily_remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModDependencyEntry {
    pub dependency_type: String,
    pub target_mod_id: Option<String>,
    pub target_value: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModDependenciesRequest {
    pub mod_id: String,
    pub entries: Vec<UpdateModDependencyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModRequest {
    pub id: String,
    pub name: String,
    pub enabled_default: bool,
    pub tags: Vec<String>,
    pub notes: String,
    pub rule_type: String,
    pub rule_target_mod_id: Option<String>,
    pub rule_weight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportModFolderRequest {
    pub instance_id: String,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub target_mod_id: Option<String>,
    #[serde(default)]
    pub preview_source_path: Option<PathBuf>,
    pub name: String,
    pub source_path: PathBuf,
    pub copy_into_workspace: bool,
    pub fomod_selection: Option<FomodSelectionRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedModReport {
    pub mod_record: ModRecord,
    pub files_scanned: usize,
    pub plugins_discovered: usize,
    pub copied_into_workspace: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanModFilesRequest {
    pub mod_id: String,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModFile {
    pub mod_id: String,
    pub original_rel_path: String,
    pub normalized_rel_path: String,
    pub abs_source_path: PathBuf,
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileModEntry {
    pub mod_id: String,
    pub mod_name: String,
    pub version: Option<String>,
    pub source_path: PathBuf,
    pub installed_path: PathBuf,
    pub enabled: bool,
    pub priority: i64,
    pub plugin_count: i64,
    pub rule_type: Option<String>,
    pub rule_target_mod_id: Option<String>,
    pub rule_weight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePluginEntry {
    pub plugin_id: String,
    pub mod_id: String,
    pub mod_name: String,
    pub filename: String,
    pub plugin_type: String,
    pub normalized_rel_path: String,
    pub mod_enabled: bool,
    pub enabled: bool,
    pub priority: i64,
    pub dependency_count: i64,
    pub missing_dependency_count: i64,
    pub dependency_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfileModEntry {
    pub mod_id: String,
    pub enabled: bool,
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfileModsRequest {
    pub profile_id: String,
    pub entries: Vec<UpdateProfileModEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfilePluginEntry {
    pub plugin_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfilePluginsRequest {
    pub profile_id: String,
    pub entries: Vec<UpdateProfilePluginEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictEntry {
    pub normalized_rel_path: String,
    pub winner_mod_id: String,
    pub candidates: Vec<ConflictCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictCandidate {
    pub mod_id: String,
    pub priority: i64,
    pub original_rel_path: String,
    pub abs_source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModConflictSummary {
    pub mod_id: String,
    pub overwrites_mod_ids: Vec<String>,
    pub overwritten_by_mod_ids: Vec<String>,
    pub winning_file_count: i64,
    pub losing_file_count: i64,
    pub file_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeployTarget {
    DryRun,
    Staging,
    RealData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeployAction {
    Copy,
    Symlink,
    Hardlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployOperation {
    pub action: DeployAction,
    pub source: PathBuf,
    pub target: PathBuf,
    pub mod_id: String,
    pub original_rel_path: String,
    pub normalized_rel_path: String,
    pub conflict_winner: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployWarning {
    pub mod_id: String,
    pub dependency_id: String,
    pub dependency_type: String,
    pub target_value: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployPlan {
    pub instance_id: String,
    pub profile_id: String,
    pub target: DeployTarget,
    pub generated_at: String,
    pub operations: Vec<DeployOperation>,
    pub conflicts: Vec<ConflictEntry>,
    pub warnings: Vec<DeployWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSnapshot {
    pub id: String,
    pub instance_id: String,
    pub profile_id: Option<String>,
    pub label: String,
    pub source_root: PathBuf,
    pub snapshot_root: PathBuf,
    pub created_at: String,
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProfileValidation {
    pub tool_key: String,
    pub display_name: String,
    pub enabled: bool,
    pub executable_configured: bool,
    pub executable_exists: bool,
    pub working_directory_exists: bool,
    pub wine_prefix_exists: bool,
    pub effective_log_path: Option<PathBuf>,
    pub status: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRunRecord {
    pub id: String,
    pub tool_key: String,
    pub display_name: String,
    pub program: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticFinding {
    pub source: String,
    pub severity: String,
    pub category: String,
    pub mod_id: Option<String>,
    pub plugin: Option<String>,
    pub message: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDiagnosticFinding {
    pub id: String,
    pub instance_id: Option<String>,
    pub source: String,
    pub severity: String,
    pub category: String,
    pub mod_id: Option<String>,
    pub plugin: Option<String>,
    pub message: String,
    pub evidence: String,
    pub found_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspectModScore {
    pub mod_id: String,
    pub score: i64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosisReport {
    pub instance_id: String,
    pub profile_id: String,
    pub generated_at: String,
    pub mods: Vec<ProfileModEntry>,
    pub dependencies: Vec<ModDependencySummary>,
    pub conflicts: Vec<ModConflictSummary>,
    pub deploy_warnings: Vec<DeployWarning>,
    pub tool_validations: Vec<ToolProfileValidation>,
    pub recent_tool_runs: Vec<ToolRunRecord>,
    pub findings: Vec<DiagnosticFinding>,
    pub stored_findings: Vec<StoredDiagnosticFinding>,
    pub suspect_mods: Vec<SuspectModScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileComparison {
    pub instance_id: String,
    pub base_profile_id: String,
    pub compare_profile_id: String,
    pub added_mods: Vec<ProfileModEntry>,
    pub removed_mods: Vec<ProfileModEntry>,
    pub changed_priorities: Vec<ProfilePriorityChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePriorityChange {
    pub mod_id: String,
    pub mod_name: String,
    pub base_priority: i64,
    pub compare_priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootSortPreview {
    pub instance_id: String,
    pub profile_id: String,
    pub loot_executable_path: Option<PathBuf>,
    pub notes: Vec<String>,
    pub current_order: Vec<ProfileModEntry>,
    pub proposed_order: Vec<ProfileModEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchLootRequest {
    pub instance_id: String,
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootLaunchResult {
    pub instance_id: String,
    pub game_identifier: String,
    pub executable_path: PathBuf,
    pub args: Vec<String>,
    pub process_id: u32,
    pub synced_mods: usize,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodSelectionRequest {
    pub selections: Vec<FomodSelectionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodSelectionEntry {
    pub step_index: usize,
    pub group_index: usize,
    pub selected_option_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodPackagePreview {
    pub module_name: Option<String>,
    pub source_path: PathBuf,
    pub has_fomod: bool,
    pub validation_notes: Vec<String>,
    pub module_dependencies: Option<FomodDependencyGroup>,
    pub required_files: Vec<FomodFileSpec>,
    pub conditional_file_installs: Vec<FomodConditionalInstall>,
    pub steps: Vec<FomodStepPreview>,
    pub saved_selection: Option<FomodSelectionRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodStepPreview {
    pub name: String,
    pub visible: Option<FomodDependencyGroup>,
    pub groups: Vec<FomodGroupPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodGroupPreview {
    pub name: String,
    pub selection_mode: String,
    pub visible: Option<FomodDependencyGroup>,
    pub options: Vec<FomodOptionPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodOptionPreview {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub image_path: Option<String>,
    pub default_selected: bool,
    pub option_type: String,
    pub file_count: usize,
    pub condition_flags: Vec<FomodFlagAssignment>,
    pub dependencies: Option<FomodDependencyGroup>,
    pub dependency_context_available: bool,
    pub files: Vec<FomodFileSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodFileSpec {
    pub source: String,
    pub destination: Option<String>,
    pub is_folder: bool,
    #[serde(default)]
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodFlagAssignment {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodConditionalInstall {
    pub dependencies: Option<FomodDependencyGroup>,
    pub files: Vec<FomodFileSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FomodDependencyGroup {
    pub operator: String,
    pub dependencies: Vec<FomodDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FomodDependency {
    Flag {
        flag: String,
        value: String,
    },
    File {
        file: String,
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        satisfied: Option<bool>,
    },
    Group(FomodDependencyGroup),
}
