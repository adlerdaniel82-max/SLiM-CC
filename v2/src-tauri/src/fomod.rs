use crate::error::{SlimError, SlimResult};
use crate::models::{
    FomodConditionalInstall, FomodDependency, FomodDependencyGroup, FomodFileSpec,
    FomodFlagAssignment, FomodGroupPreview, FomodOptionPreview, FomodPackagePreview,
    FomodSelectionEntry, FomodSelectionRequest, FomodStepPreview,
};
use encoding_rs::{UTF_16BE, UTF_16LE};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn inspect_package_with_context(
    package_root: &Path,
    saved_selection: Option<FomodSelectionRequest>,
    _dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<FomodPackagePreview> {
    let Some(config_path) = find_module_config_path(package_root) else {
        return Ok(FomodPackagePreview {
            module_name: None,
            source_path: package_root.to_path_buf(),
            has_fomod: false,
            validation_notes: Vec::new(),
            module_dependencies: None,
            required_files: Vec::new(),
            conditional_file_installs: Vec::new(),
            steps: Vec::new(),
            saved_selection,
        });
    };

    let effective_root = resolve_package_root(package_root, &config_path);

    let xml_bytes = fs::read(&config_path)?;
    let xml = decode_fomod_xml(&xml_bytes);
    let doc = Document::parse(&xml).map_err(|err| SlimError::InvalidPath(err.to_string()))?;
    let root = doc.root_element();

    let mut preview = FomodPackagePreview {
        module_name: child_text(root, "moduleName"),
        source_path: effective_root.clone(),
        has_fomod: true,
        validation_notes: Vec::new(),
        module_dependencies: parse_optional_dependency_group(
            root,
            "moduleDependencies",
            _dependency_context,
        )?,
        required_files: parse_file_specs(root, "requiredInstallFiles")?,
        conditional_file_installs: parse_conditional_installs(root, _dependency_context)?,
        steps: parse_steps(root, _dependency_context)?,
        saved_selection,
    };

    absolutize_option_image_paths(&mut preview, &effective_root);
    Ok(with_validation_notes(preview))
}

#[derive(Debug, Clone, Default)]
pub struct FomodDependencyContext {
    pub available_files: BTreeSet<String>,
    pub search_roots: Vec<PathBuf>,
}

impl FomodDependencyContext {
    pub fn from_roots<I>(available_files: BTreeSet<String>, roots: I) -> Self
    where
        I: IntoIterator<Item = PathBuf>,
    {
        Self {
            available_files,
            search_roots: roots.into_iter().collect(),
        }
    }
}

fn resolve_package_root(package_root: &Path, config_path: &Path) -> PathBuf {
    let config_dir = config_path.parent().unwrap_or(package_root);
    if config_dir
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("fomod"))
    {
        config_dir.parent().unwrap_or(package_root).to_path_buf()
    } else {
        config_dir.to_path_buf()
    }
}

fn find_module_config_path(package_root: &Path) -> Option<std::path::PathBuf> {
    let direct_candidates = [
        package_root.join("fomod").join("ModuleConfig.xml"),
        package_root.join("FOMOD").join("ModuleConfig.xml"),
        package_root.join("ModuleConfig.xml"),
    ];
    for candidate in direct_candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let mut best_candidate: Option<(usize, bool, PathBuf)> = None;
    for entry in WalkDir::new(package_root).follow_links(false).max_depth(12) {
        let entry = entry.ok()?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case("ModuleConfig.xml")
        {
            let path = entry.path().to_path_buf();
            let depth = path
                .strip_prefix(package_root)
                .ok()
                .map(|relative| relative.components().count())
                .unwrap_or(usize::MAX);
            let parent_is_fomod = path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("fomod"));
            let candidate = (depth, !parent_is_fomod, path);
            if best_candidate
                .as_ref()
                .is_none_or(|current| candidate < *current)
            {
                best_candidate = Some(candidate);
            }
        }
    }

    best_candidate.map(|(_, _, path)| path)
}

fn absolutize_option_image_paths(preview: &mut FomodPackagePreview, package_root: &Path) {
    for step in &mut preview.steps {
        for group in &mut step.groups {
            for option in &mut group.options {
                let Some(image_path) = option.image_path.as_deref() else {
                    continue;
                };
                let normalized = image_path.replace('\\', "/");
                let path = Path::new(&normalized);
                let absolute = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    package_root.join(path)
                };
                option.image_path = Some(absolute.to_string_lossy().into_owned());
            }
        }
    }
}

pub fn default_selection_with_context(
    preview: &FomodPackagePreview,
    dependency_context: Option<&FomodDependencyContext>,
) -> FomodSelectionRequest {
    let resolver = SelectionResolver::new(preview, None, dependency_context);
    resolver.default_selection()
}

pub fn merge_selection_with_context(
    preview: &FomodPackagePreview,
    request: Option<FomodSelectionRequest>,
    dependency_context: Option<&FomodDependencyContext>,
) -> FomodSelectionRequest {
    let base =
        request.unwrap_or_else(|| default_selection_with_context(preview, dependency_context));
    normalize_selection_with_context(preview, base, dependency_context)
}

pub fn files_for_selection_with_context(
    preview: &FomodPackagePreview,
    selection: &FomodSelectionRequest,
    dependency_context: Option<&FomodDependencyContext>,
) -> Vec<FomodFileSpec> {
    let resolver = SelectionResolver::new(preview, Some(selection), dependency_context);
    resolver.resolve_files()
}

pub fn install_selected_files_with_context(
    package_root: &Path,
    target_root: &Path,
    preview: &FomodPackagePreview,
    selection: &FomodSelectionRequest,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<usize> {
    if !module_dependencies_satisfied(preview, dependency_context) {
        return Err(SlimError::Safety(
            "FOMOD module dependencies are not satisfied; installation was cancelled".into(),
        ));
    }
    let mut files = files_for_selection_with_context(preview, selection, dependency_context);
    files.sort_by_key(|spec| spec.priority);
    let mut copied = 0usize;
    for spec in files {
        copied += copy_spec(package_root, target_root, &spec)?;
    }
    Ok(copied)
}

pub fn module_dependencies_satisfied(
    preview: &FomodPackagePreview,
    dependency_context: Option<&FomodDependencyContext>,
) -> bool {
    preview
        .module_dependencies
        .as_ref()
        .map(|group| evaluate_dependency_group(group, &BTreeMap::new(), dependency_context))
        .unwrap_or(true)
}

#[derive(Debug, Clone)]
struct SelectionResolver<'a> {
    preview: &'a FomodPackagePreview,
    selection_map: BTreeMap<(usize, usize), Vec<String>>,
    flags: BTreeMap<String, String>,
    dependency_context: Option<&'a FomodDependencyContext>,
}

