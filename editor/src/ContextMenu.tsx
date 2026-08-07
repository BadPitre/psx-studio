// Menu contextuel partagé (hiérarchie, panneau Project). Une action peut
// porter un sous-menu (« Créer ▸ ») : il s'ouvre au survol, pensé pour
// s'enrichir (scènes, puis prefabs, bases de données, etc.).

import { useEffect, useLayoutEffect, useRef, useState } from "react";

export interface MenuAction {
  label: string;
  shortcut?: string;
  onClick?: () => void;
  danger?: boolean;
  /** Sous-menu : l'action devient un simple déclencheur au survol. */
  children?: MenuAction[];
}

export function ContextMenu({
  x,
  y,
  onClose,
  actions,
}: {
  x: number;
  y: number;
  onClose: () => void;
  actions: MenuAction[];
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

  /* Un menu long (la liste des scripts) ne doit jamais sortir de la
     fenêtre : on le remonte (ou on le colle en haut) après mesure. */
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const margin = 8;
    const maxX = window.innerWidth - width - margin;
    const maxY = window.innerHeight - height - margin;
    setPos({
      x: Math.max(margin, Math.min(x, maxX)),
      y: Math.max(margin, Math.min(y, maxY)),
    });
  }, [x, y, actions]);

  /* Sous-menu ouvert : rendu en position FIXE, mesurée sur l'item —
     un sous-menu absolu serait rogné par le défilement du menu parent
     (et faisait apparaître des barres dans tous les sens). */
  const [sub, setSub] = useState<{ label: string; x: number; y: number } | null>(
    null,
  );
  const openSub = (a: MenuAction, el: HTMLElement) => {
    if (!a.children) return setSub(null);
    const r = el.getBoundingClientRect();
    const width = 200;
    const toLeft = r.right + width > window.innerWidth - 8;
    setSub({
      label: a.label,
      x: toLeft ? Math.max(8, r.left - width) : r.right - 4,
      y: Math.min(r.top - 4, window.innerHeight - 8 - a.children.length * 26),
    });
  };

  return (
    <>
    <div
      ref={ref}
      className="context-menu"
      style={{ left: pos.x, top: pos.y }}
    >
      {actions.map((a) => (
        <div
          key={a.label}
          className={`context-item ${a.danger ? "danger" : ""} ${a.children ? "has-submenu" : ""} ${
            sub?.label === a.label ? "open" : ""
          }`}
          onMouseEnter={(e) => openSub(a, e.currentTarget)}
          onClick={() => {
            if (a.children) return;
            onClose();
            a.onClick?.();
          }}
        >
          <span>{a.label}</span>
          {a.shortcut && <span className="context-shortcut">{a.shortcut}</span>}
          {a.children && <span className="context-shortcut">▸</span>}
        </div>
      ))}
    </div>
    {sub && (
      <div
        className="context-menu context-submenu-fixed"
        style={{ left: sub.x, top: Math.max(8, sub.y) }}
        onMouseLeave={() => setSub(null)}
      >
        {(actions.find((a) => a.label === sub.label)?.children ?? []).map((c) => (
          <div
            key={c.label}
            className={`context-item ${c.danger ? "danger" : ""}`}
            onClick={() => {
              onClose();
              c.onClick?.();
            }}
          >
            <span>{c.label}</span>
          </div>
        ))}
      </div>
    )}
    </>
  );
}
