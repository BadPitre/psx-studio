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
  /** Texture extraite d'un glTF/GLB et convertie avec le modèle. */
  texture_out: string | null;
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
  getModelSubdiv: (projectDir: string, pmdOut: string) =>
    tauriInvoke<number | null>("get_model_subdiv", { projectDir, pmdOut }),
  /** Écrit project.json et reconvertit le modèle ; retourne un résumé. */
  setModelSubdiv: (projectDir: string, pmdOut: string, subdiv: number | null) =>
    tauriInvoke<string>("set_model_subdiv", { projectDir, pmdOut, subdiv }),
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
  /** Live tweaking : écrit la transform d'une entité dans la RAM console. */
  reduxSyncEntity: (
    port: number,
    index: number,
    pos: [number, number, number],
    rot: [number, number, number],
    scale: [number, number, number],
  ) => tauriInvoke<void>("redux_sync_entity", { port, index, pos, rot, scale }),
  reduxClearBeacon: () => tauriInvoke<void>("redux_clear_beacon"),
};
