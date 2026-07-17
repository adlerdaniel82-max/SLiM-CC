import { invoke } from "@tauri-apps/api/core";

export interface Backend { call<T>(command: string, args?: Record<string, unknown>): Promise<T> }

export class TauriBackend implements Backend {
  async call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    try { return await invoke<T>(command, args); }
    catch (error) { throw new Error(`${command}: ${error instanceof Error ? error.message : String(error)}`); }
  }
}

export function backend(): Backend {
  const mock = (window as Window & { __SLIMCC_BACKEND__?: Backend }).__SLIMCC_BACKEND__;
  return mock ?? new TauriBackend();
}
