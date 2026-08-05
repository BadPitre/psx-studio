// PSX Studio — éditeur, Phase 4 (part 1) : visionneuse/inspecteur de
// scènes .psc avec viewport PS1 authentique. L'édition du JSON source et
// l'intégration Tauri (Play Mode PCSX-Redux) arrivent ensuite.

import { useCallback, useEffect, useMemo, useState } from "react";
import { parsePsc, sceneTriangleCount, type PscScene } from "./formats/psc";
import { Viewport } from "./viewport/Viewport";

type Transform = {
  pos: [number, number, number];
  rot: [number, number, number];
  scale: [number, number, number];
};

/* --------------------------------------------------------- hiérarchie -- */

function Hierarchy({
  scene,
  selected,
  onSelect,
}: {
  scene: PscScene;
  selected: number;
  onSelect: (i: number) => void;
}) {
  // Profondeur par entité (les parents précèdent toujours les enfants).
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
          entité {i}
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
  selected,
  transform,
  onChange,
}: {
  scene: PscScene;
  selected: number;
  transform: Transform;
  onChange: (t: Transform) => void;
}) {
  const entity = scene.entities[selected];
  const toDeg = (u: number) => Math.round((u / 4096) * 3600) / 10;
  const toUnits = (deg: number) => Math.round((deg / 360) * 4096);
  return (
    <div className="panel">
      <div className="panel-title">Inspecteur — entité {selected}</div>
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
        Position en unités monde (+Y vers le bas). Les éditions sont
        appliquées en direct au viewport (non sauvegardées — Phase 4 suite).
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

  /* Scène de démo servie par vite (editor/public), si présente. */
  useEffect(() => {
    fetch("/scene0.psc")
      .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject()))
      .then((buf) => loadBuffer("scene0.psc (démo)", buf))
      .catch(() => {});
  }, [loadBuffer]);

  /* Drag & drop d'un .psc n'importe où dans la fenêtre. */
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
        <span className="file-name">{fileName || "aucune scène chargée"}</span>
        {scene && (
          <span className="stats">
            {scene.entities.length} entités · {sceneTriangleCount(scene)} tris ·{" "}
            {scene.textures.length} textures
          </span>
        )}
        {error && <span className="error">{error}</span>}
      </header>
      <main className="layout">
        {scene ? (
          <>
            <Hierarchy scene={scene} selected={selected} onSelect={setSelected} />
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
                selected={selected}
                transform={currentTransform}
                onChange={(t) => {
                  const next = new Map(overrides);
                  next.set(selected, t);
                  setOverrides(next);
                }}
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
            Glisse un fichier <code>.psc</code> ici, ou utilise « Ouvrir une scène… ».
            <br />
            (les scènes de démo sont dans <code>runtime/player/assets/</code>)
          </div>
        )}
      </main>
    </div>
  );
}
