use crate::error::{SlimError, SlimResult};
use crate::models::{DeployAction, DeployPlan, DeployTarget};
use crate::paths;
use std::path::Path;

pub fn execute_staging_plan(workspace_root: &Path, plan: &DeployPlan) -> SlimResult<()> {
    let staging_root = paths::vfs_layer_path(workspace_root, &plan.instance_id, &plan.profile_id);
    execute_deploy_plan_at_root(workspace_root, plan, &staging_root)
}

pub fn execute_deploy_plan_at_root(
    workspace_root: &Path,
    plan: &DeployPlan,
    target_root: &Path,
) -> SlimResult<()> {
    if plan.target != DeployTarget::Staging {
        if plan.target != DeployTarget::RealData {
            return Err(SlimError::Safety(
                "deploy executor only accepts Staging or RealData plans".into(),
            ));
        }
    }

    let profile_dir = paths::profile_path(workspace_root, &plan.instance_id, &plan.profile_id);
    std::fs::create_dir_all(&profile_dir)?;
    let manifest_path = profile_dir.join("last_deploy_plan.json");
    paths::guard_inside(workspace_root, &manifest_path)?;
    std::fs::write(&manifest_path, serde_json::to_string_pretty(plan)?)?;

    std::fs::create_dir_all(target_root.parent().unwrap_or(target_root))?;
    if target_root.exists() {
        paths::guard_descendant(target_root, &target_root.join(".guard"))?;
        std::fs::remove_dir_all(target_root)?;
    }
    std::fs::create_dir_all(target_root)?;

    for op in &plan.operations {
        paths::guard_descendant(target_root, &op.target)?;
        if let Some(parent) = op.target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        apply_deploy_action(&op.source, &op.target, &op.action)?;
    }

    Ok(())
}

fn apply_deploy_action(source: &Path, target: &Path, action: &DeployAction) -> SlimResult<()> {
    if !source.exists() {
        return Err(SlimError::NotFound(format!(
            "deploy source file does not exist: {}",
            source.display()
        )));
    }

    match action {
        DeployAction::Copy => {
            std::fs::copy(source, target)?;
        }
        DeployAction::Symlink => {
            if let Err(error) = create_symlink(source, target) {
                std::fs::copy(source, target).map_err(|copy_error| {
                    SlimError::Process(format!(
                        "symlink deployment failed ({error}); fallback copy also failed ({copy_error})"
                    ))
                })?;
            }
        }
        DeployAction::Hardlink => {
            if let Err(error) = std::fs::hard_link(source, target) {
                std::fs::copy(source, target).map_err(|copy_error| {
                    SlimError::Process(format!(
                        "hardlink deployment failed ({error}); fallback copy also failed ({copy_error})"
                    ))
                })?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> SlimResult<()> {
    std::os::unix::fs::symlink(source, target)?;
    Ok(())
}

#[cfg(not(unix))]
fn create_symlink(_source: &Path, _target: &Path) -> SlimResult<()> {
    Err(SlimError::Disabled(
        "symlink deployment is currently only implemented for Unix".into(),
    ))
}
