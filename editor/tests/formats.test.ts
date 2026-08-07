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
    expect(scene.entities.length).toBe(30);
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

  it("décode les textures TIM (4bpp auto + CLUT)", () => {
    // checker + house en 256x256, l'atlas du perso (guy) en 64x64.
    const sizes = scene.textures.map((t) => t.width).sort((a, b) => a - b);
    expect(sizes).toEqual([64, 256, 256]);
    for (const tex of scene.textures) {
      // Les textures de démo tiennent en 16 couleurs : l'auto-4bpp
      // du pipeline les émet en mode 0 (moitié de la place en VRAM).
      expect(tex.bpp).toBe(4);
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
    // La cheminée (parentée à une maison) + les widgets UI sous leurs
    // canvas (HUD, dialogue, menu pause).
    const children = scene.entities.filter((e) => e.parent >= 0);
    expect(children.length).toBe(17);
    // Le sol a une échelle 4 en X/Z.
    const sol = scene.entities[0];
    expect(sol.scale[0]).toBeCloseTo(4.0, 2);
    expect(sol.scale[2]).toBeCloseTo(4.0, 2);
  });

  it("compte les triangles comme le runtime", () => {
    // sol(288) + 3 maisons(16) + cheminée(12) + perso, pnj (48) + torche(12)
    // + girouette(12) = 468.
    expect(sceneTriangleCount(scene)).toBe(468);
  });
});

describe("parsePsc sur scene0.psc — table UI (v1.3)", () => {
  const scene = loadScene("scene0.psc");

  it("lit les widgets, la police et les chaînes", () => {
    // hud (canvas) + vie_fond + vie (filled) + zone (texte).
    expect(scene.ui.length).toBe(19);
    expect(scene.fontCount).toBe(1);
    const [hud, fond, vie, zone] = scene.ui;
    expect(hud.components & 1).toBe(1);
    expect(fond.size).toEqual([70, 12]);
    expect(fond.anchorMin).toEqual([0, 0]);
    // vie : image filled étirée, amount 0.75, rouge.
    expect(vie.flags & 3).toBe(3);
    expect(vie.data).toBe(3072);
    expect(vie.anchorMax).toEqual([1, 1]);
    expect(vie.color).toEqual([200, 40, 40]);
    // zone : texte aligné à droite, résolu depuis la table de chaînes.
    expect(zone.text).toBe("VILLAGE");
    expect(zone.extra).toBe(2);
    // Les entités UI portent le flag (bit 3).
    expect(scene.entities[hud.entity].flags & 8).toBe(8);
    // vie_fond : image Sliced (type 1) avec ses borders.
    expect(fond.flags & 3).toBe(1);
    expect(fond.border).toEqual([6, 6, 6, 6]);
    // aide : image Tiled (type 2) ; liste : Layout Group vertical.
    expect(scene.ui[4].flags & 3).toBe(2);
    const liste = scene.ui[5];
    expect(liste.components & (1 << 4)).toBe(1 << 4);
    expect(liste.uv).toEqual([4, 4, 4, 4]);
    expect(liste.extra).toBe(4);
    // Jalon 4 : canvas dialogue inactif, boutons du menu pause (bit 3).
    const dialogue = scene.ui[8];
    expect(dialogue.components & (1 << 0)).toBe(1 << 0);
    expect(dialogue.components & (1 << 5)).toBe(0);
    const btn = scene.ui[17];
    expect(btn.components & (1 << 3)).toBe(1 << 3);
    expect(btn.text).toBe("REPRENDRE");
    expect(scene.ui[18].text).toBe("QUITTER");
  });
});

describe("parsePsc sur scene1.psc (champ de cubes)", () => {
  const scene = loadScene("scene1.psc");

  it("charge la deuxième scène de démo", () => {
    expect(scene.entities.length).toBe(16);
    expect(scene.background).toEqual([8, 8, 20]);
  });
});