impl<'a> SelectionResolver<'a> {
    fn new(
        preview: &'a FomodPackagePreview,
        selection: Option<&'a FomodSelectionRequest>,
        dependency_context: Option<&'a FomodDependencyContext>,
    ) -> Self {
        let mut selection_map = BTreeMap::new();
        if let Some(selection) = selection {
            for entry in &selection.selections {
                selection_map.insert(
                    (entry.step_index, entry.group_index),
                    entry.selected_option_ids.clone(),
                );
            }
        }

        Self {
            preview,
            selection_map,
            flags: BTreeMap::new(),
            dependency_context,
        }
    }

    fn default_selection(mut self) -> FomodSelectionRequest {
        let mut selections = Vec::new();
        for (step_index, step) in self.preview.steps.iter().enumerate() {
            if !self.step_visible(step) {
                continue;
            }

            for (group_index, group) in step.groups.iter().enumerate() {
                if !self.group_visible(group) {
                    continue;
                }
                let selected_option_ids = self.default_group_selection(group);
                self.apply_selected_flags(group, &selected_option_ids);
                selections.push(FomodSelectionEntry {
                    step_index,
                    group_index,
                    selected_option_ids,
                });
            }
        }
        FomodSelectionRequest { selections }
    }

    fn resolve_files(mut self) -> Vec<FomodFileSpec> {
        let mut files = self.preview.required_files.clone();

        if let Some(group) = &self.preview.module_dependencies {
            if !evaluate_dependency_group(group, &self.flags, self.dependency_context) {
                return files;
            }
        }

        for (step_index, step) in self.preview.steps.iter().enumerate() {
            if !self.step_visible(step) {
                continue;
            }

            for (group_index, group) in step.groups.iter().enumerate() {
                if !self.group_visible(group) {
                    continue;
                }
                let mut selected = self
                    .selection_map
                    .get(&(step_index, group_index))
                    .cloned()
                    .unwrap_or_else(|| self.default_group_selection(group));
                self.normalize_group_selection(group, &mut selected);
                self.apply_selected_flags(group, &selected);

                for option in &group.options {
                    if selected.iter().any(|id| id == &option.id)
                        && self.option_is_available(option)
                    {
                        files.extend(option.files.clone());
                    }
                }
            }
        }

        for install in &self.preview.conditional_file_installs {
            if install
                .dependencies
                .as_ref()
                .map(|group| evaluate_dependency_group(group, &self.flags, self.dependency_context))
                .unwrap_or(true)
            {
                files.extend(install.files.clone());
            }
        }

        files
    }

    fn normalize_selection(mut self) -> FomodSelectionRequest {
        let mut selections = Vec::new();

        for (step_index, step) in self.preview.steps.iter().enumerate() {
            if !self.step_visible(step) {
                continue;
            }

            for (group_index, group) in step.groups.iter().enumerate() {
                if !self.group_visible(group) {
                    continue;
                }

                let mut selected = self
                    .selection_map
                    .get(&(step_index, group_index))
                    .cloned()
                    .unwrap_or_else(|| self.default_group_selection(group));
                self.normalize_group_selection(group, &mut selected);
                self.apply_selected_flags(group, &selected);
                selections.push(FomodSelectionEntry {
                    step_index,
                    group_index,
                    selected_option_ids: selected,
                });
            }
        }

        FomodSelectionRequest { selections }
    }

    fn step_visible(&self, step: &FomodStepPreview) -> bool {
        step.visible
            .as_ref()
            .map(|group| evaluate_dependency_group(group, &self.flags, self.dependency_context))
            .unwrap_or(true)
    }

    fn apply_selected_flags(&mut self, group: &FomodGroupPreview, selected: &[String]) {
        for option in &group.options {
            if selected.iter().any(|id| id == &option.id) {
                for assignment in &option.condition_flags {
                    self.flags
                        .insert(assignment.name.clone(), assignment.value.clone());
                }
            }
        }
    }

    fn option_is_available(&self, option: &FomodOptionPreview) -> bool {
        if !option.dependency_context_available {
            return false;
        }
        option
            .dependencies
            .as_ref()
            .map(|group| evaluate_dependency_group(group, &self.flags, self.dependency_context))
            .unwrap_or(true)
    }

    fn group_visible(&self, group: &FomodGroupPreview) -> bool {
        group
            .visible
            .as_ref()
            .map(|dependency| {
                evaluate_dependency_group(dependency, &self.flags, self.dependency_context)
            })
            .unwrap_or(true)
    }

    fn default_group_selection(&self, group: &FomodGroupPreview) -> Vec<String> {
        let mut selected = default_group_selection(group);
        self.normalize_group_selection(group, &mut selected);
        selected
    }

