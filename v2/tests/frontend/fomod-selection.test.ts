import { describe, expect, it } from "vitest";
import { normalizeSelection, selectionIsValid } from "../../src/features/fomod/selection";
import type { FomodGroupPreview, FomodPackagePreview } from "../../src/types";

const group = (selection_mode: string): FomodGroupPreview => ({ name: "Optionen", selection_mode, visible: null, options: [
  { id: "a", name: "A", description: null, image_path: null, default_selected: false, file_count: 1, condition_flags: [], dependencies: null, dependency_context_available: true, files: [] },
  { id: "b", name: "B", description: null, image_path: null, default_selected: false, file_count: 1, condition_flags: [], dependencies: null, dependency_context_available: true, files: [] }
] });

describe("FOMOD selection semantics", () => {
  it("allows no selection for SelectAny and SelectAtMostOne", () => {
    expect(normalizeSelection(group("SelectAny"), [])).toEqual([]);
    expect(normalizeSelection(group("SelectAtMostOne"), [])).toEqual([]);
  });
  it("requires exactly one for SelectExactlyOne", () => {
    expect(normalizeSelection(group("SelectExactlyOne"), [])).toEqual(["a"]);
    expect(normalizeSelection(group("SelectExactlyOne"), ["a", "b"])).toEqual(["a"]);
  });
  it("validates minimum-selection groups", () => {
    const preview = { steps: [{ name: "Schritt", visible: null, groups: [group("SelectAtLeastOne")] }] } as FomodPackagePreview;
    expect(selectionIsValid(preview, [{ step_index: 0, group_index: 0, selected_option_ids: [] }])).toBe(false);
  });
});
