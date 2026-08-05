// Les parsers TS sont validés contre les VRAIES scènes produites par le
// pipeline Rust (runtime/player/assets/*.psc) — un désaccord de format
// entre l'éditeur et psxpipe casse ces tests.

import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { parsePsc, sceneTriangleCount } from "../src/formats/psc";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

function loadScene(name: string) {
  const bytes = readFileSync(join(repo, "runtime", "player", "assets", name));
  return parsePsc(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
}

describe("parsePsc sur scene0.psc (village)", () => {
  const scene = loadScene("scene0.psc");

  it("lit l'en-tête et les réglages de scène", () => {
    expect(scene.models.length).toBe(4);
    expect(scene.textures.length).toBe(3);
    expect(scene.entities.length).toBe(10);
    expect(scene.background).toEqual([24, 32, 56]);
    // Le vecteur lumière pointe vers la source (Y négatif = vers le haut).
    expect(scene.lightToward[1]).toBeLessThan(-0.4);
  });

  it("lit les composants v1.2 (lumière + caméra)", () => {
    // La lune : entité-lumière bleutée ; la caméra est flaguée.
    expect(scene.lights.length).toBe(2);
    expect(scene.lights[0].color).toEqual([70, 90, 160]);
    const light = scene.entities[scene.lights[0].entity];
    expect(light.flags & 1).toBe(1);
    expect(scene.entities.some((e) => e.flags & 2)).toBe(true);
    // Les entités classiques n'ont aucun flag.
    expect(scene.entities[0].flags).toBe(0);
  });

  it("décode les modèles PMD embarqués", () => {
    const ground = scene.models.find((m) => m.prims.length === 288);
    const house = scene.models.find((m) => m.prims.length === 16);
    const cube = scene.models.find((m) => m.prims.length === 12);
    expect(ground).toBeDefined();
    expect(house).toBeDefined();
    expect(cube).toBeDefined();
    // Cube : 8 sommets ±128, tous texturés.
    expect(cube!.verts.length).toBe(8 * 3);
    expect(Math.max(...cube!.verts)).toBe(128);
    expect(cube!.textured).toBe(true);
    expect(cube!.prims[0].uv).not.toBeNull();
  });

  it("décode les textures TIM (8bpp + CLUT)", () => {
    // checker + house en 256x256, l'atlas du perso (guy) en 64x64.
    const sizes = scene.textures.map((t) => t.width).sort((a, b) => a - b);
    expect(sizes).toEqual([64, 256, 256]);
    for (const tex of scene.textures) {
      expect(tex.bpp).toBe(8);
      expect(tex.height).toBe(tex.width);
      expect(tex.rgba.length).toBe(tex.width * tex.height * 4);
    }
    // Le damier n'a aucun texel transparent.
    const checker = scene.textures[0];
    let opaque = 0;
    for (let i = 3; i < checker.rgba.length; i += 4) {
      if (checker.rgba[i] === 255) opaque++;
    }
    expect(opaque).toBe(256 * 256);
  });

  it("lit les entités avec hiérarchie et échelle", () => {
    // Invariant du format : parent toujours avant l'enfant.
    scene.entities.forEach((e, i) => {
      if (e.parent >= 0) expect(e.parent).toBeLessThan(i);
    });
    // La cheminée est le seul enfant (parenté à une maison).
    const children = scene.entities.filter((e) => e.parent >= 0);
    expect(children.length).toBe(1);
    // Le sol a une échelle 4 en X/Z.
    const sol = scene.entities[0];
    expect(sol.scale[0]).toBeCloseTo(4.0, 2);
    expect(sol.scale[2]).toBeCloseTo(4.0, 2);
  });

  it("compte les triangles comme le runtime", () => {
    // sol(288) + 3 maisons(16) + cheminée(12) + perso, pnj (48) + torche(12) = 456.
    expect(sceneTriangleCount(scene)).toBe(456);
  });
});

describe("parsePsc sur scene1.psc (champ de cubes)", () => {
  const scene = loadScene("scene1.psc");

  it("charge la deuxième scène de démo", () => {
    expect(scene.entities.length).toBe(12);
    expect(scene.background).toEqual([8, 8, 20]);
  });
});
