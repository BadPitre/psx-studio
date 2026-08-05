// Panneau « Project » façon Unity, en bas de l'éditeur : sections du
// projet à gauche, grille de tuiles à droite, recherche. Le contenu croise
// le disque et project.json — les badges signalent un fichier présent mais
// pas importé, ou une entrée dont le fichier a disparu.
//
// Clic droit dans le vide : menu « Créer ▸ » (Scène pour l'instant,
// extensible — prefabs, bases de données… viendront s'y ajouter).

import { useMemo, useRef, useState } from "react";
import type { ProjectFile } from "./bridge";
import { ContextMenu, type MenuAction } from "./ContextMenu";

const SECTIONS: { id: string; label: string }[] = [
  { id: "", label: "Tout" },
  { id: "scenes", label: "Scènes" },
  { id: "assets", label: "Assets" },
  { id: "audio", label: "Audio" },
];

const ICONS: Record<ProjectFile["kind"], string> = {
  scene: "🎬",
  model: "▣",
  texture: "🖼",
  audio: "♪",
  buffer: "⛓",
  other: "📄",
};

function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} Mo`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} Ko`;
  return `${bytes} o`;
}

export function ProjectPanel({
  files,
  currentScenePath,
  onOpenScene,
  onImport,
  onCreateScene,
  onRefresh,
}: {
  files: ProjectFile[];
  currentScenePath: string;
  /** Ouvre une scène (chemin relatif projet). */
  onOpenScene: (path: string) => void;
  /** (Ré)importe un asset gltf/glb/png (chemin relatif projet). */
  onImport: (path: string) => void;
  onCreateScene: (name: string) => void;
  onRefresh: () => void;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const [section, setSection] = useState("");
  const [search, setSearch] = useState("");
  const [menu, setMenu] = useState<{ x: number; y: number; file: ProjectFile | null } | null>(
    null,
  );
  const [naming, setNaming] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);

  const shown = useMemo(() => {
    const query = search.trim().toLowerCase();
    return files.filter(
      (f) =>
        (!section || f.section === section) &&
        (!query || f.name.toLowerCase().includes(query)),
    );
  }, [files, section, search]);

  const importable = (f: ProjectFile) =>
    f.exists && (f.kind === "model" || f.kind === "texture");

  const activate = (f: ProjectFile) => {
    if (f.kind === "scene" && f.exists) onOpenScene(f.path);
    else if (importable(f) && !f.registered) onImport(f.path);
  };

  const actionsFor = (f: ProjectFile | null): MenuAction[] => {
    const create: MenuAction = {
      label: "Créer",
      children: [
        { label: "🎬 Scène…", onClick: () => setNaming(true) },
        // Demain : prefabs, bases de données, etc.
      ],
    };
    if (!f) return [create, { label: "Rafraîchir", onClick: onRefresh }];
    const actions: MenuAction[] = [];
    if (f.kind === "scene" && f.exists)
      actions.push({ label: "Ouvrir", onClick: () => onOpenScene(f.path) });
    if (importable(f))
      actions.push({
        label: f.registered ? "Réimporter" : "Importer dans le projet",
        onClick: () => onImport(f.path),
      });
    actions.push(create, { label: "Rafraîchir", onClick: onRefresh });
    return actions;
  };

  const submitName = () => {
    const name = nameRef.current?.value.trim();
    setNaming(false);
    if (name) onCreateScene(name);
  };

  return (
    <div className={`project-panel ${collapsed ? "collapsed" : ""}`}>
      <div className="project-tabs">
        <button className="project-tab active" onClick={() => setCollapsed(!collapsed)}>
          {collapsed ? "▸" : "▾"} Project
        </button>
        {!collapsed && (
          <>
            <input
              className="project-search"
              placeholder="Rechercher…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            {naming && (
              <span className="project-naming">
                Nom de la scène :
                <input
                  ref={nameRef}
                  autoFocus
                  defaultValue="nouvelle-scene"
                  onKeyDown={(e) => {
                    if (e.key === "Enter") submitName();
                    if (e.key === "Escape") setNaming(false);
                  }}
                />
                <button className="button" onClick={submitName}>
                  Créer
                </button>
              </span>
            )}
          </>
        )}
      </div>
      {!collapsed && (
        <div className="project-body">
          <div className="project-tree">
            {SECTIONS.map((s) => (
              <div
                key={s.id}
                className={`project-node ${section === s.id ? "selected" : ""}`}
                onClick={() => setSection(s.id)}
              >
                {s.label}
                <span className="project-count">
                  {s.id ? files.filter((f) => f.section === s.id).length : files.length}
                </span>
              </div>
            ))}
          </div>
          <div
            className="project-grid"
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, file: null });
            }}
          >
            {shown.map((f) => (
              <div
                key={f.path}
                className={`project-tile ${f.path === currentScenePath ? "current" : ""} ${
                  f.exists ? "" : "missing"
                }`}
                title={`${f.path}${f.exists ? ` — ${formatSize(f.size)}` : " — fichier manquant"}`}
                onDoubleClick={() => activate(f)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setMenu({ x: e.clientX, y: e.clientY, file: f });
                }}
              >
                <span className="tile-icon">{ICONS[f.kind]}</span>
                <span className="tile-name">{f.name}</span>
                {!f.exists && <span className="tile-badge danger">manquant</span>}
                {f.exists && !f.registered && (
                  <span className="tile-badge">non importé</span>
                )}
              </div>
            ))}
            {shown.length === 0 && (
              <div className="hint">Aucun fichier — clic droit pour créer.</div>
            )}
          </div>
        </div>
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          actions={actionsFor(menu.file)}
        />
      )}
    </div>
  );
}
