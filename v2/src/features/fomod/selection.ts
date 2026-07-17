import type { FomodGroupPreview, FomodPackagePreview, FomodSelectionEntry } from "../../types";

export function initialSelections(preview: FomodPackagePreview): FomodSelectionEntry[] {
  if (preview.saved_selection) return structuredClone(preview.saved_selection.selections);
  return preview.steps.flatMap((step, stepIndex) => step.groups.map((group, groupIndex) => ({
    step_index: stepIndex,
    group_index: groupIndex,
    selected_option_ids: normalizeSelection(group, group.options.filter((option) => option.default_selected).map((option) => option.id))
  })));
}

export function normalizeSelection(group: FomodGroupPreview, selected: string[]): string[] {
  const available = new Set(group.options.map((option) => option.id));
  let result = [...new Set(selected)].filter((id) => available.has(id));
  const required = group.options.filter((option) => option.option_type?.toLowerCase() === "required").map((option) => option.id);
  result = [...new Set([...required, ...result])];
  switch (group.selection_mode.toLowerCase()) {
    case "selectexactlyone": return [result[0] ?? group.options[0]?.id].filter(Boolean) as string[];
    case "selectatleastone": return result.length ? result : [group.options[0]?.id].filter(Boolean) as string[];
    case "selectatmostone": return result.slice(0, 1);
    case "selectany": default: return result;
  }
}

export function selectionIsValid(preview: FomodPackagePreview, selections: FomodSelectionEntry[]): boolean {
  return preview.steps.every((step, stepIndex) => step.groups.every((group, groupIndex) => {
    const count = selections.find((entry) => entry.step_index === stepIndex && entry.group_index === groupIndex)?.selected_option_ids.length ?? 0;
    const mode = group.selection_mode.toLowerCase();
    return mode === "selectexactlyone" ? count === 1 : mode === "selectatleastone" ? count >= 1 : mode === "selectatmostone" ? count <= 1 : true;
  }));
}
