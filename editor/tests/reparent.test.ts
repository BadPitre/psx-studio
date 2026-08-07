// Reparentage : la position MONDE doit être conservée (comme Unity).
import { describe, expect, it } from "vitest";
import { localAfterReparent, worldMatrix } from "../src/transforms";
import * as THREE from "three";

const worldPos = (ents: Record<string, unknown>[], name: string) =>
  new THREE.Vector3().setFromMatrixPosition(worldMatrix(ents, name));

describe("localAfterReparent", () => {
  it("conserve la position monde en devenant enfant", () => {
    const ents = [
      { name: "parent", position: [100, 0, 50], rotation: [0, 90, 0] },
      { name: "objet", position: [10, 0, 0] },
    ];
    const before = worldPos(ents, "objet");
    const local = localAfterReparent(ents, "objet", "parent");
    const after = [
      ents[0],
      { name: "objet", parent: "parent", ...local },
    ];
    expect(worldPos(after, "objet").distanceTo(before)).toBeLessThan(1.5);
    // La locale n'est PAS la position monde : elle est exprimée dans le
    // repère du parent (tourné de 90°).
    expect(local.position).not.toEqual([10, 0, 0]);
  });

  it("conserve la position monde en sortant à la racine", () => {
    const ents = [
      { name: "parent", position: [100, -20, 50], rotation: [0, 45, 0], scale: [2, 2, 2] },
      { name: "objet", parent: "parent", position: [10, 0, 0] },
    ];
    const before = worldPos(ents, "objet");
    const local = localAfterReparent(ents, "objet", undefined);
    const after = [ents[0], { name: "objet", ...local }];
    expect(worldPos(after, "objet").distanceTo(before)).toBeLessThan(1.5);
    // Échelle du parent reprise dans la locale (2x).
    expect(local.scale[0]).toBeCloseTo(2, 2);
  });

  it("gère un enfant d'enfant (chaîne d'ancêtres)", () => {
    const ents = [
      { name: "a", position: [50, 0, 0] },
      { name: "b", parent: "a", position: [0, -30, 0], rotation: [0, 90, 0] },
      { name: "c", parent: "b", position: [20, 0, 0] },
      { name: "autre", position: [-80, 0, 10], rotation: [0, 180, 0] },
    ];
    const before = worldPos(ents, "c");
    const local = localAfterReparent(ents, "c", "autre");
    const after = [ents[0], ents[1], { name: "c", parent: "autre", ...local }, ents[3]];
    expect(worldPos(after, "c").distanceTo(before)).toBeLessThan(1.5);
  });
});
