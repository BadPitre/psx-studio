// Pont vers le backend Tauri. En navigateur pur (npm run dev sans Tauri),
// isTauri est false et l'éditeur reste une visionneuse de .psc.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

export const isTauri =
  typeof window !== "undefined" &&
  ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

export interface SceneEntry {
  name: string;
  path: string;
}

export interface ProjectInfo {
  name: string;
  dir: string;
  scenes: SceneEntry[];
  exe_present: boolean;
}

export interface BuiltScene {
  psc: number[];
  entity_names: string[];
  warnings: string[];
}

export interface BuildSummary {
  converted: number;
  cached: number;
  cue_path: string;
  mkpsxiso_ran: boolean;
  warnings: string[];
}

export async function pickProjectDir(): Promise<string | null> {
  const dir = await openDialog({ directory: true, title: "Ouvrir un projet PSX Studio" });
  return typeof dir === "string" ? dir : null;
}

export interface ImportedAsset {
  kind: "Model" | "Texture";
  id: string;
  out: string;
  summary: string;
  warnings: string[];
}

/** Abonnement au drag & drop natif Tauri (chemins de fichiers). */
export async function onFileDrop(cb: (paths: string[]) => void): Promise<() => void> {
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "drop") cb(event.payload.paths);
  });
}

export const api = {
  importAsset: (projectDir: string, srcPath: string) =>
    tauriInvoke<ImportedAsset>("import_asset", { projectDir, srcPath }),
  openProject: (path: string) => tauriInvoke<ProjectInfo>("open_project", { path }),
  loadScene: (projectDir: string, scenePath: string) =>
    tauriInvoke<string>("load_scene", { projectDir, scenePath }),
  saveScene: (projectDir: string, scenePath: string, contents: string) =>
    tauriInvoke<void>("save_scene", { projectDir, scenePath, contents }),
  buildScene: (projectDir: string, scenePath: string, contents: string) =>
    tauriInvoke<BuiltScene>("build_scene", { projectDir, scenePath, contents }),
  play: (projectDir: string, emulatorPath: string | null, port: number) =>
    tauriInvoke<BuildSummary>("play", { projectDir, emulatorPath, port }),
  reduxStatus: (port: number) => tauriInvoke<boolean>("redux_status", { port }),
  reduxPause: (port: number) => tauriInvoke<void>("redux_pause", { port }),
  reduxResume: (port: number) => tauriInvoke<void>("redux_resume", { port }),
  reduxReset: (port: number) => tauriInvoke<void>("redux_reset", { port }),
};
