// Panneau bas façon Unity, deux onglets :
// - Project : arborescence de dossiers libre à gauche (l'utilisateur crée
//   ses dossiers et range comme il veut — drag & drop d'une tuile vers un
//   dossier), grille du dossier sélectionné à droite, recherche. Le
//   contenu croise le disque et project.json — badges « non importé »
//   (présent, pas enregistré) et « manquant » (enregistré, disparu).
// - Console : le journal de l'éditeur (imports, builds, erreurs).
//
// Clic droit : menu « Créer ▸ » (Scène, Dossier — extensible : prefabs,
// bases de données… viendront s'y ajouter).

import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { ProjectFile } from "./bridge";
import { ContextMenu, type MenuAction } from "./ContextMenu";

/** Vignette asynchrone (aperçu rendu) avec repli sur l'icône. */
const Thumb = memo(function Thumb({
  file,
  icon,
  getThumb,
}: {
  file: ProjectFile;
  icon: string;
  getThumb?: (f: ProjectFile) => Promise<string | null>;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setUrl(null);
    getThumb?.(file).then((u) => alive && setUrl(u));
    return () => {
      alive = false;
    };
  }, [file.path, file.size, file.out, getThumb]); // eslint-disable-line react-hooks/exhaustive-deps
  return url ? (
    <img className="tile-thumb" src={url} alt="" draggable={false} />
  ) : (
    <span className="tile-icon">{icon}</span>
  );
});

export interface LogEntry {
  time: string;
  level: "info" | "error";
  text: string;
}

const ROOTS: { id: string; label: string }[] = [
  { id: "scenes", label: "Scènes" },
  { id: "prefabs", label: "Prefabs" },
  { id: "assets", label: "Assets" },
  { id: "audio", label: "Audio" },
];

