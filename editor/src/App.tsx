// PSX Studio — éditeur, Phase 4.
// Mode navigateur : visionneuse de .psc (drag & drop).
// Mode Tauri (desktop) : projet complet — édition des scene.json avec
// rebuild à la volée via psxpipe, sauvegarde, et Play Mode PCSX-Redux.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { parsePsc, sceneTriangleCount, type PscScene } from "./formats/psc";
import { Viewport } from "./viewport/Viewport";
import { api, isTauri, pickProjectDir, type ProjectInfo } from "./bridge";
import { PlayBar } from "./PlayBar";

type Transform = {
  pos: [number, number, number];
  rot: [number, number, number];
  scale: [number, number, number];
};

/* --------------------------------------------------------- hiérarchie -- */

function Hierarchy({
  scene,
  names,
  selected,
  onSelect,
}: {
  scene: PscScene;
  names: string[];
  selected: number;
  onSelect: (i: number) => void;
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
}: {
  scene: PscScene;
  name: string;
  selected: number;
  transform: Transform;
  onChange: (t: Transform) => void;
}) {
  const entity = scene.entities[selected];
  const toDeg = (u: number) => Math.round((u / 4096) * 3600) / 10;
  const toUnits = (deg: number) => Math.round((deg / 360) * 4096);
  return (
    <div className="panel">
      <div className="panel-title">Inspecteur — {name}</div>
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
        <div className="field-readonly">
          {entity.model >= 0
            ? `modèle #${entity.model} — ${scene.models[entity.model].prims.length} triangles${
                scene.models[entity.model].textured ? ", texturé" : ""
              }`
            : "aucun modèle (nœud vide)"}
        </div>
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

  /* Mode projet (Tauri). */
  const [project, setProject] = useState<ProjectInfo | null>(null);
  const [scenePath, setScenePath] = useState<string>("");
  const [sceneDoc, setSceneDoc] = useState<Record<string, unknown> | null>(null);
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
    async (doc: Record<string, unknown>, path: string, dir: string) => {
      try {
        const built = await api.buildScene(dir, path, JSON.stringify(doc));
        const bytes = new Uint8Array(built.psc);
        setScene(parsePsc(bytes.buffer));
        setEntityNames(built.entity_names);
        setOverrides(new Map());
        setError(built.warnings.join(" · "));
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const selectScene = useCallback(
    async (proj: ProjectInfo, path: string) => {
      try {
        const text = await api.loadScene(proj.dir, path);
        const doc = JSON.parse(text) as Record<string, unknown>;
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
      const entities = (sceneDoc.entities as Record<string, unknown>[]) ?? [];
      const entity = entities.find((e) => e.name === name);
      if (!entity) return;
      entity.position = t.pos;
      entity.rotation = t.rot.map((u) => Math.round((u / 4096) * 3600) / 10);
      entity.scale = t.scale;
      setSceneDoc({ ...sceneDoc });
      setDirty(true);

      window.clearTimeout(rebuildTimer.current);
      rebuildTimer.current = window.setTimeout(
        () => rebuild(sceneDoc, scenePath, project.dir),
        400,
      );
    },
    [overrides, sceneDoc, project, entityNames, scenePath, rebuild],
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

  /* Scène de démo (mode navigateur) + drag & drop. */
  useEffect(() => {
    if (isTauri) return;
    fetch("/scene0.psc")
      .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject()))
      .then((buf) => loadBuffer("scene0.psc (démo)", buf))
      .catch(() => {});
  }, [loadBuffer]);

  useEffect(() => {
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
        <span className="file-name">{fileName || "aucune scène chargée"}</span>
        {scene && (
          <span className="stats">
            {scene.entities.length} entités · {sceneTriangleCount(scene)} tris ·{" "}
            {scene.textures.length} textures
          </span>
        )}
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
            />
            <div className="viewport">
              <Viewport
                scene={scene}
                overrides={overrides}
                selected={selected}
                onSelect={setSelected}
              />
            </div>
            {currentTransform ? (
              <Inspector
                scene={scene}
                name={entityNames[selected] ?? `entité ${selected}`}
                selected={selected}
                transform={currentTransform}
                onChange={(t) => editTransform(selected, t)}
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
