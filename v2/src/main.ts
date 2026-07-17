import "./styles/app.css";
import { SlimApp } from "./app";

const root = document.querySelector<HTMLElement>("#app");
if (!root) throw new Error("SLiM-CC v2 mount point is missing");

new SlimApp(root).start().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  root.innerHTML = `<main class="fatal"><h1>SLiM-CC konnte nicht gestartet werden</h1><p>${message}</p></main>`;
});
