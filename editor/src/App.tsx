// PSX Studio — éditeur, Phase 4.
// Mode navigateur : visionneuse de .psc (drag & drop).
// Mode Tauri (desktop) : projet complet — édition des scene.json (transforms,
// création/duplication/suppression d'entités, renommage, modèle), import
// d'assets par drag & drop, VRAM Viewer, Play Mode PCSX-Redux.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { parsePsc, sceneTriangleCount, type PscScene } from "./formats/psc";
import { Viewport, type GizmoMode } from "./viewport/Viewport";
import {
  api,
  isTauri,
  onFileDrop,
  pickProjectDir,
  type ProjectFile,
  type ProjectInfo,
} from "./bridge";
import { PlayBar, PLAY_PORT } from "./PlayBar";
import { VramPanel } from "./VramPanel";
import { ProjectPanel, type LogEntry } from "./ProjectPanel";
import { ContextMenu } from "./ContextMenu";
import { thumbFor } from "./thumbs";

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
  onModelDrop,
}: {
  scene: PscScene;
  names: string[];
  selected: number;
  onSelect: (i: number) => void;
  /** Ouvre le menu « ajouter » (GameObject / Lumière / Caméra). */
  onAdd?: (x: number, y: number) => void;
  onContextMenu?: (i: number, x: number, y: number) => void;
  /** Drop d'un modèle du panneau Project : instancier (à l'origine). */
  onModelDrop?: (out: string) => void;
}) {
  const depths = useMemo(() => {
    const d: number[] = [];
    scene.entities.forEach((e, i) => {
      d[i] = e.parent >= 0 ? d[e.parent] + 1 : 0;
    });
    return d;
  }, [scene]);

  return (
    <div
      className="panel"
      onDragOver={(e) => {
        if (onModelDrop) e.preventDefault();
      }}
      onDrop={(e) => {
        const out = e.dataTransfer.getData("text/psx-model");
        if (onModelDrop && out) {
          e.preventDefault();
          onModelDrop(out);
        }
      }}
    >
      <div className="panel-title">Hiérarchie</div>
      {onAdd && (
        <div className="hierarchy-tools">
          <button
            className="button"
            onClick={(e) => {
              e.stopPropagation();
              const r = (e.target as HTMLElement).getBoundingClientRect();
              onAdd(r.left, r.bottom + 4);
            }}
            title="Ajouter : GameObject, lumière ou caméra"
          >
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
          <span className="tree-icon">
            {e.flags & 1 ? "☀" : e.flags & 2 ? "🎥" : e.model >= 0 ? "▣" : "○"}
          </span>
          {names[i] ?? `entité ${i}`}
          {e.model >= 0 && (
            <span className="tree-meta">{scene.models[e.model].prims.length} tris</span>
          )}
        </div>
      ))}
    </div>
  );
}

/* --------------------------------------------------------- inspecteur -- */

const rgbToHex = (c: [number, number, number]) =>
  "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");
const hexToRgb = (hex: string): [number, number, number] => [
  parseInt(hex.slice(1, 3), 16),
  parseInt(hex.slice(3, 5), 16),
  parseInt(hex.slice(5, 7), 16),
];

