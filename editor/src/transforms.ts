// Transforms hiérarchiques du scene.json (côté éditeur).
//
// Le format stocke des transforms LOCALES (relatives au parent), comme
// Unity : la position d'un enfant s'exprime dans le repère de son
// parent. Reparenter un objet doit donc recalculer sa transform locale
// pour qu'il ne bouge pas à l'écran — c'est ce que fait `localAfterReparent`.
//
// Conventions du projet : position en unités monde (+Y vers le bas),
// rotation en degrés appliqués dans l'ordre XYZ (RotMatrix compose
// mX * mY * mZ), échelle en facteurs.

import * as THREE from "three";

export type EntityLike = Record<string, unknown>;

const vec3 = (v: unknown, fallback: number): [number, number, number] =>
  Array.isArray(v)
    ? [Number(v[0]) || 0, Number(v[1]) || 0, Number(v[2]) || 0]
    : [fallback, fallback, fallback];

/** Matrice locale d'une entité du JSON (position/rotation°/échelle). */
export function localMatrix(entity: EntityLike): THREE.Matrix4 {
  const pos = vec3(entity.position, 0);
  const rot = vec3(entity.rotation, 0);
  const scl = Array.isArray(entity.scale) ? vec3(entity.scale, 1) : [1, 1, 1];
  const deg = Math.PI / 180;
  return new THREE.Matrix4().compose(
    new THREE.Vector3(pos[0], pos[1], pos[2]),
    new THREE.Quaternion().setFromEuler(
      new THREE.Euler(rot[0] * deg, rot[1] * deg, rot[2] * deg, "XYZ"),
    ),
    new THREE.Vector3(scl[0] || 1e-6, scl[1] || 1e-6, scl[2] || 1e-6),
  );
}

/** Matrice monde : produit des locales de la chaîne d'ancêtres. */
export function worldMatrix(
  entities: EntityLike[],
  name: string | undefined,
): THREE.Matrix4 {
  const out = new THREE.Matrix4();
  if (!name) return out;
  const chain: EntityLike[] = [];
  const seen = new Set<string>();
  let current = entities.find((e) => e.name === name);
  while (current && !seen.has(current.name as string)) {
    seen.add(current.name as string);
    chain.unshift(current);
    const parent = current.parent as string | undefined;
    current = parent ? entities.find((e) => e.name === parent) : undefined;
  }
  for (const e of chain) out.multiply(localMatrix(e));
  return out;
}

/** Arrondis du format : positions entières, degrés au dixième, échelle au millième. */
function quantize(v: number, factor: number) {
  const r = Math.round(v * factor) / factor;
  return Object.is(r, -0) ? 0 : r;
}

/**
 * Transform locale à donner à `name` pour qu'il garde sa position
 * MONDE une fois attaché à `newParent` (undefined = racine) — le
 * comportement d'un reparentage Unity.
 */
export function localAfterReparent(
  entities: EntityLike[],
  name: string,
  newParent: string | undefined,
): { position: [number, number, number]; rotation: [number, number, number]; scale: [number, number, number] } {
  const world = worldMatrix(entities, name);
  const parentWorld = worldMatrix(entities, newParent);
  const local = new THREE.Matrix4()
    .copy(parentWorld)
    .invert()
    .multiply(world);

  const pos = new THREE.Vector3();
  const quat = new THREE.Quaternion();
  const scl = new THREE.Vector3();
  local.decompose(pos, quat, scl);
  const euler = new THREE.Euler().setFromQuaternion(quat, "XYZ");
  const rad = 180 / Math.PI;
  return {
    position: [quantize(pos.x, 1), quantize(pos.y, 1), quantize(pos.z, 1)],
    rotation: [
      quantize(euler.x * rad, 10),
      quantize(euler.y * rad, 10),
      quantize(euler.z * rad, 10),
    ],
    scale: [quantize(scl.x, 1000), quantize(scl.y, 1000), quantize(scl.z, 1000)],
  };
}
