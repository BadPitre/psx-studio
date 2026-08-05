// Parser du format PMD v1 (spec : docs/PMD-FORMAT.md).
// Miroir TypeScript du writer Rust (pipeline/psxpipe/src/pmd.rs).

export interface PmdPrim {
  vidx: [number, number, number];
  nidx: [number, number, number];
  /** Couleur de base du paquet (0-255, 128 = neutre pour les texturés). */
  color: [number, number, number];
  /** UV en texels (0-255) ou null pour les primitives non texturées. */
  uv: [[number, number], [number, number], [number, number]] | null;
}

export interface PmdModel {
  scale412: number;
  textured: boolean;
  /** Positions modèle, i16 (x, y, z par sommet). */
  verts: Int16Array;
  /** Normales unitaires en flottant (déjà divisées par 4096). */
  normals: Float32Array;
  prims: PmdPrim[];
}

const KINDS = [
  { recSize: 28, gouraud: false, textured: false }, // F3
  { recSize: 40, gouraud: true, textured: false }, // G3
  { recSize: 40, gouraud: false, textured: true }, // FT3
  { recSize: 52, gouraud: true, textured: true }, // GT3
];

export function parsePmd(data: DataView): PmdModel {
  const magic =
    String.fromCharCode(data.getUint8(0), data.getUint8(1), data.getUint8(2)) +
    data.getUint8(3);
  if (data.getUint8(0) !== 0x50 || data.getUint8(1) !== 0x4d || data.getUint8(2) !== 0x44) {
    throw new Error(`PMD: mauvais magic (${magic})`);
  }
  const version = data.getUint16(4, true);
  if (version !== 1) throw new Error(`PMD: version ${version} non supportée`);

  const flags = data.getUint16(6, true);
  const scale412 = data.getInt32(8, true);
  const vertexCount = data.getUint16(12, true);
  const normalCount = data.getUint16(14, true);
  const primCounts = [16, 18, 20, 22].map((o) => data.getUint16(o, true));
  const vertsOffset = data.getUint32(24, true);
  const normalsOffset = data.getUint32(28, true);
  const primOffsets = [32, 36, 40, 44].map((o) => data.getUint32(o, true));

  const verts = new Int16Array(vertexCount * 3);
  for (let i = 0; i < vertexCount; i++) {
    const o = vertsOffset + i * 8;
    verts[i * 3] = data.getInt16(o, true);
    verts[i * 3 + 1] = data.getInt16(o + 2, true);
    verts[i * 3 + 2] = data.getInt16(o + 4, true);
  }
  const normals = new Float32Array(normalCount * 3);
  for (let i = 0; i < normalCount; i++) {
    const o = normalsOffset + i * 8;
    normals[i * 3] = data.getInt16(o, true) / 4096;
    normals[i * 3 + 1] = data.getInt16(o + 2, true) / 4096;
    normals[i * 3 + 2] = data.getInt16(o + 4, true) / 4096;
  }

  const prims: PmdPrim[] = [];
  for (let k = 0; k < 4; k++) {
    const { recSize, gouraud, textured } = KINDS[k];
    const idxSize = gouraud ? 12 : 8;
    for (let p = 0; p < primCounts[k]; p++) {
      const rec = primOffsets[k] + p * recSize;
      const vidx: [number, number, number] = [
        data.getUint16(rec, true),
        data.getUint16(rec + 2, true),
        data.getUint16(rec + 4, true),
      ];
      const n0 = data.getUint16(rec + 6, true);
      const nidx: [number, number, number] = gouraud
        ? [n0, data.getUint16(rec + 8, true), data.getUint16(rec + 10, true)]
        : [n0, n0, n0];
      const pkt = rec + idxSize;
      const color: [number, number, number] = [
        data.getUint8(pkt + 4),
        data.getUint8(pkt + 5),
        data.getUint8(pkt + 6),
      ];
      let uv: PmdPrim["uv"] = null;
      if (textured) {
        // GT3 : uv aux offsets paquet 12/24/36 — FT3 : 12/20/28.
        const offs = gouraud ? [12, 24, 36] : [12, 20, 28];
        uv = offs.map((o) => [data.getUint8(pkt + o), data.getUint8(pkt + o + 1)]) as [
          [number, number],
          [number, number],
          [number, number],
        ];
      }
      prims.push({ vidx, nidx, color, uv });
    }
  }

  return {
    scale412,
    textured: (flags & 1) !== 0,
    verts,
    normals,
    prims,
  };
}