/** Carte de composant façon Unity : titre, retrait, corps. */
function ComponentCard({
  icon,
  title,
  onRemove,
  children,
}: {
  icon: string;
  title: string;
  onRemove?: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="component-card">
      <div className="component-head">
        <span className="component-title">
          {icon} {title}
        </span>
        {onRemove && (
          <button
            className="component-remove"
            onClick={onRemove}
            title={`Retirer le composant ${title}`}
          >
            ✕
          </button>
        )}
      </div>
      <div className="component-body">{children}</div>
    </div>
  );
}

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
  light,
  onLightChange,
  lightIntensity,
  onLightIntensityChange,
  lightType,
  onLightTypeChange,
  lightRadius,
  onLightRadiusChange,
  isCamera,
  onCameraChange,
  camFov,
  onCamFovChange,
  camDraw,
  onCamDrawChange,
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
  light?: [number, number, number] | null;
  onLightChange?: (color: [number, number, number] | null) => void;
  lightIntensity?: number | null;
  onLightIntensityChange?: (v: number | null) => void;
  lightType?: "directional" | "point";
  onLightTypeChange?: (t: "directional" | "point") => void;
  lightRadius?: number | null;
  onLightRadiusChange?: (r: number | null) => void;
  isCamera?: boolean;
  onCameraChange?: (on: boolean) => void;
  camFov?: number | null;
  onCamFovChange?: (fov: number | null) => void;
  camDraw?: number | null;
  onCamDrawChange?: (d: number | null) => void;
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
  /* Carte Script présente sans script enregistré (juste ajoutée). */
  const [pendingScript, setPendingScript] = useState(false);
  useEffect(() => setPendingScript(false), [selected]);
  /* Menu « Ajouter un composant ». */
  const [addComp, setAddComp] = useState<{ x: number; y: number } | null>(null);
  const editable = Boolean(onModelChange && onLightChange && onCameraChange && onScriptChange);
  const hasMesh = editable ? currentModelId != null : entity.model >= 0;
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
      {entity.parent >= 0 && (
        <div className="field-group">
          <div className="field-readonly">parent : entité {entity.parent}</div>
        </div>
      )}

      {/* Composants façon Unity : une carte par composant, retirable. */}
      {hasMesh && (
        <ComponentCard
          icon="▣"
          title="MeshRenderer"
          onRemove={onModelChange ? () => onModelChange(null) : undefined}
        >
          {onModelChange && modelIds ? (
            <select
              className="scene-select model-select"
              value={currentModelId ?? ""}
              onChange={(e) => onModelChange(e.target.value || null)}
            >
              {modelIds.map((id) => (
                <option key={id} value={id}>
                  {id}
                </option>
              ))}
            </select>
          ) : (
            <div className="field-readonly">
              modèle #{entity.model} —{" "}
              {entity.model >= 0 ? scene.models[entity.model].prims.length : 0} triangles
            </div>
          )}
          {onSubdivChange && entity.model >= 0 && (
            <div className="field">
              <label>Subdivision anti-warping</label>
              <select
                className="scene-select model-select"
                value={draftSubdiv}
                onChange={(e) => {
                  setDraftSubdiv(e.target.value);
                  onSubdivChange(e.target.value === "" ? null : Number(e.target.value));
                }}
                title="Découpe les grands triangles du modèle pour limiter la déformation affine des textures (pour les sols et grands murs). Reconvertit le modèle immédiatement."
              >
                <option value="">désactivée</option>
                <option value="64">légère (arête max 64)</option>
                <option value="48">moyenne (arête max 48)</option>
                <option value="32">forte (arête max 32)</option>
                {draftSubdiv !== "" && !["64", "48", "32"].includes(draftSubdiv) && (
                  <option value={draftSubdiv}>personnalisée ({draftSubdiv})</option>
                )}
              </select>
            </div>
          )}
        </ComponentCard>
      )}

      {light && (
        <ComponentCard
          icon={lightType === "point" ? "🔥" : "☀"}
          title={lightType === "point" ? "Lumière ponctuelle (torche)" : "Lumière directionnelle"}
          onRemove={onLightChange ? () => onLightChange(null) : undefined}
        >
          {onLightTypeChange && (
            <div className="field">
              <label>Type</label>
              <select
                className="scene-select model-select"
                value={lightType ?? "directional"}
                onChange={(e) =>
                  onLightTypeChange(e.target.value as "directional" | "point")
                }
                title="Directionnelle : lignes GTE natives (3 max). Ponctuelle : approximation par objet avec atténuation par la distance (4 max), plus halo additif en jeu."
              >
                <option value="directional">directionnelle (soleil, lune)</option>
                <option value="point">ponctuelle (torche, lampe)</option>
              </select>
            </div>
          )}
          {lightType === "point" && onLightRadiusChange && (
            <div className="field">
              <label>Rayon d'action (unités monde)</label>
              <input
                type="number"
                min={64}
                max={8192}
                step={20}
                value={lightRadius ?? ""}
                placeholder="600"
                onChange={(e) => {
                  const v = e.target.value === "" ? null : Number(e.target.value);
                  if (v === null || (v >= 64 && v <= 8192)) onLightRadiusChange(v);
                }}
                title="Au-delà, l'objet n'est plus éclairé (atténuation linéaire). Un script peut le faire osciller = vacillement."
              />
            </div>
          )}
          <div className="field color-field">
            <label>Couleur</label>
            <input
              type="color"
              value={rgbToHex(light)}
              disabled={!onLightChange}
              onChange={(e) => onLightChange?.(hexToRgb(e.target.value))}
              title="La rotation de l'entité donne la direction : elle éclaire le long de son axe -Z"
            />
          </div>
          {onLightIntensityChange && (
            <div className="field">
              <label>Intensité — {Math.round((lightIntensity ?? 1) * 100)} %</label>
              <input
                type="range"
                min={10}
                max={250}
                step={5}
                value={Math.round((lightIntensity ?? 1) * 100)}
                onChange={(e) => {
                  const pct = Number(e.target.value);
                  onLightIntensityChange(pct === 100 ? null : pct / 100);
                }}
                title="Multiplicateur d'intensité : la matrice couleur du GTE est en 4.12, une lumière peut dépasser 100 %"
              />
            </div>
          )}
          <div className="field-readonly">
            {lightType === "point"
              ? "appliquée par objet (approximation d'époque) · atténuation linéaire · halo additif en jeu · 4 max par scène"
              : "directionnelle GTE · par sommet (Gouraud) · pas d'ombres · direction = rotation de l'entité (-Z local) · 3 max par scène (soleil des settings + 2 entités)"}
          </div>
        </ComponentCard>
      )}

      {isCamera && (
        <ComponentCard
          icon="🎥"
          title="Caméra"
          onRemove={onCameraChange ? () => onCameraChange(false) : undefined}
        >
          {onCamFovChange && (
            <>
              <div className="field">
                <label>FOV vertical (°)</label>
                <input
                  type="number"
                  min={10}
                  max={170}
                  step={1}
                  value={camFov ?? ""}
                  placeholder="74 (natif PS1)"
                  onChange={(e) => {
                    const v = e.target.value === "" ? null : Number(e.target.value);
                    if (v === null || (v >= 10 && v <= 170)) onCamFovChange(v);
                  }}
                  title="Champ de vision vertical. Le défaut console (distance de projection h = 160) vaut ~74°. Le runtime le convertit en gte_SetGeomScreen."
                />
              </div>
              <div className="field">
                <label>Distance d'affichage (unités, vide = ∞)</label>
                <input
                  type="number"
                  min={100}
                  max={32767}
                  step={100}
                  value={camDraw ?? ""}
                  placeholder="illimitée"
                  onChange={(e) => {
                    const v = e.target.value === "" ? null : Number(e.target.value);
                    if (v === null || (v >= 100 && v <= 32767)) onCamDrawChange?.(v);
                  }}
                  title="Les entités au-delà ne sont pas dessinées (culling par objet, le « pop » maîtrisé des jeux PS1). Le far plane du PiP la simule."
                />
              </div>
            </>
          )}
          <div className="field-readonly">
            vue initiale de la scène · sortie 320×240 · 4:3 · near ≈ 16
            unités (garde GTE) · profondeur triée par Ordering Table ·
            regard le long du -Z local
          </div>
        </ComponentCard>
      )}

      {(script != null || pendingScript) && onScriptChange && (
        <ComponentCard
          icon="📜"
          title="Script"
          onRemove={() => {
            setPendingScript(false);
            setDraftScript("");
            if (script != null) onScriptChange(null);
          }}
        >
          <input
            className="name-input"
            value={draftScript}
            placeholder="nom du script (ex. player, npc)"
            spellCheck={false}
            autoFocus={pendingScript && !script}
            onChange={(e) => setDraftScript(e.target.value)}
            onBlur={() => {
              const trimmed = draftScript.trim();
              if (trimmed !== (script ?? "")) onScriptChange(trimmed || null);
              if (!trimmed && script == null) setPendingScript(false);
            }}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            title="Nom du script runtime (résolu par hash au chargement de la scène)"
          />
          <div className="hint">
            Doit correspondre à un ScriptDef enregistré dans le jeu
            (<code>g_scripts</code>).
          </div>
        </ComponentCard>
      )}

      {editable && (
        <div className="add-component">
          <button
            className="button"
            onClick={(e) => {
              const r = (e.target as HTMLElement).getBoundingClientRect();
              setAddComp({ x: r.left, y: r.bottom + 4 });
            }}
          >
            ＋ Ajouter un composant
          </button>
        </div>
      )}
      {addComp && (
        <ContextMenu
          x={addComp.x}
          y={addComp.y}
          onClose={() => setAddComp(null)}
          actions={[
            ...(!hasMesh && modelIds && modelIds.length > 0
              ? [
                  {
                    label: "▣ MeshRenderer",
                    onClick: () => onModelChange?.(modelIds[0]),
                  },
                ]
              : []),
            ...(!light
              ? [
                  {
                    label: "☀ Lumière directionnelle",
                    onClick: () => onLightChange?.([255, 235, 200]),
                  },
                ]
              : []),
            ...(!isCamera
              ? [{ label: "🎥 Caméra", onClick: () => onCameraChange?.(true) }]
              : []),
            ...(script == null && !pendingScript
              ? [{ label: "📜 Script", onClick: () => setPendingScript(true) }]
              : []),
          ]}
        />
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
  const [dither, setDither] = useState(true);
  const [culling, setCulling] = useState(false);

  /* Mode projet (Tauri). */
  const [project, setProject] = useState<ProjectInfo | null>(null);
  const [projectFiles, setProjectFiles] = useState<ProjectFile[]>([]);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [scenePath, setScenePath] = useState<string>("");
  const [sceneDoc, setSceneDoc] = useState<SceneDoc | null>(null);
  const [entityNames, setEntityNames] = useState<string[]>([]);
  const [dirty, setDirty] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [addMenu, setAddMenu] = useState<{ x: number; y: number } | null>(null);
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

  /* Console : chaque notice/erreur affichée alimente aussi le journal. */
  useEffect(() => {
    if (!notice) return;
    setLogs((l) => [
      ...l.slice(-499),
      { time: new Date().toLocaleTimeString(), level: "info", text: notice },
    ]);
  }, [notice]);
  useEffect(() => {
    if (!error) return;
    setLogs((l) => [
      ...l.slice(-499),
      { time: new Date().toLocaleTimeString(), level: "error", text: error },
    ]);
  }, [error]);

  /* Panneau Project : rafraîchit la liste disque + project.json. */
  const refreshFiles = useCallback(async (dir: string) => {
    try {
      setProjectFiles(await api.listProjectFiles(dir));
    } catch (e) {
      setNotice(`panneau Project : ${e}`);
    }
  }, []);

  /* Vignettes du panneau : rendues depuis Library/ (cache en module). */
  const getThumb = useCallback(
    (f: ProjectFile) =>
      project ? thumbFor(project.dir, f, projectFiles) : Promise.resolve<string | null>(null),
    [project, projectFiles],
  );

  const openProject = useCallback(async () => {
    const dir = await pickProjectDir();
    if (!dir) return;
    try {
      const proj = await api.openProject(dir);
      setProject(proj);
      setError("");
      if (proj.scenes.length > 0) await selectScene(proj, proj.scenes[0].path);
      refreshFiles(dir);
    } catch (e) {
      setError(String(e));
    }
  }, [selectScene, refreshFiles]);

  /* Actions du panneau Project. */
  const importFromPanel = useCallback(
    async (rel: string) => {
      if (!project) return;
      try {
        const imported = await api.importAsset(project.dir, `${project.dir}/${rel}`);
        setNotice(`${imported.id} : ${imported.summary}`);
        refreshFiles(project.dir);
      } catch (e) {
        setError(String(e));
      }
    },
    [project, refreshFiles],
  );

  const createSceneFromPanel = useCallback(
    async (name: string) => {
      if (!project) return;
      try {
        const rel = await api.createScene(project.dir, name);
        // project.json a changé : recharge la liste des scènes du menu.
        const proj = await api.openProject(project.dir);
        setProject(proj);
        setNotice(`scène créée : ${rel}`);
        await selectScene(proj, rel);
        refreshFiles(project.dir);
      } catch (e) {
        setError(String(e));
      }
    },
    [project, selectScene, refreshFiles],
  );

  const createFolderFromPanel = useCallback(
    async (parent: string, name: string) => {
      if (!project) return;
      try {
        const rel = await api.createFolder(project.dir, parent, name);
        setNotice(`dossier créé : ${rel}`);
        refreshFiles(project.dir);
      } catch (e) {
        setError(String(e));
      }
    },
    [project, refreshFiles],
  );

  /* Rangement par drag & drop : project.json suit, et si la scène
     ouverte a bougé, elle est rouverte à son nouveau chemin. */
  const moveFromPanel = useCallback(
    async (from: string, toDir: string) => {
      if (!project) return;
      try {
        const rel = await api.moveEntry(project.dir, from, toDir);
        setNotice(`déplacé : ${from} → ${rel}`);
        if (from.startsWith("scenes/") || toDir.startsWith("scenes")) {
          const proj = await api.openProject(project.dir);
          setProject(proj);
          if (from === scenePath) await selectScene(proj, rel);
        }
        refreshFiles(project.dir);
      } catch (e) {
        setError(String(e));
      }
    },
    [project, scenePath, selectScene, refreshFiles],
  );

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

  /* Ajout par type : GameObject vide, lumière (orientée vers le sol,
     couleur chaude) ou caméra (recul + regard vers l'origine, la scène
     dans le cadre). */
  const addEntity = useCallback(
    (kind: "empty" | "light" | "camera") => {
      const base =
        kind === "light" ? "lumiere" : kind === "camera" ? "camera" : "entite";
      const name = uniqueName(base);
      mutateDoc((doc) => {
        doc.entities = doc.entities ?? [];
        if (kind === "light") {
          doc.entities.push({
            name,
            position: [0, -200, 0],
            rotation: [45, 30, 0],
            light: { color: [255, 235, 200] },
          });
        } else if (kind === "camera") {
          doc.entities.push({
            name,
            position: [0, -200, -420],
            rotation: [-21, 180, 0],
            camera: true,
          });
        } else {
          doc.entities.push({ name, position: [0, 0, 0] });
        }
      }, name);
    },
    [mutateDoc, uniqueName],
  );

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

  /* Instanciation d'un modèle du panneau Project (drop viewport = à la
     position visée au sol, drop hiérarchie = à l'origine). Le modèle est
     ajouté aux assets de la scène s'il n'y est pas, avec sa texture
     appariée par nom de sortie (guy.pmd -> guy.tim). */
  const addModelEntity = useCallback(
    (out: string, pos: [number, number, number]) => {
      if (!sceneDoc) return;
      const stem = out.replace(/\.pmd$/i, "");
      const timOut = `${stem}.tim`;
      const hasTim = projectFiles.some((f) => f.out === timOut);
      const name = uniqueName(stem);
      mutateDoc((doc) => {
        doc.assets = doc.assets ?? {};
        doc.assets.models = doc.assets.models ?? [];
        doc.assets.textures = doc.assets.textures ?? [];
        let model = doc.assets.models.find((m) => m.pmd === out);
        if (!model) {
          const texId = `${stem}_tex`;
          if (hasTim && !doc.assets.textures.some((t) => t.tim === timOut)) {
            doc.assets.textures.push({ id: texId, tim: timOut });
          }
          const texture = doc.assets.textures.find((t) => t.tim === timOut);
          let id = stem;
          let n = 2;
          while (doc.assets.models.some((m) => m.id === id)) id = `${stem}_${n++}`;
          model = { id, pmd: out, ...(texture ? { texture: texture.id } : {}) };
          doc.assets.models.push(model);
        }
        doc.entities = doc.entities ?? [];
        doc.entities.push({ name, position: pos, model: model.id });
      }, name);
      setNotice(`${name} instancié depuis ${out}`);
    },
    [sceneDoc, projectFiles, uniqueName, mutateDoc],
  );

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

  const setEntityLight = useCallback(
    (color: [number, number, number] | null) => {
      if (selected < 0) return;
      const name = entityNames[selected];
      mutateDoc((doc) => {
        const entity = doc.entities?.find((e) => e.name === name);
        if (!entity) return;
        if (color) {
          entity.light = { color };
          // Lumière et caméra sont exclusives : une entité a un seul rôle.
          delete entity.camera;
        } else {
          delete entity.light;
        }
      }, name);
    },
    [mutateDoc, selected, entityNames],
  );

  const setEntityCamera = useCallback(
    (on: boolean) => {
      if (selected < 0) return;
      const name = entityNames[selected];
      mutateDoc((doc) => {
        const entity = doc.entities?.find((e) => e.name === name);
        if (!entity) return;
        // Ne pas écraser un FOV déjà réglé en re-cochant la case.
        if (on && !entity.camera) entity.camera = true;
        else if (!on) delete entity.camera;
        // Lumière et caméra sont exclusives : une entité a un seul rôle.
        if (on) delete entity.light;
      }, name);
    },
    [mutateDoc, selected, entityNames],
  );

  /* Propriétés caméra (fov, draw_distance) fusionnées dans l'objet
     camera du JSON ; toutes au défaut -> retour à `true`. */
  const setEntityCamProp = useCallback(
    (key: "fov" | "draw_distance", value: number | null) => {
      if (selected < 0) return;
      const name = entityNames[selected];
      mutateDoc((doc) => {
        const entity = doc.entities?.find((e) => e.name === name);
        if (!entity || !entity.camera) return;
        const props: Record<string, number> =
          typeof entity.camera === "object"
            ? { ...(entity.camera as Record<string, number>) }
            : {};
        if (value === null) delete props[key];
        else props[key] = value;
        entity.camera = Object.keys(props).length > 0 ? props : true;
      }, name);
    },
    [mutateDoc, selected, entityNames],
  );

  /* Propriétés lumière fusionnées ({ type, color, intensity, radius }) ;
     null retire la clé, "directional" retire aussi le rayon. */
  const setEntityLightProp = useCallback(
    (key: "type" | "intensity" | "radius", value: string | number | null) => {
      if (selected < 0) return;
      const name = entityNames[selected];
      mutateDoc((doc) => {
        const entity = doc.entities?.find((e) => e.name === name);
        if (!entity || !entity.light) return;
        const light = { ...(entity.light as Record<string, unknown>) };
        if (value === null || (key === "type" && value === "directional")) {
          delete light[key];
          if (key === "type") delete light.radius;
        } else {
          light[key] = value;
        }
        entity.light = light;
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
      refreshFiles(project.dir);
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
  /* Composants : source JSON en mode projet, .psc en mode visionneuse
     (cartes en lecture seule). */
  const pscEntity = scene && selected >= 0 ? scene.entities[selected] : undefined;
  const pscLight = scene?.lights.find((l) => l.entity === selected);
  const currentLight =
    ((selectedJsonEntity?.light as { color?: [number, number, number] } | undefined)
      ?.color as [number, number, number] | undefined) ??
    (pscLight ? pscLight.color : null);
  const currentCamera =
    Boolean(selectedJsonEntity?.camera) ||
    (!sceneDoc && Boolean((pscEntity?.flags ?? 0) & 2));
  const camProps =
    typeof selectedJsonEntity?.camera === "object"
      ? (selectedJsonEntity.camera as { fov?: number; draw_distance?: number })
      : {};
  const currentCamFov =
    camProps.fov ?? ((!sceneDoc && pscEntity?.camFov) || null);
  const currentCamDraw =
    camProps.draw_distance ?? ((!sceneDoc && pscEntity?.camDraw) || null);
  const currentLightIntensity =
    ((selectedJsonEntity?.light as { intensity?: number } | undefined)
      ?.intensity as number | undefined) ??
    (pscLight && pscLight.intensity !== 1 ? pscLight.intensity : null);
  const currentLightType: "directional" | "point" =
    ((selectedJsonEntity?.light as { type?: string } | undefined)?.type ??
      (!sceneDoc && (pscEntity?.flags ?? 0) & 4 ? "point" : "directional")) as
      | "directional"
      | "point";
  const currentLightRadius =
    ((selectedJsonEntity?.light as { radius?: number } | undefined)?.radius as
      | number
      | undefined) ??
    ((!sceneDoc && pscEntity?.lightRadius) || null);
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
              onAdd={isTauri && sceneDoc ? (x, y) => setAddMenu({ x, y }) : undefined}
              onContextMenu={
                isTauri && sceneDoc ? (_, x, y) => setMenu({ x, y }) : undefined
              }
              onModelDrop={
                isTauri && sceneDoc ? (out) => addModelEntity(out, [0, 0, 0]) : undefined
              }
            />
            {addMenu && (
              <ContextMenu
                x={addMenu.x}
                y={addMenu.y}
                onClose={() => setAddMenu(null)}
                actions={[
                  { label: "▣ GameObject", onClick: () => addEntity("empty") },
                  { label: "☀ Lumière directionnelle", onClick: () => addEntity("light") },
                  { label: "🎥 Caméra", onClick: () => addEntity("camera") },
                ]}
              />
            )}
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
                <div className="gizmo-bar view-bar">
                  <button
                    className={dither ? "active" : ""}
                    title="Dithering console : matrice Bayer 4x4 + sortie 15 bits, comme le GPU PS1"
                    onClick={() => setDither(!dither)}
                  >
                    ▦
                  </button>
                  <button
                    className={culling ? "active" : ""}
                    title="Culling console : cache les faces arrière comme le nclip GTE (off = double face, plus ergonomique)"
                    onClick={() => setCulling(!culling)}
                  >
                    ◪
                  </button>
                </div>
                <Viewport
                  scene={scene}
                  overrides={overrides}
                  selected={selected}
                  onSelect={setSelected}
                  gizmoMode={gizmoMode}
                  onTransform={editTransform}
                  onGizmoDragging={onGizmoDragging}
                  dither={dither}
                  culling={culling}
                  onModelDrop={isTauri && sceneDoc ? addModelEntity : undefined}
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
                light={currentLight}
                onLightChange={isTauri && sceneDoc ? setEntityLight : undefined}
                lightIntensity={currentLightIntensity}
                onLightIntensityChange={
                  isTauri && sceneDoc
                    ? (v) => setEntityLightProp("intensity", v)
                    : undefined
                }
                lightType={currentLightType}
                onLightTypeChange={
                  isTauri && sceneDoc
                    ? (t) => setEntityLightProp("type", t)
                    : undefined
                }
                lightRadius={currentLightRadius}
                onLightRadiusChange={
                  isTauri && sceneDoc
                    ? (r) => setEntityLightProp("radius", r)
                    : undefined
                }
                isCamera={currentCamera}
                onCameraChange={isTauri && sceneDoc ? setEntityCamera : undefined}
                camFov={currentCamFov}
                onCamFovChange={
                  isTauri && sceneDoc ? (v) => setEntityCamProp("fov", v) : undefined
                }
                camDraw={currentCamDraw}
                onCamDrawChange={
                  isTauri && sceneDoc
                    ? (v) => setEntityCamProp("draw_distance", v)
                    : undefined
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
      {isTauri && project && (
        <ProjectPanel
          files={projectFiles}
          logs={logs}
          currentScenePath={scenePath}
          onOpenScene={(path) => selectScene(project, path)}
          onImport={importFromPanel}
          onCreateScene={createSceneFromPanel}
          onCreateFolder={createFolderFromPanel}
          onMove={moveFromPanel}
          onRefresh={() => refreshFiles(project.dir)}
          onClearLogs={() => setLogs([])}
          getThumb={getThumb}
        />
      )}
    </div>
  );
}
