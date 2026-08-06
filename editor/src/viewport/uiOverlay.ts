// Rendu 2D des canvas UI de la scène (parité avec engine/ui.c et le
// previewer) : résolution des RectTransforms en entier (ancres 4.12),
// aplats, Image Filled tronquée par amount, texte .fnt aligné. Dessiné
// dans un canvas 320x240 superposé au rendu 3D, pointer-events none.

import type { PscScene, PscUiWidget } from "../formats/psc";
import { textWidth } from "../formats/fnt";

const W = 320;
const H = 240;

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Rects écran résolus de chaque widget (index aligné sur scene.ui). */
export function resolveUiRects(scene: PscScene): { rects: Rect[]; visible: boolean[] } {
  const rects: Rect[] = [];
  const visible: boolean[] = [];
  const axis = (
    pStart: number,
    pLen: number,
    aMin: number,
    aMax: number,
    pivot: number,
    pos: number,
    size: number,
  ): [number, number] => {
    const lo = pStart + Math.floor(pLen * aMin);
    const hi = pStart + Math.floor(pLen * aMax);
    if (aMin === aMax) return [lo + pos - Math.floor(size * pivot), size];
    const start = lo + pos;
    return [start, hi - size - start];
  };

  // Curseur d'empilement par Layout Group (parité avec engine/ui.c).
  const cursor: number[] = new Array(scene.ui.length).fill(0);
  scene.ui.forEach((rec, i) => {
    let parent: Rect = { x: 0, y: 0, w: W, h: H };
    let parentVisible = true;
    let pi = -1;
    const parentEntity = scene.entities[rec.entity]?.parent ?? -1;
    if (parentEntity >= 0) {
      pi = scene.ui.findIndex((r) => r.entity === parentEntity);
      if (pi >= 0 && pi < i) {
        parent = rects[pi];
        parentVisible = visible[pi];
      } else pi = -1;
    }
    visible.push(parentVisible && (rec.components & (1 << 5)) !== 0);

    const parentRec = pi >= 0 ? scene.ui[pi] : null;
    if (parentRec && parentRec.components & (1 << 4)) {
      // Parent Layout Group : empilement + alignement, ancres ignorées.
      const [pl, pt, pr, pb] = parentRec.uv;
      const cx = parent.x + pl;
      const cy = parent.y + pt;
      const cw = parent.w - pl - pr;
      const ch = parent.h - pt - pb;
      const horizontal = (parentRec.flags & (1 << 4)) !== 0;
      const w = parentRec.flags & (1 << 5) && !horizontal ? cw : rec.size[0];
      const h = parentRec.flags & (1 << 6) && horizontal ? ch : rec.size[1];
      const align = parentRec.border[0];
      let x: number;
      let y: number;
      if (horizontal) {
        x = cx + cursor[pi];
        y = cy + Math.floor((Math.floor(align / 3) * (ch - h)) / 2);
        cursor[pi] += w + parentRec.extra;
      } else {
        y = cy + cursor[pi];
        x = cx + Math.floor(((align % 3) * (cw - w)) / 2);
        cursor[pi] += h + parentRec.extra;
      }
      rects.push({ x, y, w, h });
      return;
    }

    const [x, w] = axis(parent.x, parent.w, rec.anchorMin[0], rec.anchorMax[0], rec.pivot[0], rec.position[0], rec.size[0]);
    const [y, h] = axis(parent.y, parent.h, rec.anchorMin[1], rec.anchorMax[1], rec.pivot[1], rec.position[1], rec.size[1]);
    rects.push({ x, y, w, h });
  });
  return { rects, visible };
}

