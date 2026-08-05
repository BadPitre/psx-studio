// Parser du format TIM (texture PS1) → image RGBA décodée.
// Miroir TypeScript du writer Rust (pipeline/psxpipe/src/tim.rs).

export interface TimTexture {
  /** Largeur en texels (déjà multipliée selon le bpp). */
  width: number;
  height: number;
  bpp: 4 | 8 | 16;
  /** RGBA 8 bits par canal ; le texel 0x0000 devient alpha 0. */
  rgba: Uint8ClampedArray<ArrayBuffer>;
  /** Placement VRAM des pixels (x en mots 16 bits). */
  vramX: number;
  vramY: number;
  wordsPerRow: number;
  /** Placement de la CLUT (0 entrées = pas de CLUT). */
  clutX: number;
  clutY: number;
  clutEntries: number;
}

function ps1ToRgba(c: number, out: Uint8ClampedArray, o: number) {
  if (c === 0) {
    out[o] = out[o + 1] = out[o + 2] = out[o + 3] = 0;
    return;
  }
  out[o] = ((c & 31) * 255) / 31;
  out[o + 1] = (((c >> 5) & 31) * 255) / 31;
  out[o + 2] = (((c >> 10) & 31) * 255) / 31;
  out[o + 3] = 255;
}

export function parseTim(data: DataView): TimTexture {
  if (data.getUint32(0, true) !== 0x10) throw new Error("TIM: mauvais magic");
  const flags = data.getUint32(4, true);
  const bpp = ([4, 8, 16] as const)[flags & 3];
  if (bpp === undefined) throw new Error("TIM: mode non supporté");

  let off = 8;
  const clut: number[] = [];
  let clutX = 0;
  let clutY = 0;
  if (flags & 8) {
    const len = data.getUint32(off, true);
    clutX = data.getUint16(off + 4, true);
    clutY = data.getUint16(off + 6, true);
    const count = (len - 12) / 2;
    for (let i = 0; i < count; i++) clut.push(data.getUint16(off + 12 + i * 2, true));
    off += len;
  }
  const vramX = data.getUint16(off + 4, true);
  const vramY = data.getUint16(off + 6, true);
  const wordsPerRow = data.getUint16(off + 8, true);
  const height = data.getUint16(off + 10, true);
  const pixelBase = off + 12;
  const texelsPerWord = bpp === 4 ? 4 : bpp === 8 ? 2 : 1;
  const width = wordsPerRow * texelsPerWord;

  const rgba = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let wx = 0; wx < wordsPerRow; wx++) {
      const word = data.getUint16(pixelBase + (y * wordsPerRow + wx) * 2, true);
      for (let k = 0; k < texelsPerWord; k++) {
        const x = wx * texelsPerWord + k;
        let c: number;
        if (bpp === 16) c = word;
        else if (bpp === 8) c = clut[(word >> (k * 8)) & 0xff] ?? 0;
        else c = clut[(word >> (k * 4)) & 0xf] ?? 0;
        ps1ToRgba(c, rgba, (y * width + x) * 4);
      }
    }
  }
  return {
    width,
    height,
    bpp,
    rgba,
    vramX,
    vramY,
    wordsPerRow,
    clutX,
    clutY,
    clutEntries: clut.length,
  };
}
