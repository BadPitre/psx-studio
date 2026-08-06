// Parser des polices bitmap .fnt (miroir de pipeline/psxpipe/src/fnt.rs) :
// "FNT1" | cell_w | cell_h | first | count | chasses[count] | pad(4) | TIM.

import { parseTim, type TimTexture } from "./tim";

export interface FntFont {
  cellW: number;
  cellH: number;
  first: number;
  count: number;
  /** Chasse (avance) par glyphe. */
  advances: Uint8Array;
  /** Atlas décodé (16 glyphes par ligne). */
  tim: TimTexture;
}

export function parseFnt(data: DataView): FntFont {
  if (
    data.getUint8(0) !== 0x46 || data.getUint8(1) !== 0x4e ||
    data.getUint8(2) !== 0x54 || data.getUint8(3) !== 0x31
  ) {
    throw new Error("FNT: mauvais magic");
  }
  const count = data.getUint8(7);
  const timOffset = (8 + count + 3) & ~3;
  return {
    cellW: data.getUint8(4),
    cellH: data.getUint8(5),
    first: data.getUint8(6),
    count,
    advances: new Uint8Array(data.buffer, data.byteOffset + 8, count),
    tim: parseTim(new DataView(data.buffer, data.byteOffset + timOffset)),
  };
}

/** Largeur d'un texte dans une police (somme des chasses). */
export function textWidth(font: FntFont, text: string): number {
  let w = 0;
  for (const ch of text) {
    const g = ch.charCodeAt(0) - font.first;
    w += g >= 0 && g < font.count ? font.advances[g] : font.cellW >> 1;
  }
  return w;
}
