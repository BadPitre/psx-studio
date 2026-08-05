// PSX Studio — éditeur, Phase 4.
// Mode navigateur : visionneuse de .psc (drag & drop).
// Mode Tauri (desktop) : projet complet — édition des scene.json (transforms,
// création/duplication/suppression d'entités, renommage, modèle), import
// d'assets par drag & drop, VRAM Viewer, Play Mode PCSX-Redux.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { parsePsc, sceneTriangleCount, type PscScene } from "./formats/psc";
import { Viewport, type GizmoMode } from "./viewport/Viewport";
import { api, isTauri, onFileDrop, pickProjectDir, type ProjectInfo } from "./bridge";
import { PlayBar, PLAY_PORT } from "./PlayBar";
import { VramPanel } from "./VramPanel";

type Transform = {
  pos: [number, number, number];
  rot: [number, number, number];
  scale: [number, number, number];
};

type SceneDoc = {
  name?: string;
  assets?: {
    textures?: { id: string; tim: string }[];
    models?: { id: string; pmd: string; texture?: string }[];
  };
  entities?: Record<string, unknown>[];
  [key: string]: unknown;
};

/* --------------------------------------------------------- hiérarchie -- */

function Hierarchy({
  scene,
  names,
  selected,
  onSelect,
  onAdd,
  onContextMenu,
}: {
  scene: PscScene;
  names: string[];
  selected: number;
  onSelect: (i: number) => void;
  onAdd?: () => void;
  onContextMenu?: (i: number, x: number, y: number) => void;
}) {
  const depths = useMemo(() => {
    const d: number[] = [];
    scene.entities.forEach((e, i) => {
      d[i] = e.parent >= 0 ? d[e.parent] + 1 : 0;
    });
    return d;
  }, [scene]);

  return (
    <div className="panel">
      <div className="panel-title">Hiérarchie</div>
      {onAdd && (
        <div className="hierarchy-tools">
          <button className="button" onClick={onAdd} title="Nouvelle entité">
            ＋
          </button>
          <span
            className="muted"
            title="Ctrl+Z annuler · Ctrl+Y rétablir · Ctrl+C copier · Ctrl+V coller · Ctrl+D dupliquer · Suppr supprimer · F2 renommer · clic droit : menu"
          >
            raccourcis ⓘ
          </span>
        </div>
      )}
      {scene.entities.map((e, i) => (
        <div
          key={i}
          className={`tree-item ${selected === i ? "selected" : ""}`}
          style={{ paddingLeft: 8 + depths[i] * 16 }}
          onClick={() => onSelect(i)}
          onContextMenu={(ev) => {
            if (!onContextMenu) return;
            ev.preventDefault();
            onSelect(i);
            onContextMenu(i, ev.clientX, ev.clientY);
          }}
        >
          <span className="tree-icon">{e.model >= 0 ? "▣" : "○"}</span>
          {names[i] ?? `entité ${i}`}
          {e.model >= 0 && (
            <span className="tree-meta">{scene.models[e.model].prims.length} tris</span>
          )}
        </div>
      ))}
    </div>
  );
}

/* ------------------------------------------------------ menu contextuel -- */