const ICONS: Record<ProjectFile["kind"], string> = {
  dir: "📁",
  scene: "🎬",
  prefab: "🧩",
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

/** Enfant direct de `dir` ("" = jamais : les racines sont listées à part). */
const isDirectChild = (path: string, dir: string) =>
  path.startsWith(dir + "/") && !path.slice(dir.length + 1).includes("/");

export function ProjectPanel({
  files,
  logs,
  currentScenePath,
  onOpenScene,
  onOpenPrefab,
  onImport,
  onCreateScene,
  onCreateFolder,
  onMove,
  onRefresh,
  onClearLogs,
  getThumb,
  onCreatePrefab,
}: {
  files: ProjectFile[];
  logs: LogEntry[];
  currentScenePath: string;
  /** Ouvre une scène (chemin relatif projet). */
  onOpenScene: (path: string) => void;
  /** Ouvre un prefab en Prefab Mode (édition isolée). */
  onOpenPrefab?: (path: string) => void;
  /** (Ré)importe un asset gltf/glb/png (chemin relatif projet). */
  onImport: (path: string) => void;
  onCreateScene: (name: string) => void;
  onCreateFolder: (parent: string, name: string) => void;
  /** Déplace un fichier vers un dossier (chemins relatifs projet). */
  onMove: (from: string, toDir: string) => void;
  onRefresh: () => void;
  onClearLogs: () => void;
  /** Aperçu rendu d'une tuile (null : icône). */
  getThumb?: (f: ProjectFile) => Promise<string | null>;
  /** Drop d'une entité de la hiérarchie : sauvegarder en prefab. */
  onCreatePrefab?: (entityName: string) => void;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const [tab, setTab] = useState<"project" | "console">("project");
  /* Hauteur redimensionnable (poignée du bord haut), mémorisée. */
  const [height, setHeight] = useState(() =>
    Number(localStorage.getItem("projectPanelHeight")) || 218,
  );
  const [selectedDir, setSelectedDir] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set(["scenes", "assets", "audio"]));
  const [search, setSearch] = useState("");
  const [menu, setMenu] = useState<{ x: number; y: number; file: ProjectFile | null } | null>(
    null,
  );
  const [naming, setNaming] = useState<{ kind: "scene" | "folder"; parent: string } | null>(
    null,
  );
  const [dropTarget, setDropTarget] = useState("");
  const nameRef = useRef<HTMLInputElement>(null);
  const consoleRef = useRef<HTMLDivElement>(null);

  const dirs = useMemo(() => files.filter((f) => f.kind === "dir"), [files]);
  const childDirs = (parent: string) => dirs.filter((d) => isDirectChild(d.path, parent));

  useEffect(() => {
    consoleRef.current?.scrollTo(0, consoleRef.current.scrollHeight);
  }, [logs, tab]);

  const shown = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (query) {
      // La recherche fouille tout le projet, dossiers exclus.
      return files.filter((f) => f.kind !== "dir" && f.name.toLowerCase().includes(query));
    }
    if (!selectedDir) return files.filter((f) => f.kind !== "dir");
    // Contenu direct du dossier : sous-dossiers d'abord, puis fichiers.
    return files.filter((f) => isDirectChild(f.path, selectedDir));
  }, [files, selectedDir, search]);

  const importable = (f: ProjectFile) =>
    f.exists && (f.kind === "model" || f.kind === "texture");

  const activate = (f: ProjectFile) => {
    if (f.kind === "dir") {
      setSelectedDir(f.path);
      setExpanded((s) => new Set(s).add(f.path));
    } else if (f.kind === "scene" && f.exists) onOpenScene(f.path);
    else if (f.kind === "prefab" && f.exists && onOpenPrefab) onOpenPrefab(f.path);
    else if (importable(f) && !f.registered) onImport(f.path);
  };

  const actionsFor = (f: ProjectFile | null): MenuAction[] => {
    // Le dossier visé : la tuile/le nœud cliqué, sinon le dossier courant.
    const parent = f?.kind === "dir" ? f.path : selectedDir || "assets";
    const create: MenuAction = {
      label: "Créer",
      children: [
        { label: "🎬 Scène…", onClick: () => setNaming({ kind: "scene", parent }) },
        { label: "📁 Dossier…", onClick: () => setNaming({ kind: "folder", parent }) },
        // Demain : prefabs, bases de données, etc.
      ],
    };
    if (!f) return [create, { label: "Rafraîchir", onClick: onRefresh }];
    const actions: MenuAction[] = [];
    if (f.kind === "scene" && f.exists)
      actions.push({ label: "Ouvrir", onClick: () => onOpenScene(f.path) });
    if (f.kind === "prefab" && f.exists && onOpenPrefab)
      actions.push({
        label: "🧩 Ouvrir (Prefab Mode)",
        onClick: () => onOpenPrefab(f.path),
      });
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
    const req = naming;
    setNaming(null);
    if (!name || !req) return;
    if (req.kind === "scene") onCreateScene(name);
    else onCreateFolder(req.parent, name);
  };

  const dropProps = (dir: string) => ({
    // stopPropagation : une tuile-dossier est posée SUR la grille, qui est
    // elle-même une cible de drop — sans ça, un même drop déplacerait deux
    // fois le fichier.
    onDragOver: (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDropTarget(dir);
    },
    onDragLeave: () => setDropTarget(""),
    onDrop: (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDropTarget("");
      const entity = e.dataTransfer.getData("text/psx-entity");
      if (entity && onCreatePrefab) {
        onCreatePrefab(entity);
        return;
      }
      const from = e.dataTransfer.getData("text/psx-path");
      if (from && from !== dir && !dir.startsWith(from + "/")) onMove(from, dir);
    },
  });

  const renderDir = (dir: ProjectFile, depth: number) => {
    const children = childDirs(dir.path);
    const open = expanded.has(dir.path);
    return (
      <div key={dir.path}>
        <div
          className={`project-node ${selectedDir === dir.path ? "selected" : ""} ${
            dropTarget === dir.path ? "drop" : ""
          }`}
          style={{ paddingLeft: 12 + depth * 14 }}
          onClick={() => setSelectedDir(dir.path)}
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
            setMenu({ x: e.clientX, y: e.clientY, file: dir });
          }}
          {...dropProps(dir.path)}
        >
          <span>
            {children.length > 0 && (
              <span
                className="project-caret"
                onClick={(e) => {
                  e.stopPropagation();
                  setExpanded((s) => {
                    const next = new Set(s);
                    if (next.has(dir.path)) next.delete(dir.path);
                    else next.add(dir.path);
                    return next;
                  });
                }}
              >
                {open ? "▾" : "▸"}
              </span>
            )}
            📁 {dir.name}
          </span>
        </div>
        {open && children.map((c) => renderDir(c, depth + 1))}
      </div>
    );
  };

  return (
    <div
      className={`project-panel ${collapsed ? "collapsed" : ""}`}
      style={collapsed ? undefined : { height }}
    >
      <div
        className="project-resize"
        title="Glisser pour redimensionner"
        onPointerDown={(e) => {
          e.preventDefault();
          (e.target as HTMLElement).setPointerCapture(e.pointerId);
          setCollapsed(false);
        }}
        onPointerMove={(e) => {
          if (!(e.target as HTMLElement).hasPointerCapture(e.pointerId)) return;
          const h = Math.min(
            Math.max(window.innerHeight - e.clientY, 96),
            Math.round(window.innerHeight * 0.7),
          );
          setHeight(h);
        }}
        onPointerUp={() => localStorage.setItem("projectPanelHeight", String(height))}
      />
      <div className="project-tabs">
        <button className="project-collapse" onClick={() => setCollapsed(!collapsed)}>
          {collapsed ? "▸" : "▾"}
        </button>
        <button
          className={`project-tab ${tab === "project" ? "active" : ""}`}
          onClick={() => {
            setTab("project");
            setCollapsed(false);
          }}
        >
          Project
        </button>
        <button
          className={`project-tab ${tab === "console" ? "active" : ""}`}
          onClick={() => {
            setTab("console");
            setCollapsed(false);
          }}
        >
          Console{logs.some((l) => l.level === "error") ? " ⚠" : ""}
        </button>
        {!collapsed && tab === "project" && (
          <input
            className="project-search"
            placeholder="Rechercher…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        )}
        {!collapsed && tab === "console" && (
          <button className="button" onClick={onClearLogs}>
            Effacer
          </button>
        )}
        {!collapsed && naming && (
          <span className="project-naming">
            {naming.kind === "scene" ? "Nom de la scène :" : `Dossier dans ${naming.parent} :`}
            <input
              ref={nameRef}
              autoFocus
              defaultValue={naming.kind === "scene" ? "nouvelle-scene" : "nouveau-dossier"}
              onKeyDown={(e) => {
                if (e.key === "Enter") submitName();
                if (e.key === "Escape") setNaming(null);
              }}
            />
            <button className="button" onClick={submitName}>
              Créer
            </button>
          </span>
        )}
      </div>
      {!collapsed && tab === "project" && (
        <div className="project-body">
          <div
            className="project-tree"
            onContextMenu={(e) => {
              // Clic droit dans le vide de l'arbre (ou sur « Tout ») :
              // menu Créer ▸ (dossier/scène) sur le dossier courant.
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, file: null });
            }}
          >
            <div
              className={`project-node ${selectedDir === "" ? "selected" : ""}`}
              onClick={() => setSelectedDir("")}
            >
              Tout
              <span className="project-count">
                {files.filter((f) => f.kind !== "dir").length}
              </span>
            </div>
            {ROOTS.map((root) => {
              const node = dirs.find((d) => d.path === root.id) ?? {
                path: root.id,
                name: root.label,
                section: root.id,
                kind: "dir" as const,
                size: 0,
                registered: true,
                exists: true,
                out: null,
              };
              return renderDir({ ...node, name: root.label }, 0);
            })}
          </div>
          <div
            className="project-grid"
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, file: null });
            }}
            {...(selectedDir ? dropProps(selectedDir) : {})}
          >
            {shown.map((f) => (
              <div
                key={f.path}
                className={`project-tile ${f.path === currentScenePath ? "current" : ""} ${
                  f.exists ? "" : "missing"
                } ${dropTarget === f.path ? "drop" : ""}`}
                title={`${f.path}${f.exists ? (f.kind === "dir" ? "" : ` — ${formatSize(f.size)}`) : " — fichier manquant"}`}
                draggable={f.kind !== "dir" && f.exists}
                onDragStart={(e) => {
                  e.dataTransfer.setData("text/psx-path", f.path);
                  // Un modèle converti se glisse aussi vers le viewport ou
                  // la hiérarchie pour être instancié en entité.
                  if (f.kind === "model" && f.registered && f.out) {
                    e.dataTransfer.setData("text/psx-model", f.out);
                  }
                  if (f.kind === "prefab") {
                    e.dataTransfer.setData("text/psx-prefab", f.path);
                  }
                }}
                onDoubleClick={() => activate(f)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setMenu({ x: e.clientX, y: e.clientY, file: f });
                }}
                {...(f.kind === "dir" ? dropProps(f.path) : {})}
              >
                <Thumb file={f} icon={ICONS[f.kind]} getThumb={getThumb} />
                <span className="tile-name">{f.name}</span>
                {!f.exists && <span className="tile-badge danger">manquant</span>}
                {f.exists && !f.registered && f.kind !== "dir" && (
                  <span className="tile-badge">non importé</span>
                )}
              </div>
            ))}
            {shown.length === 0 && (
              <div className="hint">
                {search ? "Aucun résultat." : "Dossier vide — clic droit pour créer, glisse une tuile ici pour ranger."}
              </div>
            )}
          </div>
        </div>
      )}
      {!collapsed && tab === "console" && (
        <div className="project-console" ref={consoleRef}>
          {logs.length === 0 && <div className="hint">Aucun message.</div>}
          {logs.map((l, i) => (
            <div key={i} className={`console-line ${l.level}`}>
              <span className="console-time">{l.time}</span>
              {l.text}
            </div>
          ))}
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
