// PSX Studio — éditeur, Phase 4.
// Mode navigateur : visionneuse de .psc (drag & drop).
// Mode Tauri (desktop) : projet complet — édition des scene.json (transforms,
// création/duplication/suppression d'entités, renommage, modèle), import
// d'assets par drag & drop, VRAM Viewer, Play Mode PCSX-Redux.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { parsePsc, sceneTriangleCount, type PscScene } from "./formats/psc";
import { Viewport } from "./viewport/Viewport";
import { api, isTauri, onFileDrop, pickProjectDir, type ProjectInfo } from "./bridge";
import { PlayBar } from "./PlayBar";
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
  onDuplicate,
  onDelete,
}: {
  scene: PscScene;
  names: string[];
  selected: number;
  onSelect: (i: number) => void;
  onAdd?: () => void;
  onDuplicate?: () => void;
  onDelete?: () => void;
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
          <button
            className="button"
            onClick={onDuplicate}
            disabled={selected < 0}
            title="Dupliquer la sélection"
          >
            ⧉
          </button>
          <button
            className="button"
            onClick={onDelete}
            disabled={selected < 0}
            title="Supprimer la sélection"
          >
            🗑
          </button>
        </div>
      )}
      {scene.entities.map((e, i) => (
        <div
          key={i}
          className={`tree-item ${selected === i ? "selected" : ""}`}
          style={{ paddingLeft: 8 + depths[i] * 16 }}
          onClick={() => onSelect(i)}
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
}) {
  const entity = scene.entities[selected];
  const toDeg = (u: number) => Math.round((u / 4096) * 3600) / 10;
  const toUnits = (deg: number) => Math.round((deg / 360) * 4096);
  const [draftName, setDraftName] = useState(name);
  useEffect(() => setDraftName(name), [name]);

  return (
    <div className="panel">
      <div className="panel-title">Inspecteur</div>
      {onRename ? (
        <div className="field-group">
          <input
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
      </div>
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

  /* Mode projet (Tauri). */
  const [project, setProject] = useState<ProjectInfo | null>(null);
  const [scenePath, setScenePath] = useState<string>("");
  const [sceneDoc, setSceneDoc] = useState<SceneDoc | null>(null);
  const [entityNames, setEntityNames] = useState<string[]>([]);
  const [dirty, setDirty] = useState(false);
  const rebuildTimer = useRef<number>(0);

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
      const doc = structuredClone(sceneDoc);
      mutator(doc);
      setSceneDoc(doc);
      setDirty(true);
      rebuild(doc, scenePath, project.dir, selectName);
    },
    [sceneDoc, project, scenePath, rebuild],
  );

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

      const name = entityNames[index];
      const doc = structuredClone(sceneDoc);
      const entity = doc.entities?.find((e) => e.name === name);
      if (!entity) return;
      entity.position = t.pos;
      entity.rotation = t.rot.map((u) => Math.round((u / 4096) * 3600) / 10);
      entity.scale = t.scale;
      setSceneDoc(doc);
      setDirty(true);

      window.clearTimeout(rebuildTimer.current);
      rebuildTimer.current = window.setTimeout(
        () => rebuild(doc, scenePath, project.dir),
        400,
      );
    },
    [overrides, sceneDoc, project, entityNames, scenePath, rebuild],
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
  const currentModelId =
    selected >= 0 && sceneDoc
      ? ((sceneDoc.entities?.find((e) => e.name === entityNames[selected])?.model as
          | string
          | undefined) ?? null)
      : null;

  const currentTransform: Transform | null =
    scene && selected >= 0
      ? overrides.get(selected) ?? {
          pos: [...scene.entities[selected].pos] as [number, number, number],
          rot: [...scene.entities[selected].rot] as [number, number, number],
          scale: [...scene.entities[selected].scale] as [number, number, number],
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
      {isTauri && <PlayBar projectDir={project?.dir ?? null} />}
      <main className="layout">
        {scene ? (
          <>
            <Hierarchy
              scene={scene}
              names={entityNames}
              selected={selected}
              onSelect={setSelected}
              onAdd={isTauri && sceneDoc ? addEntity : undefined}
              onDuplicate={isTauri && sceneDoc ? duplicateEntity : undefined}
              onDelete={isTauri && sceneDoc ? deleteEntity : undefined}
            />
            {viewMode === "vram" ? (
              <VramPanel scene={scene} />
            ) : (
              <div className="viewport">
                <Viewport
                  scene={scene}
                  overrides={overrides}
                  selected={selected}
                  onSelect={setSelected}
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