/** Dessine les canvas actifs de la scène dans un contexte 320x240. */
export function drawUi(ctx: CanvasRenderingContext2D, scene: PscScene) {
  ctx.clearRect(0, 0, W, H);
  if (!scene.ui.length) return;
  const { rects, visible } = resolveUiRects(scene);

  scene.ui.forEach((rec, i) => {
    const r = rects[i];
    if (!visible[i] || r.w <= 0 || r.h <= 0) return;

    if (rec.components & (1 << 1)) {
      let { w, h } = r;
      if ((rec.flags & 3) === 3) {
        // Image Filled : tronquée par amount (4.12).
        if (rec.flags & (1 << 2)) h = (h * rec.data) >> 12;
        else w = (w * rec.data) >> 12;
      }
      if (w > 0 && h > 0 && rec.asset === 0xff) {
        ctx.fillStyle = `rgb(${rec.color[0]},${rec.color[1]},${rec.color[2]})`;
        ctx.fillRect(r.x, r.y, w, h);
      } else if (w > 0 && h > 0) {
        const tex = scene.textures[rec.asset];
        if (tex) {
          const sw = rec.uv[2] || tex.width;
          const sh = rec.uv[3] || tex.height;
          const type = rec.flags & 3;
          if (type === 1) {
            // Sliced : coins intacts, bords/centre étirés.
            const [bl, bt, br, bb] = rec.border;
            const xs = [r.x, r.x + bl, r.x + w - br, r.x + w];
            const ys = [r.y, r.y + bt, r.y + h - bb, r.y + h];
            const us = [rec.uv[0], rec.uv[0] + bl, rec.uv[0] + sw - br, rec.uv[0] + sw];
            const vs = [rec.uv[1], rec.uv[1] + bt, rec.uv[1] + sh - bb, rec.uv[1] + sh];
            for (let cy = 0; cy < 3; cy++)
              for (let cx = 0; cx < 3; cx++) {
                const pw = xs[cx + 1] - xs[cx];
                const ph = ys[cy + 1] - ys[cy];
                if (pw > 0 && ph > 0)
                  drawRgbaRegion(ctx, tex.rgba, tex.width, us[cx], vs[cy],
                    us[cx + 1] - us[cx], vs[cy + 1] - vs[cy], xs[cx], ys[cy], pw, ph);
              }
          } else if (type === 2) {
            // Tiled : répétition du sprite, bords tronqués.
            for (let ty = 0; ty < h; ty += sh)
              for (let tx = 0; tx < w; tx += sw) {
                const pw = Math.min(sw, w - tx);
                const ph = Math.min(sh, h - ty);
                drawRgbaRegion(ctx, tex.rgba, tex.width, rec.uv[0], rec.uv[1], pw, ph, r.x + tx, r.y + ty, pw, ph);
              }
          } else {
            drawRgbaRegion(ctx, tex.rgba, tex.width, rec.uv[0], rec.uv[1], sw, sh, r.x, r.y, w, h);
          }
        }
      }
    }

    if (rec.components & (1 << 2) && rec.text !== null) {
      const font = scene.fonts[rec.asset];
      if (!font) return;
      let x = r.x;
      if (rec.extra === 1) x = r.x + ((r.w - textWidth(font, rec.text)) >> 1);
      else if (rec.extra === 2) x = r.x + r.w - textWidth(font, rec.text);
      drawText(ctx, font, rec, x, r.y);
    }
  });
}

function drawText(
  ctx: CanvasRenderingContext2D,
  font: NonNullable<PscScene["fonts"][number]>,
  rec: PscUiWidget,
  x: number,
  y: number,
) {
  for (const ch of rec.text!) {
    const g = ch.charCodeAt(0) - font.first;
    if (g < 0 || g >= font.count) {
      x += font.cellW >> 1;
      continue;
    }
    const gu = (g % 16) * font.cellW;
    const gv = Math.floor(g / 16) * font.cellH;
    // Glyphes blancs de l'atlas teintés par la couleur du widget.
    const img = ctx.createImageData(font.cellW, font.cellH);
    for (let py = 0; py < font.cellH; py++) {
      for (let px = 0; px < font.cellW; px++) {
        const src = ((gv + py) * font.tim.width + gu + px) * 4;
        if (font.tim.rgba[src + 3] === 0) continue;
        const dst = (py * font.cellW + px) * 4;
        img.data[dst] = rec.color[0];
        img.data[dst + 1] = rec.color[1];
        img.data[dst + 2] = rec.color[2];
        img.data[dst + 3] = 255;
      }
    }
    // putImageData écrase l'alpha : passer par un canvas intermédiaire
    // pour composer par-dessus l'existant.
    const off = document.createElement("canvas");
    off.width = font.cellW;
    off.height = font.cellH;
    off.getContext("2d")!.putImageData(img, 0, 0);
    ctx.drawImage(off, x, y);
    x += font.advances[g];
  }
}

function drawRgbaRegion(
  ctx: CanvasRenderingContext2D,
  rgba: Uint8ClampedArray,
  texWidth: number,
  sx: number,
  sy: number,
  sw: number,
  sh: number,
  dx: number,
  dy: number,
  dw: number,
  dh: number,
) {
  const off = document.createElement("canvas");
  off.width = sw;
  off.height = sh;
  const img = off.getContext("2d")!.createImageData(sw, sh);
  for (let py = 0; py < sh; py++) {
    for (let px = 0; px < sw; px++) {
      const src = ((sy + py) * texWidth + sx + px) * 4;
      const dst = (py * sw + px) * 4;
      for (let c = 0; c < 4; c++) img.data[dst + c] = rgba[src + c];
    }
  }
  off.getContext("2d")!.putImageData(img, 0, 0);
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(off, dx, dy, dw, dh);
}