    fn normalize_group_selection(&self, group: &FomodGroupPreview, selected: &mut Vec<String>) {
        selected.retain(|id| {
            group
                .options
                .iter()
                .find(|option| &option.id == id)
                .map(|option| self.option_is_available(option))
                .unwrap_or(false)
        });

        let required: Vec<String> = group
            .options
            .iter()
            .filter(|option| {
                option.option_type.eq_ignore_ascii_case("Required")
                    && self.option_is_available(option)
            })
            .map(|option| option.id.clone())
            .collect();
        for option_id in required {
            if !selected.contains(&option_id) {
                selected.push(option_id);
            }
        }

        match normalized_selection_mode(&group.selection_mode).as_str() {
            "SelectAll" => {
                let available: Vec<String> = group
                    .options
                    .iter()
                    .filter(|option| self.option_is_available(option))
                    .map(|option| option.id.clone())
                    .collect();
                if !available.is_empty() && selected.len() != available.len() {
                    *selected = available;
                }
            }
            "SelectAny" => {}
            "SelectAtLeastOne" => {
                if selected.is_empty() {
                    if let Some(option) = group
                        .options
                        .iter()
                        .find(|option| self.option_is_available(option))
                    {
                        selected.push(option.id.clone());
                    }
                }
            }
            "SelectAtMostOne" => {
                if selected.len() > 1 {
                    selected.truncate(1);
                }
            }
            "SelectExactlyOne" => {
                if selected.len() > 1 {
                    selected.truncate(1);
                }
                if selected.is_empty() {
                    if let Some(option) = group
                        .options
                        .iter()
                        .find(|option| self.option_is_available(option))
                    {
                        selected.push(option.id.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

fn default_group_selection(group: &FomodGroupPreview) -> Vec<String> {
    match normalized_selection_mode(&group.selection_mode).as_str() {
        "SelectAll" => group.options.iter().map(|opt| opt.id.clone()).collect(),
        "SelectAny" => group
            .options
            .iter()
            .filter(|opt| opt.default_selected)
            .map(|opt| opt.id.clone())
            .collect(),
        "SelectAtMostOne" => group
            .options
            .iter()
            .filter(|opt| opt.default_selected)
            .take(1)
            .map(|opt| opt.id.clone())
            .collect(),
        "SelectExactlyOne" | "SelectAtLeastOne" => {
            let mut selected: Vec<String> = group
                .options
                .iter()
                .filter(|opt| opt.default_selected)
                .map(|opt| opt.id.clone())
                .collect();
            if selected.is_empty() {
                if let Some(option) = group.options.first() {
                    selected.push(option.id.clone());
                }
            }
            selected.truncate(1);
            selected
        }
        _ => Vec::new(),
    }
}

fn normalized_selection_mode(mode: &str) -> String {
    match mode {
        "SelectAll" | "SelectAny" | "SelectAtMostOne" | "SelectExactlyOne" | "SelectAtLeastOne" => {
            mode.to_string()
        }
        _ => "SelectExactlyOne".to_string(),
    }
}

fn normalize_selection(
    preview: &FomodPackagePreview,
    selection: FomodSelectionRequest,
) -> FomodSelectionRequest {
    normalize_selection_with_context(preview, selection, None)
}

fn normalize_selection_with_context(
    preview: &FomodPackagePreview,
    selection: FomodSelectionRequest,
    dependency_context: Option<&FomodDependencyContext>,
) -> FomodSelectionRequest {
    SelectionResolver::new(preview, Some(&selection), dependency_context).normalize_selection()
}

fn parse_steps(
    root: Node<'_, '_>,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<Vec<FomodStepPreview>> {
    let mut steps = Vec::new();
    if let Some(install_steps) = root
        .children()
        .find(|node| node.has_tag_name("installSteps"))
    {
        for step in install_steps
            .children()
            .filter(|node| node.has_tag_name("installStep"))
        {
            let name = step.attribute("name").unwrap_or("Unnamed step").to_string();
            let visible = parse_optional_dependency_group(step, "visible", dependency_context)?;
            let mut groups = Vec::new();
            if let Some(optional_groups) = step
                .children()
                .find(|node| node.has_tag_name("optionalFileGroups"))
            {
                for group in optional_groups
                    .children()
                    .filter(|node| node.has_tag_name("group"))
                {
                    groups.push(parse_group(group, dependency_context)?);
                }
            }
            steps.push(FomodStepPreview {
                name,
                visible,
                groups,
            });
        }
    }
    Ok(steps)
}

fn parse_group(
    node: Node<'_, '_>,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<FomodGroupPreview> {
    let name = node
        .attribute("name")
        .unwrap_or("Unnamed group")
        .to_string();
    let selection_mode =
        normalized_selection_mode(node.attribute("type").unwrap_or("SelectExactlyOne"));
    let visible = combine_dependency_groups([
        parse_optional_dependency_group(node, "visible", dependency_context)?,
        parse_optional_dependency_group(node, "dependencies", dependency_context)?,
    ]);
    let mut options = Vec::new();
    if let Some(plugins) = node.children().find(|child| child.has_tag_name("plugins")) {
        for (index, plugin) in plugins
            .children()
            .filter(|child| child.has_tag_name("plugin"))
            .enumerate()
        {
            options.push(parse_option(plugin, index, dependency_context)?);
        }
    }
    Ok(FomodGroupPreview {
        name,
        selection_mode,
        visible,
        options,
    })
}

fn parse_option(
    node: Node<'_, '_>,
    index: usize,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<FomodOptionPreview> {
    let name = node
        .attribute("name")
        .unwrap_or("Unnamed option")
        .to_string();
    let description = child_text(node, "description");
    let image_path = node
        .children()
        .find(|child| child.has_tag_name("image"))
        .and_then(|img| img.attribute("path"))
        .map(|value| value.to_string());
    let option_type = parse_option_type(node, dependency_context)?;
    let default_selected = matches!(option_type.as_str(), "Recommended" | "Required");
    let condition_flags = parse_condition_flags(node);
    let dependencies = combine_dependency_groups([
        parse_optional_dependency_group(node, "visible", dependency_context)?,
        parse_optional_dependency_group(node, "dependencies", dependency_context)?,
    ]);
    let dependency_context_available = !option_type.eq_ignore_ascii_case("NotUsable")
        && dependency_group_context_available(dependencies.as_ref(), dependency_context);
    let files = parse_file_specs(node, "files")?;
    Ok(FomodOptionPreview {
        id: format!("opt-{index}"),
        name,
        description,
        image_path,
        default_selected,
        option_type,
        file_count: files.len(),
        condition_flags,
        dependencies,
        dependency_context_available,
        files,
    })
}

fn parse_option_type(
    node: Node<'_, '_>,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<String> {
    let Some(descriptor) = node
        .children()
        .find(|child| child.has_tag_name("typeDescriptor"))
    else {
        return Ok("Optional".to_string());
    };

    if let Some(ty) = descriptor
        .children()
        .find(|child| child.has_tag_name("type"))
    {
        return Ok(ty.attribute("name").unwrap_or("Optional").to_string());
    }

    let Some(dependency_type) = descriptor
        .children()
        .find(|child| child.has_tag_name("dependencyType"))
    else {
        return Ok("Optional".to_string());
    };

    let default_type = dependency_type
        .children()
        .find(|child| child.has_tag_name("defaultType"))
        .and_then(|node| node.attribute("name"))
        .unwrap_or("Optional")
        .to_string();

    if let Some(patterns) = dependency_type
        .children()
        .find(|child| child.has_tag_name("patterns"))
    {
        for pattern in patterns
            .children()
            .filter(|child| child.has_tag_name("pattern"))
        {
            let dependencies =
                parse_optional_dependency_group(pattern, "dependencies", dependency_context)?;
            let matches = dependencies
                .as_ref()
                .map(|group| evaluate_dependency_group(group, &BTreeMap::new(), dependency_context))
                .unwrap_or(true);
            if matches {
                return Ok(pattern
                    .children()
                    .find(|child| child.has_tag_name("type"))
                    .and_then(|node| node.attribute("name"))
                    .unwrap_or(default_type.as_str())
                    .to_string());
            }
        }
    }

    Ok(default_type)
}

fn combine_dependency_groups(
    groups: [Option<FomodDependencyGroup>; 2],
) -> Option<FomodDependencyGroup> {
    let mut parts = Vec::new();
    for group in groups.into_iter().flatten() {
        parts.push(group);
    }

    match parts.len() {
        0 => None,
        1 => parts.into_iter().next(),
        _ => Some(FomodDependencyGroup {
            operator: "And".to_string(),
            dependencies: parts.into_iter().map(FomodDependency::Group).collect(),
        }),
    }
}

fn parse_condition_flags(node: Node<'_, '_>) -> Vec<FomodFlagAssignment> {
    let mut flags = Vec::new();
    if let Some(container) = node
        .children()
        .find(|child| child.has_tag_name("conditionFlags"))
    {
        for flag in container
            .children()
            .filter(|child| child.has_tag_name("flag"))
        {
            if let Some(name) = flag.attribute("name") {
                flags.push(FomodFlagAssignment {
                    name: name.to_string(),
                    value: flag.text().unwrap_or("selected").trim().to_string(),
                });
            }
        }
    }
    flags
}

fn parse_file_specs(parent: Node<'_, '_>, container_name: &str) -> SlimResult<Vec<FomodFileSpec>> {
    let mut files = Vec::new();
    if let Some(container) = parent
        .children()
        .find(|node| node.has_tag_name(container_name))
    {
        for file in container.children().filter(|node| node.is_element()) {
            if file.has_tag_name("file") {
                if let Some(source) = file.attribute("source") {
                    files.push(FomodFileSpec {
                        source: source.to_string(),
                        destination: file.attribute("destination").map(|value| value.to_string()),
                        is_folder: false,
                        priority: file
                            .attribute("priority")
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(0),
                    });
                }
            } else if file.has_tag_name("folder") {
                if let Some(source) = file.attribute("source") {
                    files.push(FomodFileSpec {
                        source: source.to_string(),
                        destination: file.attribute("destination").map(|value| value.to_string()),
                        is_folder: true,
                        priority: file
                            .attribute("priority")
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(0),
                    });
                }
            }
        }
    }
    Ok(files)
}

fn parse_conditional_installs(
    root: Node<'_, '_>,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<Vec<FomodConditionalInstall>> {
    let mut installs = Vec::new();
    if let Some(container) = root
        .children()
        .find(|node| node.has_tag_name("conditionalFileInstalls"))
    {
        if let Some(patterns) = container
            .children()
            .find(|node| node.has_tag_name("patterns"))
        {
            for pattern in patterns
                .children()
                .filter(|node| node.has_tag_name("pattern"))
            {
                let dependencies =
                    parse_optional_dependency_group(pattern, "dependencies", dependency_context)?;
                let files = parse_file_specs(pattern, "files")?;
                installs.push(FomodConditionalInstall {
                    dependencies,
                    files,
                });
            }
        }
    }
    Ok(installs)
}

fn parse_optional_dependency_group(
    parent: Node<'_, '_>,
    tag: &str,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<Option<FomodDependencyGroup>> {
    Ok(parent
        .children()
        .find(|node| node.has_tag_name(tag))
        .map(|node| parse_dependency_group(node, dependency_context))
        .transpose()?)
}

fn parse_dependency_group(
    node: Node<'_, '_>,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<FomodDependencyGroup> {
    let operator = node.attribute("operator").unwrap_or("And").to_string();
    let mut dependencies = Vec::new();
    for child in node.children().filter(|node| node.is_element()) {
        if child.has_tag_name("flagDependency") {
            dependencies.push(FomodDependency::Flag {
                flag: child.attribute("flag").unwrap_or("").to_string(),
                value: child.attribute("value").unwrap_or("").to_string(),
            });
        } else if child.has_tag_name("fileDependency") {
            let file = child.attribute("file").unwrap_or("").to_string();
            let state = child.attribute("state").unwrap_or("Active").to_string();
            dependencies.push(FomodDependency::File {
                satisfied: dependency_context
                    .map(|context| file_dependency_is_satisfied(&file, &state, Some(context))),
                file,
                state,
            });
        } else if child.has_tag_name("dependencies") || child.has_tag_name("moduleDependencies") {
            dependencies.push(FomodDependency::Group(parse_dependency_group(
                child,
                dependency_context,
            )?));
        }
    }
    Ok(FomodDependencyGroup {
        operator,
        dependencies,
    })
}

fn evaluate_dependency_group(
    group: &FomodDependencyGroup,
    flags: &BTreeMap<String, String>,
    dependency_context: Option<&FomodDependencyContext>,
) -> bool {
    let mut values = group
        .dependencies
        .iter()
        .map(|dependency| evaluate_dependency(dependency, flags, dependency_context));

    if group.operator.eq_ignore_ascii_case("or") {
        values.any(|value| value)
    } else {
        values.all(|value| value)
    }
}

fn dependency_group_context_available(
    group: Option<&FomodDependencyGroup>,
    dependency_context: Option<&FomodDependencyContext>,
) -> bool {
    match group {
        Some(group) => dependency_group_context_available_inner(group, dependency_context),
        None => true,
    }
}

fn dependency_group_context_available_inner(
    group: &FomodDependencyGroup,
    dependency_context: Option<&FomodDependencyContext>,
) -> bool {
    let mut values = group
        .dependencies
        .iter()
        .map(|dependency| dependency_context_available_dependency(dependency, dependency_context));

    if group.operator.eq_ignore_ascii_case("or") {
        values.any(|value| value)
    } else {
        values.all(|value| value)
    }
}

fn dependency_context_available_dependency(
    dependency: &FomodDependency,
    dependency_context: Option<&FomodDependencyContext>,
) -> bool {
    match dependency {
        FomodDependency::File { file, state, .. } => {
            file_dependency_is_satisfied(file, state, dependency_context)
        }
        FomodDependency::Group(group) => {
            dependency_group_context_available_inner(group, dependency_context)
        }
        FomodDependency::Flag { .. } => true,
    }
}

fn evaluate_dependency(
    dependency: &FomodDependency,
    flags: &BTreeMap<String, String>,
    dependency_context: Option<&FomodDependencyContext>,
) -> bool {
    match dependency {
        FomodDependency::Flag { flag, value } => flags
            .get(flag)
            .map(|current| current.eq_ignore_ascii_case(value))
            .unwrap_or(false),
        FomodDependency::File { file, state, .. } => {
            evaluate_file_dependency(file, state, dependency_context)
        }
        FomodDependency::Group(group) => {
            evaluate_dependency_group(group, flags, dependency_context)
        }
    }
}

pub fn file_dependency_is_satisfied(
    file: &str,
    state: &str,
    dependency_context: Option<&FomodDependencyContext>,
) -> bool {
    evaluate_file_dependency(file, state, dependency_context)
}

fn evaluate_file_dependency(
    file: &str,
    state: &str,
    dependency_context: Option<&FomodDependencyContext>,
) -> bool {
    let present = file_is_available(file, dependency_context);
    match state.trim().to_ascii_lowercase().as_str() {
        "active" => present,
        "inactive" | "missing" => !present,
        _ => present,
    }
}

fn file_is_available(file: &str, dependency_context: Option<&FomodDependencyContext>) -> bool {
    let Some(context) = dependency_context else {
        return false;
    };

    let normalized = normalize_fomod_dependency_path(file);
    if context.available_files.contains(&normalized) {
        return true;
    }

    let relative_variants = dependency_file_variants(&normalized);
    for root in &context.search_roots {
        for relative in &relative_variants {
            if path_exists_case_insensitive(root, relative) {
                return true;
            }
        }
    }

    false
}

fn dependency_file_variants(normalized: &str) -> Vec<PathBuf> {
    let mut variants = vec![PathBuf::from(normalized)];
    if let Some(stripped) = normalized.strip_prefix("data/") {
        variants.push(PathBuf::from(stripped));
    }
    variants
}

fn path_exists_case_insensitive(root: &Path, relative: &Path) -> bool {
    let mut current = root.to_path_buf();
    let components: Vec<_> = relative.components().collect();
    if components.is_empty() {
        return false;
    }

    for (index, component) in components.iter().enumerate() {
        let std::path::Component::Normal(expected_name) = component else {
            return false;
        };
        let Ok(read_dir) = fs::read_dir(&current) else {
            return false;
        };

        let mut matched = None;
        for entry in read_dir.filter_map(Result::ok) {
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&expected_name.to_string_lossy())
            {
                matched = Some(entry.path());
                break;
            }
        }

        let Some(next) = matched else {
            return false;
        };

        if index + 1 == components.len() {
            return next.exists();
        }
        current = next;
    }

    false
}

fn normalize_fomod_dependency_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim()
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_lowercase()
}

fn with_validation_notes(mut preview: FomodPackagePreview) -> FomodPackagePreview {
    let mut notes = Vec::new();

    if preview.module_name.is_none() {
        notes.push("moduleName is missing in ModuleConfig.xml".to_string());
    }

    if preview.steps.is_empty()
        && preview.required_files.is_empty()
        && preview.conditional_file_installs.is_empty()
    {
        notes.push("FOMOD package does not contain install steps or required files".to_string());
    }

    for (step_index, step) in preview.steps.iter().enumerate() {
        if step.groups.is_empty() {
            notes.push(format!("step #{step_index} ({}) has no groups", step.name));
        }
        for (group_index, group) in step.groups.iter().enumerate() {
            if group.options.is_empty() {
                notes.push(format!(
                    "step #{step_index} group #{group_index} ({}) has no options",
                    group.name
                ));
            }
            if !matches!(
                group.selection_mode.as_str(),
                "SelectAll"
                    | "SelectAny"
                    | "SelectAtMostOne"
                    | "SelectExactlyOne"
                    | "SelectAtLeastOne"
            ) {
                notes.push(format!(
                    "group '{}' uses unsupported selection mode '{}'; treating it as SelectExactlyOne",
                    group.name, group.selection_mode
                ));
            }
        }
    }

    if let Some(saved) = &preview.saved_selection {
        let normalized = normalize_selection(&preview, saved.clone());
        if normalized.selections.len() != saved.selections.len() {
            notes.push(
                "saved FOMOD selection was normalized against the current package".to_string(),
            );
        }
    }

    preview.validation_notes = notes;
    preview
}

fn child_text(node: Node<'_, '_>, tag: &str) -> Option<String> {
    node.children()
        .find(|child| child.has_tag_name(tag))
        .and_then(|child| child.text())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn decode_fomod_xml(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (text, _, _) = UTF_16LE.decode(&bytes[2..]);
        return text.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (text, _, _) = UTF_16BE.decode(&bytes[2..]);
        return text.into_owned();
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }

    if looks_like_utf16_le(bytes) {
        let (text, _, _) = UTF_16LE.decode(bytes);
        return text.into_owned();
    }
    if looks_like_utf16_be(bytes) {
        let (text, _, _) = UTF_16BE.decode(bytes);
        return text.into_owned();
    }

    String::from_utf8_lossy(bytes).into_owned()
}

fn looks_like_utf16_le(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || bytes.len() % 2 != 0 {
        return false;
    }
    let odd_nuls = bytes
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|byte| **byte == 0)
        .count();
    let even_nuls = bytes.iter().step_by(2).filter(|byte| **byte == 0).count();
    odd_nuls > even_nuls && odd_nuls >= bytes.len() / 8
}

fn looks_like_utf16_be(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || bytes.len() % 2 != 0 {
        return false;
    }
    let even_nuls = bytes.iter().step_by(2).filter(|byte| **byte == 0).count();
    let odd_nuls = bytes
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|byte| **byte == 0)
        .count();
    even_nuls > odd_nuls && even_nuls >= bytes.len() / 8
}

fn copy_spec(package_root: &Path, target_root: &Path, spec: &FomodFileSpec) -> SlimResult<usize> {
    let source = resolve_fomod_source_path(package_root, &spec.source)?;
    if spec.is_folder {
        return copy_folder_contents(&source, target_root, spec.destination.as_deref());
    }

    if !source.exists() {
        return Err(SlimError::NotFound(format!(
            "FOMOD source not found: {}",
            source.display()
        )));
    }

    let destination = match &spec.destination {
        Some(dest) if !dest.is_empty() => target_root.join(safe_fomod_relative_path(dest)?),
        _ => target_root.join(source.file_name().ok_or_else(|| {
            SlimError::InvalidPath(format!("source has no filename: {}", source.display()))
        })?),
    };
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&source, &destination)?;
    Ok(1)
}

fn copy_folder_contents(
    source: &Path,
    target_root: &Path,
    destination: Option<&str>,
) -> SlimResult<usize> {
    if !source.exists() {
        return Err(SlimError::NotFound(format!(
            "FOMOD folder source not found: {}",
            source.display()
        )));
    }

    let base = if let Some(destination) = destination.filter(|value| !value.trim().is_empty()) {
        target_root.join(safe_fomod_relative_path(destination)?)
    } else {
        target_root.to_path_buf()
    };
    fs::create_dir_all(&base)?;

    let mut copied = 0usize;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|err| SlimError::InvalidPath(err.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| SlimError::InvalidPath("failed to strip FOMOD source root".into()))?;
        let destination = base.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &destination)?;
        copied += 1;
    }
    Ok(copied)
}

fn resolve_fomod_source_path(package_root: &Path, source: &str) -> SlimResult<PathBuf> {
    let relative = safe_fomod_relative_path(source)?;
    let direct = package_root.join(&relative);
    if direct.exists() {
        return Ok(direct);
    }

    let relative_components: Vec<_> = relative.components().collect();
    if relative_components.is_empty() {
        return Err(SlimError::NotFound(format!(
            "FOMOD source not found: {}",
            direct.display()
        )));
    }

    let mut fallback = None;
    for entry in WalkDir::new(package_root).follow_links(false) {
        let entry = entry.map_err(|error| SlimError::InvalidPath(error.to_string()))?;
        let candidate = entry.path();
        let Ok(candidate_relative) = candidate.strip_prefix(package_root) else {
            continue;
        };
        let candidate_components: Vec<_> = candidate_relative.components().collect();
        if components_end_with_case_insensitive(&candidate_components, &relative_components) {
            fallback = Some(candidate.to_path_buf());
            if candidate.exists() {
                break;
            }
        }
    }

    fallback
        .ok_or_else(|| SlimError::NotFound(format!("FOMOD source not found: {}", direct.display())))
}

fn components_end_with_case_insensitive(
    candidate: &[std::path::Component<'_>],
    expected_suffix: &[std::path::Component<'_>],
) -> bool {
    if expected_suffix.len() > candidate.len() {
        return false;
    }

    let start = candidate.len() - expected_suffix.len();
    candidate[start..]
        .iter()
        .zip(expected_suffix.iter())
        .all(|(left, right)| component_eq_case_insensitive(left, right))
}

fn component_eq_case_insensitive(
    left: &std::path::Component<'_>,
    right: &std::path::Component<'_>,
) -> bool {
    match (left, right) {
        (std::path::Component::Normal(left), std::path::Component::Normal(right)) => left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()),
        _ => left == right,
    }
}

fn safe_fomod_relative_path(path: &str) -> SlimResult<PathBuf> {
    let replaced = path.replace('\\', "/");
    let mut normalized = replaced.trim();
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped;
    }
    let drive_absolute = normalized.as_bytes().get(1) == Some(&b':');
    let relative = PathBuf::from(normalized);
    if normalized.starts_with('/')
        || drive_absolute
        || relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(SlimError::Safety(format!(
            "unsafe FOMOD path rejected: {path}"
        )));
    }
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const BASIC_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<fomod>
  <moduleName>Example</moduleName>
  <installSteps>
    <installStep name="Main">
      <optionalFileGroups>
        <group name="Core" type="SelectExactlyOne">
          <plugins>
            <plugin name="Recommended">
              <description>Default option</description>
              <files>
                <file source="Data/example.esp" />
              </files>
            </plugin>
          </plugins>
        </group>
      </optionalFileGroups>
    </installStep>
  </installSteps>
</fomod>
"#;

    #[test]
    fn inspect_package_detects_basic_fomod() {
        let root = temp_root("basic");
        let config = root.join("fomod");
        std::fs::create_dir_all(&config).expect("create fomod dir");
        std::fs::write(config.join("ModuleConfig.xml"), BASIC_XML).expect("write xml");

        let preview = inspect_package_with_context(&root, None, None).expect("inspect package");

        assert!(preview.has_fomod);
        assert_eq!(preview.module_name.as_deref(), Some("Example"));
        assert_eq!(preview.steps.len(), 1);
        assert_eq!(preview.steps[0].groups[0].options[0].name, "Recommended");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_package_detects_root_level_module_config() {
        let root = temp_root("root-level");
        std::fs::write(root.join("ModuleConfig.xml"), BASIC_XML).expect("write xml");

        let preview = inspect_package_with_context(&root, None, None).expect("inspect package");

        assert!(preview.has_fomod);
        assert_eq!(preview.module_name.as_deref(), Some("Example"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_package_uses_fomod_parent_as_effective_source_root() {
        let root = temp_root("effective-root");
        let package_root = root.join("Wrapper");
        let fomod_dir = package_root.join("fomod");
        let required_dir = package_root.join("Required");
        std::fs::create_dir_all(&fomod_dir).expect("create fomod dir");
        std::fs::create_dir_all(required_dir.join("AIO")).expect("create required dir");
        std::fs::write(
            fomod_dir.join("ModuleConfig.xml"),
            BASIC_XML.replace("Data/example.esp", "Required/AIO/example.txt"),
        )
        .expect("write xml");

        let preview = inspect_package_with_context(&root, None, None).expect("inspect package");

        assert!(preview.has_fomod);
        assert_eq!(preview.source_path, package_root);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_package_detects_deeply_nested_module_config() {
        let root = temp_root("deep-fomod");
        let config = root
            .join("archives")
            .join("nested")
            .join("wrapper")
            .join("installer")
            .join("payload")
            .join("fomod");
        std::fs::create_dir_all(&config).expect("create nested fomod dir");
        std::fs::write(config.join("ModuleConfig.xml"), BASIC_XML).expect("write xml");

        let preview = inspect_package_with_context(&root, None, None).expect("inspect package");

        assert!(preview.has_fomod);
        assert_eq!(preview.module_name.as_deref(), Some("Example"));
        assert_eq!(preview.source_path, config.parent().unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_package_prefers_shallow_module_config_over_nested_patch_config() {
        let root = temp_root("preferred-config");
        let main_config = root.join("fomod");
        let patch_config = root.join("04 - Playable Patches").join("ECE").join("fomod");
        std::fs::create_dir_all(&main_config).expect("create main fomod dir");
        std::fs::create_dir_all(&patch_config).expect("create patch fomod dir");
        std::fs::write(
            main_config.join("ModuleConfig.xml"),
            BASIC_XML.replace("Example", "Main Installer"),
        )
        .expect("write main xml");
        std::fs::write(
            patch_config.join("ModuleConfig.xml"),
            BASIC_XML.replace("Example", "Patch Installer"),
        )
        .expect("write patch xml");

        let preview = inspect_package_with_context(&root, None, None).expect("inspect package");

        assert!(preview.has_fomod);
        assert_eq!(preview.module_name.as_deref(), Some("Main Installer"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn default_selection_skips_hidden_groups_and_does_not_force_patch_choices() {
        let preview = FomodPackagePreview {
            module_name: Some("Example".to_string()),
            source_path: PathBuf::from("/tmp/example"),
            has_fomod: true,
            validation_notes: Vec::new(),
            module_dependencies: None,
            required_files: Vec::new(),
            conditional_file_installs: Vec::new(),
            steps: vec![FomodStepPreview {
                name: "Main".to_string(),
                visible: None,
                groups: vec![
                    FomodGroupPreview {
                        name: "Base".to_string(),
                        selection_mode: "SelectExactlyOne".to_string(),
                        visible: None,
                        options: vec![FomodOptionPreview {
                            id: "base-0".to_string(),
                            name: "Base".to_string(),
                            description: None,
                            image_path: None,
                            default_selected: true,
                            option_type: "Required".into(),
                            file_count: 0,
                            condition_flags: vec![FomodFlagAssignment {
                                name: "base".to_string(),
                                value: "selected".to_string(),
                            }],
                            dependencies: None,
                            dependency_context_available: true,
                            files: Vec::new(),
                        }],
                    },
                    FomodGroupPreview {
                        name: "Patch".to_string(),
                        selection_mode: "SelectExactlyOne".to_string(),
                        visible: Some(FomodDependencyGroup {
                            operator: "And".to_string(),
                            dependencies: vec![FomodDependency::File {
                                file: "mods/SomeMissingPatch.esp".to_string(),
                                state: "Active".to_string(),
                                satisfied: None,
                            }],
                        }),
                        options: vec![FomodOptionPreview {
                            id: "patch-0".to_string(),
                            name: "Patch".to_string(),
                            description: None,
                            image_path: None,
                            default_selected: true,
                            option_type: "Required".into(),
                            file_count: 0,
                            condition_flags: Vec::new(),
                            dependencies: None,
                            dependency_context_available: true,
                            files: Vec::new(),
                        }],
                    },
                ],
            }],
            saved_selection: None,
        };

        let selection = default_selection_with_context(&preview, None);
        assert_eq!(selection.selections.len(), 1);
        assert_eq!(selection.selections[0].group_index, 0);
    }

    #[test]
    fn inspect_package_tolerates_non_utf8_xml_bytes() {
        let root = temp_root("non-utf8");
        let config = root.join("fomod");
        std::fs::create_dir_all(&config).expect("create fomod dir");
        let xml = BASIC_XML
            .replace("Recommended", "R\u{FFFD}commended")
            .into_bytes();
        std::fs::write(config.join("ModuleConfig.xml"), xml).expect("write xml");

        let preview = inspect_package_with_context(&root, None, None).expect("inspect package");

        assert!(preview.has_fomod);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_package_decodes_utf16_xml_bytes() {
        let root = temp_root("utf16");
        let config = root.join("fomod");
        std::fs::create_dir_all(&config).expect("create fomod dir");
        let mut xml = vec![0xFF, 0xFE];
        for unit in BASIC_XML.encode_utf16() {
            xml.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(config.join("ModuleConfig.xml"), xml).expect("write xml");

        let preview = inspect_package_with_context(&root, None, None).expect("inspect package");

        assert!(preview.has_fomod);
        assert_eq!(preview.module_name.as_deref(), Some("Example"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn install_selected_files_normalizes_windows_separators_in_folder_sources() {
        let root = temp_root("windows-paths");
        let source = root.join("Required").join("AIO");
        std::fs::create_dir_all(&source).expect("create source folder");
        std::fs::write(source.join("example.txt"), b"example").expect("write source file");
        let target = root.join("target");
        std::fs::create_dir_all(&target).expect("create target folder");

        let preview = FomodPackagePreview {
            module_name: Some("Example".into()),
            source_path: root.clone(),
            has_fomod: true,
            validation_notes: Vec::new(),
            module_dependencies: None,
            required_files: vec![FomodFileSpec {
                source: r"Required\AIO".into(),
                destination: None,
                is_folder: true,
                priority: 0,
            }],
            conditional_file_installs: Vec::new(),
            steps: Vec::new(),
            saved_selection: None,
        };

        let copied = install_selected_files_with_context(
            &root,
            &target,
            &preview,
            &FomodSelectionRequest {
                selections: Vec::new(),
            },
            None,
        )
        .expect("install selected files");

        assert_eq!(copied, 1);
        assert!(target.join("example.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn install_selected_files_resolves_case_mismatched_folder_sources() {
        let root = temp_root("case-paths");
        let source = root.join("1 Core Files").join("Meshes");
        std::fs::create_dir_all(&source).expect("create source folder");
        std::fs::write(source.join("blood.nif"), b"mesh").expect("write source file");
        let target = root.join("target");
        std::fs::create_dir_all(&target).expect("create target folder");

        let preview = FomodPackagePreview {
            module_name: Some("Example".into()),
            source_path: root.clone(),
            has_fomod: true,
            validation_notes: Vec::new(),
            module_dependencies: None,
            required_files: vec![FomodFileSpec {
                source: r"1 core files/Meshes".into(),
                destination: Some("Meshes".into()),
                is_folder: true,
                priority: 0,
            }],
            conditional_file_installs: Vec::new(),
            steps: Vec::new(),
            saved_selection: None,
        };

        let copied = install_selected_files_with_context(
            &root,
            &target,
            &preview,
            &FomodSelectionRequest {
                selections: Vec::new(),
            },
            None,
        )
        .expect("install selected files");

        assert_eq!(copied, 1);
        assert!(target.join("Meshes").join("blood.nif").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn install_selected_files_finds_sources_inside_wrapper_directories() {
        let root = temp_root("wrapper-paths");
        let package_root = root.join("Wrapper");
        let source = package_root.join("Required").join("AIO");
        std::fs::create_dir_all(&source).expect("create source folder");
        std::fs::write(source.join("example.txt"), b"example").expect("write source file");
        let target = root.join("target");
        std::fs::create_dir_all(&target).expect("create target folder");

        let preview = FomodPackagePreview {
            module_name: Some("Example".into()),
            source_path: package_root.clone(),
            has_fomod: true,
            validation_notes: Vec::new(),
            module_dependencies: None,
            required_files: vec![FomodFileSpec {
                source: r"Required\AIO".into(),
                destination: None,
                is_folder: true,
                priority: 0,
            }],
            conditional_file_installs: Vec::new(),
            steps: Vec::new(),
            saved_selection: None,
        };

        let copied = install_selected_files_with_context(
            &package_root,
            &target,
            &preview,
            &FomodSelectionRequest {
                selections: Vec::new(),
            },
            None,
        )
        .expect("install selected files");

        assert_eq!(copied, 1);
        assert!(target.join("example.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn default_selection_skips_unavailable_options() {
        let preview = FomodPackagePreview {
            module_name: Some("Example".to_string()),
            source_path: PathBuf::from("/tmp/example"),
            has_fomod: true,
            validation_notes: Vec::new(),
            module_dependencies: None,
            required_files: Vec::new(),
            conditional_file_installs: Vec::new(),
            steps: vec![FomodStepPreview {
                name: "Main".to_string(),
                visible: None,
                groups: vec![FomodGroupPreview {
                    name: "Group".to_string(),
                    selection_mode: "SelectExactlyOne".to_string(),
                    visible: None,
                    options: vec![
                        FomodOptionPreview {
                            id: "opt-0".to_string(),
                            name: "Unavailable".to_string(),
                            description: None,
                            image_path: None,
                            default_selected: true,
                            option_type: "Optional".into(),
                            file_count: 0,
                            condition_flags: Vec::new(),
                            dependencies: None,
                            dependency_context_available: false,
                            files: Vec::new(),
                        },
                        FomodOptionPreview {
                            id: "opt-1".to_string(),
                            name: "Available".to_string(),
                            description: None,
                            image_path: None,
                            default_selected: false,
                            option_type: "Optional".into(),
                            file_count: 0,
                            condition_flags: Vec::new(),
                            dependencies: None,
                            dependency_context_available: true,
                            files: Vec::new(),
                        },
                    ],
                }],
            }],
            saved_selection: None,
        };

        let selection = default_selection_with_context(&preview, None);
        assert_eq!(selection.selections.len(), 1);
        assert_eq!(
            selection.selections[0].selected_option_ids,
            vec!["opt-1".to_string()]
        );
    }

    #[test]
    fn inspect_package_marks_dependency_type_options_unavailable_when_file_missing() {
        let root = temp_root("dependency-type-missing");
        let config = root.join("fomod");
        std::fs::create_dir_all(&config).expect("create fomod dir");
        std::fs::write(
            config.join("ModuleConfig.xml"),
            r#"
<fomod>
  <moduleName>Dependency Type Example</moduleName>
  <installSteps>
    <installStep name="Patches">
      <optionalFileGroups>
        <group name="Armor Patches" type="SelectExactlyOne">
          <plugins>
            <plugin name="Lustmord Armor Patch">
              <description>Only useful when the armor plugin is present.</description>
              <typeDescriptor>
                <dependencyType>
                  <defaultType name="NotUsable" />
                  <patterns>
                    <pattern>
                      <dependencies operator="And">
                        <fileDependency file="LustmordVampireArmor.esp" state="Active" />
                      </dependencies>
                      <type name="Recommended" />
                    </pattern>
                  </patterns>
                </dependencyType>
              </typeDescriptor>
              <files>
                <file source="patch.esp" destination="patch.esp" />
              </files>
            </plugin>
          </plugins>
        </group>
      </optionalFileGroups>
    </installStep>
  </installSteps>
</fomod>
"#,
        )
        .expect("write module config");

        let missing_context = FomodDependencyContext::default();
        let preview =
            inspect_package_with_context(&root, None, Some(&missing_context)).expect("inspect");
        let option = &preview.steps[0].groups[0].options[0];

        assert!(!option.dependency_context_available);
        assert!(!option.default_selected);
        assert!(
            default_selection_with_context(&preview, Some(&missing_context)).selections[0]
                .selected_option_ids
                .is_empty()
        );

        let available_context = FomodDependencyContext::from_roots(
            BTreeSet::from(["lustmordvampirearmor.esp".to_string()]),
            Vec::<PathBuf>::new(),
        );
        let preview =
            inspect_package_with_context(&root, None, Some(&available_context)).expect("inspect");
        let option = &preview.steps[0].groups[0].options[0];

        assert!(option.dependency_context_available);
        assert!(option.default_selected);
        assert_eq!(
            default_selection_with_context(&preview, Some(&available_context)).selections[0]
                .selected_option_ids,
            vec!["opt-0".to_string()]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn safe_fomod_paths_reject_parent_absolute_and_empty_paths() {
        assert!(safe_fomod_relative_path("meshes/armor/file.nif").is_ok());
        assert!(safe_fomod_relative_path(r"textures\armor\file.dds").is_ok());
        assert!(safe_fomod_relative_path("../outside.txt").is_err());
        assert!(safe_fomod_relative_path("meshes/../../outside.txt").is_err());
        assert!(safe_fomod_relative_path("/etc/passwd").is_err());
        assert!(safe_fomod_relative_path(r"C:\outside.txt").is_err());
        assert!(safe_fomod_relative_path("/").is_err());
    }

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("slim-cc-fomod-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }
}
