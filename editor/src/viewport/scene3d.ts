// Construction de la scène Three.js depuis une PscScene parsée :
// géométrie non indexée par modèle (UV et couleurs par face), textures
// TIM en DataTexture (NearestFilter), hiérarchie de groupes par entité.

import * as THREE from "three";
import type { PmdModel } from "../formats/pmd";
import type { PscLight, PscScene } from "../formats/psc";
import { makePs1Material, type Ps1Lighting } from "./ps1material";

export interface SceneGraph {
  /** Groupe racine (monde PS1 : scale.y = -1 pour passer en Y-haut). */
  root: THREE.Group;
  /** Groupe par entité, index aligné sur scene.entities. */
  entityGroups: THREE.Group[];
  materials: THREE.ShaderMaterial[];
  lighting: Ps1Lighting;
  /** Entités-lumières (v1.2) : direction vivante via leur groupe. */
  lights: PscLight[];
  /** Flags par entité (composants lumière/caméra). */
  entityFlags: number[];
}

function timToTexture(scene: PscScene, index: number): THREE.DataTexture | null {
  if (index < 0) return null;
  const tim = scene.textures[index];
  const tex = new THREE.DataTexture(
    tim.rgba,
    tim.width,
    tim.height,
    THREE.RGBAFormat,
  );
  tex.magFilter = THREE.NearestFilter;
  tex.minFilter = THREE.NearestFilter;
  tex.flipY = false;
  tex.needsUpdate = true;
  return tex;
}

function modelToGeometry(model: PmdModel, texWidth: number, texHeight: number) {
  const triCount = model.prims.length;
  const positions = new Float32Array(triCount * 9);
  const normals = new Float32Array(triCount * 9);
  const uvs = new Float32Array(triCount * 6);
  const baseColors = new Float32Array(triCount * 9);

  model.prims.forEach((prim, t) => {
    for (let k = 0; k < 3; k++) {
      const v = prim.vidx[k] * 3;
      const n = prim.nidx[k] * 3;
      const o = t * 9 + k * 3;
      positions[o] = model.verts[v];
      positions[o + 1] = model.verts[v + 1];
      positions[o + 2] = model.verts[v + 2];
      normals[o] = model.normals[n];
      normals[o + 1] = model.normals[n + 1];
      normals[o + 2] = model.normals[n + 2];
      baseColors[o] = prim.color[0];
      baseColors[o + 1] = prim.color[1];
      baseColors[o + 2] = prim.color[2];
      if (prim.uv) {
        // Texels -> UV normalisées, centre du texel.
        uvs[t * 6 + k * 2] = (prim.uv[k][0] + 0.5) / texWidth;
        uvs[t * 6 + k * 2 + 1] = (prim.uv[k][1] + 0.5) / texHeight;
      }
    }
  });

  const geo = new THREE.BufferGeometry();
  geo.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geo.setAttribute("normal", new THREE.BufferAttribute(normals, 3));
  geo.setAttribute("uv", new THREE.BufferAttribute(uvs, 2));
  geo.setAttribute("baseColor", new THREE.BufferAttribute(baseColors, 3));
  return geo;
}

/** Applique la transform PS1 d'une entité à son groupe three. */
export function applyEntityTransform(
  group: THREE.Group,
  pos: [number, number, number],
  rotUnits: [number, number, number],
  scale: [number, number, number],
) {
  group.position.set(pos[0], pos[1], pos[2]);
  const toRad = (u: number) => (u / 4096) * Math.PI * 2;
  // RotMatrix compose mX * mY * mZ -> ordre d'Euler "XYZ".
  group.rotation.set(toRad(rotUnits[0]), toRad(rotUnits[1]), toRad(rotUnits[2]), "XYZ");
  group.scale.set(scale[0] || 1e-6, scale[1] || 1e-6, scale[2] || 1e-6);
}

export function buildSceneGraph(scene: PscScene): SceneGraph {
  const lighting: Ps1Lighting = {
    toward: scene.lightToward,
    color: scene.lightColor,
    ambient: scene.ambient,
  };

  // Un matériau par texture (+ un pour les non-texturés).
  const materials: THREE.ShaderMaterial[] = [];
  const materialForTexture = new Map<number, THREE.ShaderMaterial>();
  const getMaterial = (texIndex: number) => {
    let mat = materialForTexture.get(texIndex);
    if (!mat) {
      mat = makePs1Material(timToTexture(scene, texIndex), lighting);
      materialForTexture.set(texIndex, mat);
      materials.push(mat);
    }
    return mat;
  };

  const geometries = scene.models.map((model, i) => {
    const texIndex = scene.modelTexture[i];
    const tim = texIndex >= 0 ? scene.textures[texIndex] : null;
    return modelToGeometry(model, tim?.width ?? 256, tim?.height ?? 256);
  });

  // Monde PS1 (+Y bas) rendu dans three (+Y haut) : miroir Y global. Le
  // miroir retourne le winding horaire PS1 en CCW, ce que three attend.
  const root = new THREE.Group();
  root.scale.set(1, -1, 1);

  const entityGroups: THREE.Group[] = [];
  scene.entities.forEach((entity, i) => {
    const group = new THREE.Group();
    group.name = `entity-${i}`;
    applyEntityTransform(group, entity.pos, entity.rot, entity.scale);
    if (entity.model >= 0) {
      const mesh = new THREE.Mesh(
        geometries[entity.model],
        getMaterial(scene.modelTexture[entity.model]),
      );
      mesh.userData.entityIndex = i;
      group.add(mesh);
    }
    const parent = entity.parent >= 0 ? entityGroups[entity.parent] : root;
    parent.add(group);
    entityGroups.push(group);
  });

  return {
    root,
    entityGroups,
    materials,
    lighting,
    lights: scene.lights,
    entityFlags: scene.entities.map((e) => e.flags),
  };
}
