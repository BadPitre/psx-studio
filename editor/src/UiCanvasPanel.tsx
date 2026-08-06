// Mode « Canvas » : édition de l'UI dans sa propre vue, séparée de la
// scène 3D — le canvas 320x240 en grand sur fond damier (comme la vue
// Canvas d'Unity). Clic = sélectionner le widget (le plus haut sous le
// curseur), glisser = le déplacer (position du RectTransform, commit en
// une seule étape d'undo au relâchement).

import { useEffect, useMemo, useRef, useState } from "react";
import type { PscScene } from "./formats/psc";
import { drawUi, resolveUiRects } from "./viewport/uiOverlay";

const W = 320;
const H = 240;

export function UiCanvasPanel({
  scene,
  selected,
  onSelect,
  onMoveWidget,
}: {
  scene: PscScene;
  /** Entité sélectionnée (indice global de la scène). */
  selected: number;
  onSelect: (entityIndex: number) => void;
  /** Déplacement commité d'un widget (delta en pixels écran). */
  onMoveWidget?: (entityIndex: number, dx: number, dy: number) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [drag, setDrag] = useState<{
    ui: number;
    startX: number;
    startY: number;
    dx: number;
    dy: number;
  } | null>(null);

  const { rects, visible } = useMemo(() => resolveUiRects(scene), [scene]);
  const selectedUi = scene.ui.findIndex((r) => r.entity === selected);

  useEffect(() => {
    const ctx = canvasRef.current?.getContext("2d");
    if (!ctx) return;
    // Fond damier neutre (hors console) pour juger la transparence.
    for (let y = 0; y < H; y += 8) {
      for (let x = 0; x < W; x += 8) {
        ctx.fillStyle = ((x + y) / 8) % 2 ? "#23252e" : "#1a1c24";
        ctx.fillRect(x, y, 8, 8);
      }
    }
    const layer = document.createElement("canvas");
    layer.width = W;
    layer.height = H;
    drawUi(layer.getContext("2d")!, scene);
    ctx.drawImage(layer, 0, 0);

    // Contours : tous les widgets en filigrane, la sélection en accent
    // (décalée par le drag en cours — le commit rebuildera la scène).
    scene.ui.forEach((rec, i) => {
      const r = rects[i];
      if (!visible[i] || r.w <= 0 || r.h <= 0) return;
      const off = i === selectedUi && drag ? [drag.dx, drag.dy] : [0, 0];
      ctx.strokeStyle = i === selectedUi ? "#ffcc00" : "rgba(140,150,180,0.35)";
      ctx.lineWidth = 1;
      ctx.strokeRect(r.x + off[0] + 0.5, r.y + off[1] + 0.5, r.w - 1, r.h - 1);
      if (i === selectedUi) {
        // Croix du pivot.
        const rec2 = scene.ui[i];
        const px = r.x + off[0] + r.w * rec2.pivot[0];
        const py = r.y + off[1] + r.h * rec2.pivot[1];
        ctx.strokeStyle = "#7fd0ff";
        ctx.beginPath();
        ctx.moveTo(px - 3, py);
        ctx.lineTo(px + 3, py);
        ctx.moveTo(px, py - 3);
        ctx.lineTo(px, py + 3);
        ctx.stroke();
      }
      void rec;
    });
  }, [scene, rects, visible, selectedUi, drag]);

  const toCanvas = (e: React.PointerEvent) => {
    const rect = (e.target as HTMLElement).getBoundingClientRect();
    return [
      Math.round(((e.clientX - rect.left) / rect.width) * W),
      Math.round(((e.clientY - rect.top) / rect.height) * H),
    ] as const;
  };

  return (
    <div className="ui-canvas-panel">
      <canvas
        ref={canvasRef}
        width={W}
        height={H}
        className="ui-canvas"
        onPointerDown={(e) => {
          const [x, y] = toCanvas(e);
          // Le widget le plus haut (dernier dessiné) sous le curseur.
          for (let i = scene.ui.length - 1; i >= 0; i--) {
            const r = rects[i];
            if (!visible[i] || (scene.ui[i].components & 1) === 1) continue;
            if (x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h) {
              onSelect(scene.ui[i].entity);
              if (onMoveWidget) {
                (e.target as HTMLElement).setPointerCapture(e.pointerId);
                setDrag({ ui: i, startX: x, startY: y, dx: 0, dy: 0 });
              }
              return;
            }
          }
          onSelect(-1);
        }}
        onPointerMove={(e) => {
          if (!drag) return;
          const [x, y] = toCanvas(e);
          setDrag({ ...drag, dx: x - drag.startX, dy: y - drag.startY });
        }}
        onPointerUp={() => {
          if (drag && onMoveWidget && (drag.dx !== 0 || drag.dy !== 0)) {
            onMoveWidget(scene.ui[drag.ui].entity, drag.dx, drag.dy);
          }
          setDrag(null);
        }}
      />
      <div className="vram-legend">
        <span className="muted">
          Canvas 320×240 · clic : sélectionner · glisser : déplacer · les
          champs précis sont dans l'inspecteur (RectTransform)
        </span>
      </div>
    </div>
  );
}
