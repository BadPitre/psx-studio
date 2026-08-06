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

/* Le drag & drop natif Tauri (dragDropEnabled) est coupé : il avale les
 * événements de drag HTML5 sous Windows et casse tout le drag & drop
 * interne (hiérarchie, panneau Project, viewport). Les fichiers déposés
 * arrivent donc en HTML5 (dataTransfer.files) et sont importés par
 * contenu via import_asset_bytes. */

/** Une entrée du panneau Project (fichier du projet croisé avec project.json). */
export interface ProjectFile {
  path: string;
  name: string;
  section: string;
  kind: "dir" | "scene" | "prefab" | "model" | "texture" | "audio" | "buffer" | "other";
  size: number;
  registered: boolean;
  exists: boolean;
  /** Sortie convertie dans Library/ (vignettes, instanciation). */
  out: string | null;
}

export const api = {
  importAsset: (projectDir: string, srcPath: string) =>
    tauriInvoke<ImportedAsset>("import_asset", { projectDir, srcPath }),
  /** Import d'un fichier déposé (HTML5 : contenu sans chemin absolu). */
  importAssetBytes: (projectDir: string, name: string, bytes: number[]) =>
    tauriInvoke<ImportedAsset>("import_asset_bytes", { projectDir, name, bytes }),
  listProjectFiles: (projectDir: string) =>
    tauriInvoke<ProjectFile[]>("list_project_files", { projectDir }),
  /** Crée une scène vide enregistrée ; retourne son chemin relatif. */
  createScene: (projectDir: string, name: string) =>
    tauriInvoke<string>("create_scene", { projectDir, name }),
  /** Sauvegarde un prefab (JSON d'entités + assets) ; retourne son chemin. */
  savePrefab: (projectDir: string, name: string, contents: string) =>
    tauriInvoke<string>("save_prefab", { projectDir, name, contents }),
  createFolder: (projectDir: string, parent: string, name: string) =>
    tauriInvoke<string>("create_folder", { projectDir, parent, name }),
  /** Déplace un fichier vers un dossier et met à jour project.json. */
  moveEntry: (projectDir: string, from: string, toDir: string) =>
    tauriInvoke<string>("move_entry", { projectDir, from, toDir }),
  /** Lit un fichier converti de Library/ (pour les vignettes). */
  readLibraryFile: (projectDir: string, name: string) =>
    tauriInvoke<number[]>("read_library_file", { projectDir, name }),
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
  /** Build complet du projet (conversion des assets dans Library/). */
  buildProject: (projectDir: string) =>
    tauriInvoke<BuildSummary>("build_project", { projectDir }),
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
