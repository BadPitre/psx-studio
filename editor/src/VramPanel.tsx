// VRAM Viewer : carte 1024×512 mots de la VRAM console — framebuffers,
// police de debug, textures (avec leur contenu réel) et CLUTs. Les
// placements sont lus dans les TIM embarqués du .psc (donc identiques à
// ce que le packer a décidé).

import { useEffect, useRef } from "react";
import type { PscScene } from "./formats/psc";

const OUTLINES = ["#ff9f43", "#54d68a", "#c58aff", "#5ac8e8", "#ffd32a", "#ff7f7f"];

function formatKb(bytes: number): string {
  return bytes >= 1024 ? `${(bytes / 1024).toFixed(1)} Ko` : `${bytes} o`;
}

// Statistiques d'occupation : la VRAM fait 1024×512 mots de 16 bits (1 Mo).
// Le "trou libre" est estimé en pages de texture entières (64×256 mots),
// la granularité réelle à laquelle le packer place les textures.
function computeStats(scene: PscScene) {
  const TOTAL_WORDS = 1024 * 512;
  // Zones réservées : double framebuffer 320×240 + police de debug.
  const reserved = 2 * 320 * 240 + 64 * 64;
  let texWords = 0;
  for (const tex of scene.textures) {
    texWords += tex.wordsPerRow * tex.height + tex.clutEntries;
  }

  // Grille de pages 16×2 : une page est "libre" si rien ne la touche.
  const pageUsed: boolean[][] = Array.from({ length: 2 }, () => Array(16).fill(false));
  const mark = (x: number, y: number, w: number, h: number) => {
    for (let py = Math.floor(y / 256); py <= Math.floor((y + h - 1) / 256); py++) {
      for (let px = Math.floor(x / 64); px <= Math.floor((x + w - 1) / 64); px++) {
        if (py >= 0 && py < 2 && px >= 0 && px < 16) pageUsed[py][px] = true;
      }
    }
  };
  mark(0, 0, 320, 480);
  mark(960, 0, 64, 64);
  for (const tex of scene.textures) {
    mark(tex.vramX, tex.vramY, tex.wordsPerRow, tex.height);
    if (tex.clutEntries > 0) mark(tex.clutX, tex.clutY, tex.clutEntries, 1);
  }

  let freePages = 0;
  let largestRun = 0;
  for (let py = 0; py < 2; py++) {
    let run = 0;
    for (let px = 0; px < 16; px++) {
      if (pageUsed[py][px]) {
        run = 0;
      } else {
        freePages++;
        run++;
        if (run > largestRun) largestRun = run;
      }
    }
  }

  const usedWords = reserved + texWords;
  return {
    usedPercent: (usedWords / TOTAL_WORDS) * 100,
    texBytes: texWords * 2,
    freePages,
    largestRun,
  };
}

export function VramPanel({ scene }: { scene: PscScene }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const ctx = canvasRef.current?.getContext("2d");
    if (!ctx) return;

    ctx.fillStyle = "#101116";
    ctx.fillRect(0, 0, 1024, 512);

    // Grille des pages (64 mots × 256 lignes).
    ctx.strokeStyle = "#23252e";
    for (let x = 0; x <= 1024; x += 64) {
      ctx.beginPath();
      ctx.moveTo(x + 0.5, 0);
      ctx.lineTo(x + 0.5, 512);
      ctx.stroke();
    }
    ctx.beginPath();
    ctx.moveTo(0, 256.5);
    ctx.lineTo(1024, 256.5);
    ctx.stroke();

    // Framebuffers double-buffer 320×240 et police de debug.
    ctx.fillStyle = "#28406e";
    ctx.fillRect(0, 0, 320, 240);
    ctx.fillRect(0, 240, 320, 240);
    ctx.fillStyle = "#9fb4d8";
    ctx.font = "14px system-ui";
    ctx.fillText("framebuffer 0", 12, 24);
    ctx.fillText("framebuffer 1", 12, 264);
    ctx.fillStyle = "#4a4a4a";
    ctx.fillRect(960, 0, 64, 64);
    ctx.fillStyle = "#a0a0a0";
    ctx.fillText("font", 968, 38);

    // Textures : contenu réel dessiné à sa place (largeur en mots).
    scene.textures.forEach((tex, i) => {
      const image = new ImageData(
        new Uint8ClampedArray(tex.rgba),
        tex.width,
        tex.height,
      );
      // La VRAM est adressée en mots 16 bits : on compresse horizontalement
      // le contenu texel vers sa largeur réelle en mots.
      const off = document.createElement("canvas");
      off.width = tex.width;
      off.height = tex.height;
      off.getContext("2d")!.putImageData(image, 0, 0);
      ctx.drawImage(off, tex.vramX, tex.vramY, tex.wordsPerRow, tex.height);

      ctx.strokeStyle = OUTLINES[i % OUTLINES.length];
      ctx.lineWidth = 2;
      ctx.strokeRect(tex.vramX + 1, tex.vramY + 1, tex.wordsPerRow - 2, tex.height - 2);

      if (tex.clutEntries > 0) {
        ctx.fillStyle = OUTLINES[i % OUTLINES.length];
        ctx.fillRect(tex.clutX, tex.clutY - 1, tex.clutEntries, 3);
      }
    });
  }, [scene]);

  const stats = computeStats(scene);

  return (
    <div className="vram-panel">
      <canvas ref={canvasRef} width={1024} height={512} className="vram-canvas" />
      <div className="vram-stats">
        <span>
          occupation <strong>{stats.usedPercent.toFixed(1)} %</strong>
        </span>
        <span>
          textures <strong>{formatKb(stats.texBytes)}</strong>
        </span>
        <span>
          pages libres <strong>{stats.freePages}/32</strong>
        </span>
        <span>
          plus grand bloc libre <strong>{stats.largestRun} page{stats.largestRun > 1 ? "s" : ""}</strong>
        </span>
      </div>
      <div className="vram-legend">
        <span>
          <i className="swatch" style={{ background: "#28406e" }} /> framebuffers (0–320)
        </span>
        {scene.textures.map((tex, i) => (
          <span key={i}>
            <i className="swatch" style={{ background: OUTLINES[i % OUTLINES.length] }} />
            texture {i} — {tex.width}×{tex.height} {tex.bpp}bpp,{" "}
            {formatKb((tex.wordsPerRow * tex.height + tex.clutEntries) * 2)} à ({tex.vramX},{" "}
            {tex.vramY})
            {tex.clutEntries > 0 && `, CLUT (${tex.clutX}, ${tex.clutY})`}
          </span>
        ))}
        <span className="muted">
          1024×512 mots de 16 bits · grille = pages de texture (64 mots)
        </span>
      </div>
    </div>
  );
}
