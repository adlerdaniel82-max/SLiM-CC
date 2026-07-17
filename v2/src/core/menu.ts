export function closeMenus(root: ParentNode = document): void {
  root.querySelectorAll<HTMLDetailsElement>(".menu[open]").forEach((menu) => menu.removeAttribute("open"));
}
export function installMenuBehavior(root: HTMLElement): () => void {
  const onPointer = (event: Event) => { if (!(event.target as Element).closest(".menu")) closeMenus(root); };
  const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") closeMenus(root); };
  document.addEventListener("pointerdown", onPointer); document.addEventListener("keydown", onKey);
  return () => { document.removeEventListener("pointerdown", onPointer); document.removeEventListener("keydown", onKey); };
}
