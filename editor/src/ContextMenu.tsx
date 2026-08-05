// Menu contextuel partagé (hiérarchie, panneau Project). Une action peut
// porter un sous-menu (« Créer ▸ ») : il s'ouvre au survol, pensé pour
// s'enrichir (scènes, puis prefabs, bases de données, etc.).

import { useEffect } from "react";

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

  return (
    <div className="context-menu" style={{ left: x, top: y }}>
      {actions.map((a) => (
        <div
          key={a.label}
          className={`context-item ${a.danger ? "danger" : ""} ${a.children ? "has-submenu" : ""}`}
          onClick={() => {
            if (a.children) return;
            onClose();
            a.onClick?.();
          }}
        >
          <span>{a.label}</span>
          {a.shortcut && <span className="context-shortcut">{a.shortcut}</span>}
          {a.children && <span className="context-shortcut">▸</span>}
          {a.children && (
            <div className="context-submenu">
              {a.children.map((c) => (
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
        </div>
      ))}
    </div>
  );
}
