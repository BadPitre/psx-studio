// Parser du SceneFormat v1 (.psc) — spec : docs/SCENE-FORMAT.md.
// Miroir TypeScript du writer Rust (pipeline/psxpipe/src/scene.rs).

import { parsePmd, type PmdModel } from "./pmd";
import { parseTim, type TimTexture } from "./tim";

export const NO_INDEX = 0xffff;

/** Flags d'entité (v1.2). */
export const ENTITY_FLAG_LIGHT = 1 << 0;
export const ENTITY_FLAG_CAMERA = 1 << 1;
/** Modificateur : lumière ponctuelle (torche) au lieu de directionnelle. */
export const ENTITY_FLAG_LIGHT_POINT = 1 << 2;

export interface PscEntity {
  /** Position locale (unités monde GTE, +Y vers le bas). */
  pos: [number, number, number];
  /** Rotation locale en unités PS1 (4096 = 360°). */
  rot: [number, number, number];
  /** Échelle locale en flottant (1.0 = neutre). */
  scale: [number, number, number];
  /** Indice du parent (toujours < indice courant) ou -1. */
  parent: number;
  /** Indice de modèle ou -1 (nœud vide). */
  model: number;
  /** Composants : ENTITY_FLAG_LIGHT / ENTITY_FLAG_CAMERA. */
  flags: number;
  /** FOV vertical caméra en degrés (0 = défaut PS1, ~74°). */
  camFov: number;
  /** Distance d'affichage caméra en unités monde (0 = illimitée). */
  camDraw: number;
  /** Rayon de lumière ponctuelle en unités monde (0 = directionnelle). */
  lightRadius: number;
}

/** FOV vertical de la projection PS1 native (h = 160) : 2·atan(120/160). */
export const PS1_DEFAULT_FOV = (2 * Math.atan(120 / 160) * 180) / Math.PI;

/** Lumière directionnelle portée par une entité (v1.2). */
export interface PscLight {
  /** Indice d'entité : sa rotation donne la direction (éclaire vers -Z). */
  entity: number;
  color: [number, number, number];
  /** Multiplicateur d'intensité (1.0 = 100 %). */
  intensity: number;
}

export interface PscScene {
  background: [number, number, number];
  ambient: [number, number, number];
  lightColor: [number, number, number];
  /** Vecteur unitaire VERS la source lumineuse (espace monde PS1). */
  lightToward: [number, number, number];
  models: PmdModel[];
  /** Indice de texture par modèle, ou -1. */
  modelTexture: number[];
  textures: TimTexture[];
  entities: PscEntity[];
  /** Lumières additionnelles (v1.2, 2 max — le GTE en offre 3 avec le soleil). */
  lights: PscLight[];
}

export function parsePsc(buffer: ArrayBuffer): PscScene {
  const data = new DataView(buffer);
  if (
    data.getUint8(0) !== 0x50 || // P
    data.getUint8(1) !== 0x53 || // S
    data.getUint8(2) !== 0x43 || // C
    data.getUint8(3) !== 0x31 // 1
  ) {
    throw new Error("PSC: mauvais magic (pas un fichier scène)");
  }
  const version = data.getUint16(4, true);
  if (version !== 1) throw new Error(`PSC: version ${version} non supportée`);

  const modelCount = data.getUint16(12, true);
  const textureCount = data.getUint16(14, true);
  const entityCount = data.getUint16(16, true);
  const modelsOffset = data.getUint32(20, true);
  const texturesOffset = data.getUint32(24, true);
  const entitiesOffset = data.getUint32(28, true);

  const rgb = (o: number): [number, number, number] => [
    data.getUint8(o),
    data.getUint8(o + 1),
    data.getUint8(o + 2),
  ];

  const models: PmdModel[] = [];
  const modelTexture: number[] = [];
  for (let i = 0; i < modelCount; i++) {
    const rec = modelsOffset + i * 12;
    const off = data.getUint32(rec, true);
    const size = data.getUint32(rec + 4, true);
    const tex = data.getUint16(rec + 8, true);
    models.push(parsePmd(new DataView(buffer, off, size)));
    modelTexture.push(tex === NO_INDEX ? -1 : tex);
  }

  const textures: TimTexture[] = [];
  for (let i = 0; i < textureCount; i++) {
    const rec = texturesOffset + i * 8;
    const off = data.getUint32(rec, true);
    const size = data.getUint32(rec + 4, true);
    textures.push(parseTim(new DataView(buffer, off, size)));
  }

  const entities: PscEntity[] = [];
  for (let i = 0; i < entityCount; i++) {
    const rec = entitiesOffset + i * 32;
    const i16 = (o: number) => data.getInt16(rec + o, true);
    const parent = data.getUint16(rec + 0x18, true);
    const model = data.getUint16(rec + 0x1a, true);
    entities.push({
      pos: [i16(0), i16(2), i16(4)],
      rot: [i16(8), i16(10), i16(12)],
      scale: [i16(16) / 4096, i16(18) / 4096, i16(20) / 4096],
      parent: parent === NO_INDEX ? -1 : parent,
      model: model === NO_INDEX ? -1 : model,
      flags: data.getUint16(rec + 0x1c, true),
      camFov: data.getUint16(rec + 6, true),
      camDraw: data.getUint16(rec + 0x0e, true),
      lightRadius: data.getUint16(rec + 0x16, true),
    });
  }

  // Table des lumières (v1.2 ; offset 0 sur les anciens fichiers).
  const lights: PscLight[] = [];
  const lightsOffset = data.getUint32(56, true);
  const lightCount = data.getUint16(60, true);
  if (lightsOffset > 0) {
    for (let i = 0; i < lightCount; i++) {
      const rec = lightsOffset + i * 6;
      lights.push({
        entity: data.getUint16(rec, true),
        color: rgb(rec + 2),
        intensity: (data.getUint8(rec + 5) || 100) / 100,
      });
    }
  }

  return {
    background: rgb(32),
    ambient: rgb(36),
    lightColor: rgb(40),
    lightToward: [
      data.getInt16(44, true) / 4096,
      data.getInt16(46, true) / 4096,
      data.getInt16(48, true) / 4096,
    ],
    models,
    modelTexture,
    textures,
    entities,
    lights,
  };
}

/** Nombre total de triangles d'une scène (somme des modèles instanciés). */
export function sceneTriangleCount(scene: PscScene): number {
  let total = 0;
  for (const e of scene.entities) {
    if (e.model >= 0) total += scene.models[e.model].prims.length;
  }
  return total;
}