function ContextMenu({
  x,
  y,
  onClose,
  actions,
}: {
  x: number;
  y: number;
  onClose: () => void;
  actions: { label: string; shortcut?: string; onClick: () => void; danger?: boolean }[];
}) {
  useEffect(() => {
    const close = () => onClose();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("click", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return (
    <div className="context-menu" style={{ left: x, top: y }}>
      {actions.map((a) => (
        <div
          key={a.label}
          className={`context-item ${a.danger ? "danger" : ""}`}
          onClick={() => {
            onClose();
            a.onClick();
          }}
        >
          <span>{a.label}</span>
          {a.shortcut && <span className="context-shortcut">{a.shortcut}</span>}
        </div>
      ))}
    </div>
  );
}

/* --------------------------------------------------------- inspecteur -- */

function Vec3Field({
  label,
  value,
  step,
  onChange,
}: {
  label: string;
  value: [number, number, number];
  step: number;
  onChange: (v: [number, number, number]) => void;
}) {
  return (
    <div className="field">
      <label>{label}</label>
      <div className="vec3">
        {(["X", "Y", "Z"] as const).map((axis, k) => (
          <input
            key={axis}
            type="number"
            step={step}
            value={value[k]}
            onChange={(e) => {
              const next = [...value] as [number, number, number];
              next[k] = Number(e.target.value);
              onChange(next);
            }}
          />
        ))}
      </div>
    </div>
  );
}

function Inspector({
  scene,
  name,
  selected,
  transform,
  onChange,
  modelIds,
  currentModelId,
  onRename,
  onModelChange,
  script,
  onScriptChange,
  subdiv,
  onSubdivChange,
  focusNameSignal,
}: {
  scene: PscScene;
  name: string;
  selected: number;
  transform: Transform;
  onChange: (t: Transform) => void;
  modelIds?: string[];
  currentModelId?: string | null;
  onRename?: (name: string) => void;
  onModelChange?: (id: string | null) => void;
  script?: string | null;
  onScriptChange?: (script: string | null) => void;
  subdiv?: number | null;
  onSubdivChange?: (subdiv: number | null) => void;
  focusNameSignal?: number;
}) {
  const entity = scene.entities[selected];
  const toDeg = (u: number) => Math.round((u / 4096) * 3600) / 10;
  const toUnits = (deg: number) => Math.round((deg / 360) * 4096);
  const [draftName, setDraftName] = useState(name);
  const nameRef = useRef<HTMLInputElement>(null);
  useEffect(() => setDraftName(name), [name]);
  const [draftScript, setDraftScript] = useState(script ?? "");
  useEffect(() => setDraftScript(script ?? ""), [script, selected]);
  const [draftSubdiv, setDraftSubdiv] = useState(subdiv == null ? "" : String(subdiv));
  useEffect(() => setDraftSubdiv(subdiv == null ? "" : String(subdiv)), [subdiv, selected]);
  /* F2 / menu « Renommer » : focus + sélection du champ nom. */
  useEffect(() => {
    if (focusNameSignal) {
      nameRef.current?.focus();
      nameRef.current?.select();
    }
  }, [focusNameSignal]);

  return (
    <div className="panel">
      <div className="panel-title">Inspecteur</div>
      {onRename ? (
        <div className="field-group">
          <input
            ref={nameRef}
            className="name-input"
            value={draftName}
            spellCheck={false}
            onChange={(e) => setDraftName(e.target.value)}
            onBlur={() => draftName.trim() && draftName !== name && onRename(draftName.trim())}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
          />
        </div>
      ) : (
        <div className="field-group">
          <div className="field-readonly">{name}</div>
        </div>
      )}
      <div className="field-group">
        <div className="field-group-title">Transform</div>
        <Vec3Field
          label="Position"
          step={10}
          value={transform.pos}
          onChange={(pos) => onChange({ ...transform, pos })}
        />
        <Vec3Field
          label="Rotation °"
          step={5}
          value={transform.rot.map(toDeg) as [number, number, number]}
          onChange={(deg) =>
            onChange({ ...transform, rot: deg.map(toUnits) as [number, number, number] })
          }
        />
        <Vec3Field
          label="Échelle"
          step={0.1}
          value={transform.scale}
          onChange={(scale) => onChange({ ...transform, scale })}
        />
      </div>
      <div className="field-group">
        <div className="field-group-title">MeshRenderer</div>
        {onModelChange && modelIds ? (
          <select
            className="scene-select model-select"
            value={currentModelId ?? ""}
            onChange={(e) => onModelChange(e.target.value || null)}
          >
            <option value="">(aucun modèle)</option>
            {modelIds.map((id) => (
              <option key={id} value={id}>
                {id}
              </option>
            ))}
          </select>
        ) : (
          <div className="field-readonly">
            {entity.model >= 0
              ? `modèle #${entity.model} — ${scene.models[entity.model].prims.length} triangles`
              : "aucun modèle (nœud vide)"}
          </div>
        )}
        {entity.parent >= 0 && (
          <div className="field-readonly">parent : entité {entity.parent}</div>
        )}
        {onSubdivChange && entity.model >= 0 && (
          <div className="field">
            <label>Subdivision anti-warping (arête max, vide = off)</label>
            <input
              type="number"
              min={1}
              max={32767}
              step={8}
              value={draftSubdiv}
              placeholder="off"
              onChange={(e) => setDraftSubdiv(e.target.value)}
              onBlur={() => {
                const v = draftSubdiv.trim() === "" ? null : Number(draftSubdiv);
                if (v !== (subdiv ?? null) && (v === null || v > 0)) onSubdivChange(v);
              }}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
              title="Coupe les grands triangles du modèle pour limiter la déformation affine des textures (32-64 conseillé pour sols et murs). Reconvertit le modèle immédiatement."
            />
          </div>
        )}
      </div>
      {onScriptChange && (
        <div className="field-group">
          <div className="field-group-title">Script</div>
          <input
            className="name-input"
            value={draftScript}
            placeholder="(aucun script)"
            spellCheck={false}
            onChange={(e) => setDraftScript(e.target.value)}
            onBlur={() => {
              const trimmed = draftScript.trim();
              if (trimmed !== (script ?? "")) onScriptChange(trimmed || null);
            }}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            title="Nom du script runtime (résolu par hash au chargement de la scène, ex. player, npc)"
          />
          <div className="hint">
            Doit correspondre à un ScriptDef enregistré dans le jeu
            (<code>g_scripts</code>).
          </div>
        </div>
      )}
      <div className="hint">
        Position en unités monde (+Y vers le bas).{" "}
        {isTauri
          ? "Les éditions modifient le scene.json — Enregistrer pour écrire sur disque."
          : "Éditions locales au viewport (mode visionneuse)."}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- app -- */

export default function App() {
  const [scene, setScene] = useState<PscScene | null>(null);
  const [fileName, setFileName] = useState<string>("");
  const [selected, setSelected] = useState(-1);
  const [overrides, setOverrides] = useState<Map<number, Transform>>(new Map());
  const [error, setError] = useState<string>("");
  const [notice, setNotice] = useState<string>("");
  const [viewMode, setViewMode] = useState<"scene" | "vram">("scene");
  const [gizmoMode, setGizmoMode] = useState<GizmoMode>("translate");

  /* Mode projet (Tauri). */
  const [project, setProject] = useState<ProjectInfo | null>(null);
  const [scenePath, setScenePath] = useState<string>("");
  const [sceneDoc, setSceneDoc] = useState<SceneDoc | null>(null);
  const [entityNames, setEntityNames] = useState<string[]>([]);
  const [dirty, setDirty] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [renameFocus, setRenameFocus] = useState(0);
  const rebuildTimer = useRef<number>(0);
  const clipboardRef = useRef<Record<string, unknown> | null>(null);

  /* Live tweaking : quand le jeu tourne dans PCSX-Redux, les éditions de
     transform sont aussi écrites dans la RAM console (via la balise du
     runtime). gameRunningRef évite de recréer editTransform à chaque poll. */
  const gameRunningRef = useRef(false);
  const liveSyncTimer = useRef<number>(0);
  const onRunningChange = useCallback((running: boolean) => {
    gameRunningRef.current = running;
  }, []);

  /* Pendant un drag de gizmo, les rebuilds .psc sont différés : un rebuild
     remplace le graphe three et casserait la manipulation en cours. */
  const gizmoDraggingRef = useRef(false);
  const pendingRebuildRef = useRef<SceneDoc | null>(null);

  /* Historique Ctrl+Z / Ctrl+Y : snapshots du scene.json avant chaque
     mutation. Les éditions continues (drag de gizmo, saisie) portant la
     même clé dans une fenêtre glissante fusionnent en une seule étape. */
  const undoStack = useRef<SceneDoc[]>([]);
  const redoStack = useRef<SceneDoc[]>([]);
  const histMark = useRef({ key: "", time: 0 });
  const pushHistory = useCallback((doc: SceneDoc | null, key = "") => {
    if (!doc) return;
    const now = performance.now();
    if (key && histMark.current.key === key && now - histMark.current.time < 1200) {
      histMark.current.time = now;
      return;
    }
    histMark.current = { key, time: now };
    undoStack.current.push(structuredClone(doc));
    if (undoStack.current.length > 100) undoStack.current.shift();
    redoStack.current = [];
  }, []);

  const loadBuffer = useCallback((name: string, buffer: ArrayBuffer) => {
    try {
      setScene(parsePsc(buffer));
      setFileName(name);
      setSelected(-1);
      setOverrides(new Map());
      setError("");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  /* Rebuild du .psc depuis le JSON (mode projet). */
  const rebuild = useCallback(
    async (doc: SceneDoc, path: string, dir: string, selectName?: string) => {
      try {
        const built = await api.buildScene(dir, path, JSON.stringify(doc));
        const bytes = new Uint8Array(built.psc);
        setScene(parsePsc(bytes.buffer));
        setEntityNames(built.entity_names);
        setOverrides(new Map());
        setError(built.warnings.join(" · "));
        if (selectName !== undefined) {
          setSelected(built.entity_names.indexOf(selectName));
        }
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  /* Mutation structurelle du JSON : rebuild immédiat. */
  const mutateDoc = useCallback(
    (mutator: (doc: SceneDoc) => void, selectName?: string) => {
      if (!sceneDoc || !project) return;
      pushHistory(sceneDoc);
      const doc = structuredClone(sceneDoc);
      mutator(doc);
      setSceneDoc(doc);
      setDirty(true);
      rebuild(doc, scenePath, project.dir, selectName);
    },
    [sceneDoc, project, scenePath, rebuild, pushHistory],
  );

  /* Annuler / rétablir : restaure un snapshot du scene.json. */
  const restoreDoc = useCallback(
    (doc: SceneDoc) => {
      if (!project) return;
      setSceneDoc(doc);
      setDirty(true);
      setSelected(-1);
      histMark.current = { key: "", time: 0 };
      rebuild(doc, scenePath, project.dir);
    },
    [project, scenePath, rebuild],
  );

  const undo = useCallback(() => {
    const doc = undoStack.current.pop();
    if (!doc || !sceneDoc) return;
    redoStack.current.push(structuredClone(sceneDoc));
    restoreDoc(doc);
  }, [sceneDoc, restoreDoc]);

  const redo = useCallback(() => {
    const doc = redoStack.current.pop();
    if (!doc || !sceneDoc) return;
    undoStack.current.push(structuredClone(sceneDoc));
    restoreDoc(doc);
  }, [sceneDoc, restoreDoc]);

  const selectScene = useCallback(
    async (proj: ProjectInfo, path: string) => {
      try {
        const text = await api.loadScene(proj.dir, path);
        const doc = JSON.parse(text) as SceneDoc;
        setScenePath(path);
        setSceneDoc(doc);
        setSelected(-1);
        setDirty(false);
        setFileName(path);
        undoStack.current = [];
        redoStack.current = [];
        histMark.current = { key: "", time: 0 };
        await rebuild(doc, path, proj.dir);
      } catch (e) {
        setError(String(e));
      }
    },
    [rebuild],
  );

  const openProject = useCallback(async () => {
    const dir = await pickProjectDir();
    if (!dir) return;
    try {
      const proj = await api.openProject(dir);
      setProject(proj);
      setError("");
      if (proj.scenes.length > 0) await selectScene(proj, proj.scenes[0].path);
    } catch (e) {
      setError(String(e));
    }
  }, [selectScene]);

  /* Édition d'une transform : viewport immédiat + JSON + rebuild différé. */
  const editTransform = useCallback(
    (index: number, t: Transform) => {
      const next = new Map(overrides);
      next.set(index, t);
      setOverrides(next);
      if (!isTauri || !sceneDoc || !project) return;

      /* Live tweaking : pousser la transform dans la RAM console. Légère
         temporisation pour ne pas mitrailler l'API pendant une saisie. */
      if (gameRunningRef.current) {
        window.clearTimeout(liveSyncTimer.current);
        liveSyncTimer.current = window.setTimeout(() => {
          api
            .reduxSyncEntity(
              PLAY_PORT,
              index,
              t.pos.map(Math.round) as [number, number, number],
              t.rot.map(Math.round) as [number, number, number],
              t.scale.map((s) => Math.round(s * 4096)) as [number, number, number],
            )
            .then(() => setNotice((n) => (n.startsWith("live tweak") ? "" : n)))
            .catch((e) => setNotice(`live tweak : ${e}`));
        }, 80);
      }

      const name = entityNames[index];
      pushHistory(sceneDoc, `transform:${name}`);
      const doc = structuredClone(sceneDoc);
      const entity = doc.entities?.find((e) => e.name === name);
      if (!entity) return;
      entity.position = t.pos;
      entity.rotation = t.rot.map((u) => Math.round((u / 4096) * 3600) / 10);
      entity.scale = t.scale;
      setSceneDoc(doc);
      setDirty(true);

      window.clearTimeout(rebuildTimer.current);
      if (gizmoDraggingRef.current) {
        pendingRebuildRef.current = doc;
      } else {
        rebuildTimer.current = window.setTimeout(
          () => rebuild(doc, scenePath, project.dir),
          400,
        );
      }
    },
    [overrides, sceneDoc, project, entityNames, scenePath, rebuild, pushHistory],
  );

  const onGizmoDragging = useCallback(
    (dragging: boolean) => {
      gizmoDraggingRef.current = dragging;
      if (!dragging && pendingRebuildRef.current && project) {
        const doc = pendingRebuildRef.current;
        pendingRebuildRef.current = null;
        window.clearTimeout(rebuildTimer.current);
        rebuildTimer.current = window.setTimeout(
          () => rebuild(doc, scenePath, project.dir),
          200,
        );
      }
    },
    [project, scenePath, rebuild],
  );

  /* Opérations d'entités (mode projet). */
  const uniqueName = useCallback(
    (base: string) => {
      const taken = new Set(sceneDoc?.entities?.map((e) => e.name as string) ?? []);
      if (!taken.has(base)) return base;
      let n = 2;
      while (taken.has(`${base}-${n}`)) n++;
      return `${base}-${n}`;
    },
    [sceneDoc],
  );

  const addEntity = useCallback(() => {
    const name = uniqueName("entite");
    mutateDoc((doc) => {
      doc.entities = doc.entities ?? [];
      doc.entities.push({ name, position: [0, 0, 0] });
    }, name);
  }, [mutateDoc, uniqueName]);

  const duplicateEntity = useCallback(() => {
    if (selected < 0) return;
    const srcName = entityNames[selected];
    const name = uniqueName(srcName);
    mutateDoc((doc) => {
      const src = doc.entities?.find((e) => e.name === srcName);
      if (!src) return;
      const clone = structuredClone(src);
      clone.name = name;
      const pos = (clone.position as number[]) ?? [0, 0, 0];
      clone.position = [pos[0] + 50, pos[1], pos[2]];
      doc.entities!.push(clone);
    }, name);
  }, [mutateDoc, uniqueName, selected, entityNames]);

  const deleteEntity = useCallback(() => {
    if (selected < 0) return;
    const name = entityNames[selected];
    const hasChildren = sceneDoc?.entities?.some((e) => e.parent === name);
    if (hasChildren) {
      setError(`« ${name} » a des enfants — supprime ou reparente-les d'abord`);
      return;
    }
    setSelected(-1);
    mutateDoc((doc) => {
      doc.entities = doc.entities?.filter((e) => e.name !== name);
    });
  }, [mutateDoc, selected, entityNames, sceneDoc]);

  const renameEntity = useCallback(
    (newName: string) => {
      if (selected < 0) return;
      const oldName = entityNames[selected];
      const name = uniqueName(newName);
      mutateDoc((doc) => {
        for (const e of doc.entities ?? []) {
          if (e.name === oldName) e.name = name;
          if (e.parent === oldName) e.parent = name;
        }
      }, name);
    },
    [mutateDoc, selected, entityNames, uniqueName],
  );

  const setEntityModel = useCallback(
    (modelId: string | null) => {
      if (selected < 0) return;
      const name = entityNames[selected];
      mutateDoc((doc) => {
        const entity = doc.entities?.find((e) => e.name === name);
        if (!entity) return;
        if (modelId) entity.model = modelId;
        else delete entity.model;
      }, name);
    },
    [mutateDoc, selected, entityNames],
  );

  const setEntityScript = useCallback(
    (script: string | null) => {
      if (selected < 0) return;
      const name = entityNames[selected];
      mutateDoc((doc) => {
        const entity = doc.entities?.find((e) => e.name === name);
        if (!entity) return;
        if (script) entity.script = script;
        else delete entity.script;
      }, name);
    },
    [mutateDoc, selected, entityNames],
  );

  const copyEntity = useCallback(() => {
    if (selected < 0 || !sceneDoc) return;
    const src = sceneDoc.entities?.find((e) => e.name === entityNames[selected]);
    if (src) clipboardRef.current = structuredClone(src);
  }, [selected, sceneDoc, entityNames]);

  const pasteEntity = useCallback(() => {
    const src = clipboardRef.current;
    if (!src) return;
    const name = uniqueName(src.name as string);
    mutateDoc((doc) => {
      const clone = structuredClone(src);
      clone.name = name;
      const pos = (clone.position as number[]) ?? [0, 0, 0];
      clone.position = [pos[0] + 50, pos[1], pos[2]];
      doc.entities = doc.entities ?? [];
      doc.entities.push(clone);
    }, name);
  }, [mutateDoc, uniqueName]);

  /* Raccourcis de mode gizmo (1/2/3), valables aussi en mode visionneuse. */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.ctrlKey || e.altKey || e.metaKey) return;
      if (e.code === "Digit1") setGizmoMode("translate");
      else if (e.code === "Digit2") setGizmoMode("rotate");
      else if (e.code === "Digit3") setGizmoMode("scale");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /* Raccourcis clavier globaux (hors champs de saisie). */
  useEffect(() => {
    if (!isTauri) return;
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (!sceneDoc) return;
      // Undo/redo : touche gravée (e.key), pas le code physique — sur
      // AZERTY le Z n'est pas à la position QWERTY.
      if (e.ctrlKey && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
      } else if (e.ctrlKey && e.key.toLowerCase() === "y") {
        e.preventDefault();
        redo();
      } else if (e.ctrlKey && e.code === "KeyC") {
        copyEntity();
      } else if (e.ctrlKey && e.code === "KeyV") {
        pasteEntity();
      } else if (e.ctrlKey && e.code === "KeyD") {
        e.preventDefault();
        duplicateEntity();
      } else if (e.key === "Delete") {
        deleteEntity();
      } else if (e.key === "F2" && selected >= 0) {
        e.preventDefault();
        setRenameFocus((n) => n + 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [sceneDoc, selected, copyEntity, pasteEntity, duplicateEntity, deleteEntity, undo, redo]);

  const saveScene = useCallback(async () => {
    if (!project || !sceneDoc) return;
    try {
      await api.saveScene(project.dir, scenePath, JSON.stringify(sceneDoc, null, 2) + "\n");
      setDirty(false);
    } catch (e) {
      setError(String(e));
    }
  }, [project, sceneDoc, scenePath]);

  /* Import d'assets par drag & drop natif (mode projet). */
  useEffect(() => {
    if (!isTauri || !project) return;
    let unlisten: (() => void) | undefined;
    onFileDrop(async (paths) => {
      const supported = paths.filter((p) => /\.(gltf|glb|png)$/i.test(p));
      if (supported.length === 0) return;
      // Textures d'abord : permet d'appairer modèle + texture du même nom.
      supported.sort((a, b) => Number(/\.png$/i.test(b)) - Number(/\.png$/i.test(a)));
      const summaries: string[] = [];
      for (const path of supported) {
        try {
          const imported = await api.importAsset(project.dir, path);
          summaries.push(`${imported.id} : ${imported.summary}`);
          mutateDocRef.current((doc) => {
            doc.assets = doc.assets ?? {};
            doc.assets.textures = doc.assets.textures ?? [];
            doc.assets.models = doc.assets.models ?? [];
            if (imported.kind === "Texture") {
              if (!doc.assets.textures.some((t) => t.tim === imported.out)) {
                doc.assets.textures.push({ id: `${imported.id}_tex`, tim: imported.out });
              }
            } else {
              // Texture extraite du glTF/GLB (texture_out), sinon appairage
              // par nom avec une texture importée séparément.
              const texId = `${imported.id}_tex`;
              if (
                imported.texture_out &&
                !doc.assets.textures.some((t) => t.tim === imported.texture_out)
              ) {
                doc.assets.textures.push({ id: texId, tim: imported.texture_out });
              }
              const texture = doc.assets.textures.find((t) => t.id === texId);
              if (!doc.assets.models.some((m) => m.pmd === imported.out)) {
                doc.assets.models.push({
                  id: imported.id,
                  pmd: imported.out,
                  ...(texture ? { texture: texture.id } : {}),
                });
              }
            }
          });
        } catch (e) {
          summaries.push(String(e));
        }
      }
      setNotice(`import : ${summaries.join(" · ")}`);
    }).then((fn) => (unlisten = fn));
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project]);

  /* mutateDoc change à chaque rendu : ref stable pour le listener de drop. */
  const mutateDocRef = useRef(mutateDoc);
  useEffect(() => {
    mutateDocRef.current = mutateDoc;
  }, [mutateDoc]);

  /* Scène de démo (mode navigateur) + drag & drop HTML5. */
  useEffect(() => {
    if (isTauri) return;
    fetch("/scene0.psc")
      .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject()))
      .then((buf) => loadBuffer("scene0.psc (démo)", buf))
      .catch(() => {});
  }, [loadBuffer]);

  useEffect(() => {
    if (isTauri) return;
    const onDrop = (e: DragEvent) => {
      e.preventDefault();
      const file = e.dataTransfer?.files[0];
      if (file) file.arrayBuffer().then((buf) => loadBuffer(file.name, buf));
    };
    const onDragOver = (e: DragEvent) => e.preventDefault();
    window.addEventListener("drop", onDrop);
    window.addEventListener("dragover", onDragOver);
    return () => {
      window.removeEventListener("drop", onDrop);
      window.removeEventListener("dragover", onDragOver);
    };
  }, [loadBuffer]);

  const modelIds = sceneDoc?.assets?.models?.map((m) => m.id);
  const selectedJsonEntity =
    selected >= 0 && sceneDoc
      ? sceneDoc.entities?.find((e) => e.name === entityNames[selected])
      : undefined;
  const currentModelId = (selectedJsonEntity?.model as string | undefined) ?? null;
  const currentScript = (selectedJsonEntity?.script as string | undefined) ?? null;
  const currentModelPmd =
    (currentModelId &&
      sceneDoc?.assets?.models?.find((m) => m.id === currentModelId)?.pmd) ||
    null;

  /* Seuil de subdivision du modèle sélectionné (lu dans project.json). */
  const [modelSubdiv, setModelSubdiv] = useState<number | null>(null);
  useEffect(() => {
    if (!isTauri || !project || !currentModelPmd) {
      setModelSubdiv(null);
      return;
    }
    let stale = false;
    api
      .getModelSubdiv(project.dir, currentModelPmd)
      .then((v) => !stale && setModelSubdiv(v))
      .catch(() => !stale && setModelSubdiv(null));
    return () => {
      stale = true;
    };
  }, [project, currentModelPmd]);

  const applyModelSubdiv = useCallback(
    async (value: number | null) => {
      if (!project || !currentModelPmd || !sceneDoc) return;
      try {
        const summary = await api.setModelSubdiv(project.dir, currentModelPmd, value);
        setModelSubdiv(value);
        setNotice(`subdivision : ${summary}`);
        // Le .pmd de Library a changé : re-packer la scène pour le viewport.
        await rebuild(sceneDoc, scenePath, project.dir);
      } catch (e) {
        setError(String(e));
      }
    },
    [project, currentModelPmd, sceneDoc, scenePath, rebuild],
  );

  const currentTransform: Transform | null =
    scene && selected >= 0
      ? overrides.get(selected) ?? {
          pos: [...scene.entities[selected].pos] as [number, number, number],
          rot: [...scene.entities[selected].rot] as [number, number, number],
          // L'échelle 4.12 du fichier arrondie au millième (1.30004 -> 1.3).
          scale: scene.entities[selected].scale.map(
            (s) => Math.round(s * 1000) / 1000,
          ) as [number, number, number],
        }
      : null;

  return (
    <div className="app">
      <header className="toolbar">
        <span className="logo">PSX STUDIO</span>
        {isTauri ? (
          <>
            <button className="button" onClick={openProject}>
              Ouvrir un projet…
            </button>
            {project && project.scenes.length > 0 && (
              <select
                className="scene-select"
                value={scenePath}
                onChange={(e) => selectScene(project, e.target.value)}
              >
                {project.scenes.map((s) => (
                  <option key={s.path} value={s.path}>
                    {s.name}
                  </option>
                ))}
              </select>
            )}
            {project && (
              <button className="button" onClick={saveScene} disabled={!dirty}>
                {dirty ? "● Enregistrer" : "Enregistré"}
              </button>
            )}
          </>
        ) : (
          <label className="button">
            Ouvrir une scène…
            <input
              type="file"
              accept=".psc"
              hidden
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) file.arrayBuffer().then((buf) => loadBuffer(file.name, buf));
              }}
            />
          </label>
        )}
        {scene && (
          <button
            className="button"
            onClick={() => setViewMode(viewMode === "scene" ? "vram" : "scene")}
          >
            {viewMode === "scene" ? "VRAM" : "Scène"}
          </button>
        )}
        <span className="file-name">{fileName || "aucune scène chargée"}</span>
        {scene && (
          <span className="stats">
            {scene.entities.length} entités · {sceneTriangleCount(scene)} tris ·{" "}
            {scene.textures.length} textures
          </span>
        )}
        {notice && <span className="muted">{notice}</span>}
        {error && <span className="error">{error}</span>}
      </header>
      {isTauri && (
        <PlayBar projectDir={project?.dir ?? null} onRunningChange={onRunningChange} />
      )}
      <main className="layout">
        {scene ? (
          <>
            <Hierarchy
              scene={scene}
              names={entityNames}
              selected={selected}
              onSelect={setSelected}
              onAdd={isTauri && sceneDoc ? addEntity : undefined}
              onContextMenu={
                isTauri && sceneDoc ? (_, x, y) => setMenu({ x, y }) : undefined
              }
            />
            {menu && (
              <ContextMenu
                x={menu.x}
                y={menu.y}
                onClose={() => setMenu(null)}
                actions={[
                  {
                    label: "Renommer",
                    shortcut: "F2",
                    onClick: () => setRenameFocus((n) => n + 1),
                  },
                  { label: "Copier", shortcut: "Ctrl+C", onClick: copyEntity },
                  { label: "Coller", shortcut: "Ctrl+V", onClick: pasteEntity },
                  { label: "Dupliquer", shortcut: "Ctrl+D", onClick: duplicateEntity },
                  {
                    label: "Supprimer",
                    shortcut: "Suppr",
                    onClick: deleteEntity,
                    danger: true,
                  },
                ]}
              />
            )}
            {viewMode === "vram" ? (
              <VramPanel scene={scene} />
            ) : (
              <div className="viewport">
                <div className="gizmo-bar">
                  {(
                    [
                      ["translate", "✥", "Déplacer (1)"],
                      ["rotate", "⟳", "Rotation (2)"],
                      ["scale", "⤢", "Échelle (3)"],
                    ] as const
                  ).map(([mode, icon, label]) => (
                    <button
                      key={mode}
                      className={gizmoMode === mode ? "active" : ""}
                      title={`${label} — Ctrl tenu : snap`}
                      onClick={() => setGizmoMode(mode)}
                    >
                      {icon}
                    </button>
                  ))}
                </div>
                <Viewport
                  scene={scene}
                  overrides={overrides}
                  selected={selected}
                  onSelect={setSelected}
                  gizmoMode={gizmoMode}
                  onTransform={editTransform}
                  onGizmoDragging={onGizmoDragging}
                />
              </div>
            )}
            {currentTransform ? (
              <Inspector
                scene={scene}
                name={entityNames[selected] ?? `entité ${selected}`}
                selected={selected}
                transform={currentTransform}
                onChange={(t) => editTransform(selected, t)}
                modelIds={isTauri && sceneDoc ? modelIds : undefined}
                currentModelId={currentModelId}
                onRename={isTauri && sceneDoc ? renameEntity : undefined}
                onModelChange={isTauri && sceneDoc ? setEntityModel : undefined}
                script={currentScript}
                onScriptChange={isTauri && sceneDoc ? setEntityScript : undefined}
                subdiv={modelSubdiv}
                onSubdivChange={
                  isTauri && sceneDoc && currentModelPmd ? applyModelSubdiv : undefined
                }
                focusNameSignal={renameFocus}
              />
            ) : (
              <div className="panel">
                <div className="panel-title">Inspecteur</div>
                <div className="hint">Clique une entité (hiérarchie ou viewport).</div>
              </div>
            )}
          </>
        ) : (
          <div className="empty">
            {isTauri ? (
              <>
                Ouvre un dossier projet (contenant <code>project.json</code>) —
                par exemple <code>examples/demo</code>.
                <br />
                Glisse ensuite des <code>.gltf</code>/<code>.png</code> pour
                importer des assets.
              </>
            ) : (
              <>
                Glisse un fichier <code>.psc</code> ici, ou utilise « Ouvrir une scène… ».
                <br />
                (les scènes de démo sont dans <code>runtime/player/assets/</code>)
              </>
            )}
          </div>
        )}
      </main>
    </div>
  );
}
