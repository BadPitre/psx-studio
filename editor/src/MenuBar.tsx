// Barre de menus déroulants (Fichier / Scène / Aide), comme un éditeur
// desktop classique : les actions du projet vivent ici, la barre d'outils
// en dessous ne garde que ce qui sert en permanence.

import { useEffect, useRef, useState } from "react";
import { ContextMenu, type MenuAction } from "./ContextMenu";

export interface Menu {
  label: string;
  actions: MenuAction[];
}

export function MenuBar({ menus }: { menus: Menu[] }) {
  const [open, setOpen] = useState<{ index: number; x: number; y: number } | null>(
    null,
  );
  const barRef = useRef<HTMLDivElement>(null);

  // Fermer quand le projet change de forme (menus recréés).
  useEffect(() => setOpen(null), [menus.length]);

  const openAt = (index: number, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    setOpen({ index, x: r.left, y: r.bottom });
  };

  return (
    <div className="menu-bar" ref={barRef}>
      {menus.map((m, i) => (
        <button
          key={m.label}
          className={`menu-bar-item ${open?.index === i ? "active" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            if (open?.index === i) setOpen(null);
            else openAt(i, e.currentTarget);
          }}
          // Survol après ouverture : bascule de menu, comme partout.
          onMouseEnter={(e) => open && open.index !== i && openAt(i, e.currentTarget)}
        >
          {m.label}
        </button>
      ))}
      {open && (
        <ContextMenu
          x={open.x}
          y={open.y}
          onClose={() => setOpen(null)}
          actions={menus[open.index].actions}
        />
      )}
    </div>
  );
}
