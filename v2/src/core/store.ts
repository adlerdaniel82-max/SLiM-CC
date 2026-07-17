import type { AppSettings, FomodPackagePreview, GameInstance, ModConflictSummary, ModDependencySummary, ModDownloadCandidate, ModRecord, Profile, ProfileModEntry, ProfilePluginEntry, StatusEvent, View } from "../types";

export interface AppState {
  instances: GameInstance[]; profiles: Profile[]; mods: ModRecord[]; profileMods: ProfileModEntry[];
  plugins: ProfilePluginEntry[]; downloads: ModDownloadCandidate[]; dependencies: ModDependencySummary[];
  conflicts: ModConflictSummary[]; settings: AppSettings | null; activeInstanceId: string | null;
  activeProfileId: string | null; activeModId: string | null; view: View; loading: boolean;
  fomod: FomodPackagePreview | null; events: StatusEvent[];
}

const initial: AppState = {
  instances: [], profiles: [], mods: [], profileMods: [], plugins: [], downloads: [], dependencies: [],
  conflicts: [], settings: null, activeInstanceId: null, activeProfileId: null, activeModId: null,
  view: "mods", loading: false, fomod: null, events: []
};

export class Store extends EventTarget {
  state: AppState = structuredClone(initial);
  patch(next: Partial<AppState>): void { Object.assign(this.state, next); this.dispatchEvent(new Event("change")); }
  log(severity: StatusEvent["severity"], message: string): void {
    this.state.events = [...this.state.events.slice(-99), { id: Date.now() + Math.random(), severity, message, time: new Date() }];
    this.dispatchEvent(new Event("change"));
  }
}
