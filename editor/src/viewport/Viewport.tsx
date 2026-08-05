// Viewport : rendu natif 320x240 (upscalé en pixels nets), caméra
// orbitale, sélection d'entité au clic, surlignage de la sélection.

import { useEffect, useRef } from "react";
import * as THREE from "three";
import type { PscScene } from "../formats/psc";
import { buildSceneGraph, applyEntityTransform, type SceneGraph } from "./scene3d";
import { updateLightUniforms, PS1_RESOLUTION } from "./ps1material";

export interface ViewportProps {
  scene: PscScene | null;
  /** Transforms éditées (écrasent celles du fichier), index par entité. */
  overrides: Map<number, { pos: [number, number, number]; rot: [number, number, number]; scale: [number, number, number] }>;
  selected: number;
  onSelect: (index: number) => void;
}

export function Viewport({ scene, overrides, selected, onSelect }: ViewportProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const stateRef = useRef<{
    renderer: THREE.WebGLRenderer;
    camera: THREE.PerspectiveCamera;
    three: THREE.Scene;
    graph: SceneGraph | null;
    highlight: THREE.BoxHelper | null;
  } | null>(null);

  /* Init renderer + boucle + contrôles caméra (une seule fois). */
  useEffect(() => {
    const canvas = canvasRef.current!;
    const renderer = new THREE.WebGLRenderer({ canvas, antialias: false });
    renderer.setSize(PS1_RESOLUTION.x, PS1_RESOLUTION.y, false);
    // Sortie linéaire : couleurs brutes comme sur console (pas de courbe sRGB).
    renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    const camera = new THREE.PerspectiveCamera(53, 320 / 240, 10, 8192);
    const three = new THREE.Scene();
    stateRef.current = { renderer, camera, three, graph: null, highlight: null };

    // Caméra orbitale minimaliste (drag = orbite, molette = zoom,
    // clic droit drag = pan de la cible).
    const orbit = { yaw: 0.5, pitch: 0.45, dist: 900, target: new THREE.Vector3(0, 60, 150) };
    const applyCamera = () => {
      const { yaw, pitch, dist, target } = orbit;
      camera.position.set(
        target.x + dist * Math.sin(yaw) * Math.cos(pitch),
        target.y + dist * Math.sin(pitch),
        target.z - dist * Math.cos(yaw) * Math.cos(pitch),
      );
      camera.lookAt(target);
    };
    applyCamera();

    let dragging = 0;
    let moved = false;
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    canvas.addEventListener("pointerdown", (e) => {
      dragging = e.button === 2 ? 2 : 1;
      moved = false;
      canvas.setPointerCapture(e.pointerId);
    });
    canvas.addEventListener("pointermove", (e) => {
      if (!dragging) return;
      if (Math.abs(e.movementX) + Math.abs(e.movementY) > 1) moved = true;
      if (dragging === 1) {
        orbit.yaw += e.movementX * 0.008;
        orbit.pitch = Math.min(1.4, Math.max(-1.4, orbit.pitch + e.movementY * 0.008));
      } else {
        const right = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 0);
        const up = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 1);
        orbit.target.addScaledVector(right, -e.movementX * orbit.dist * 0.002);
        orbit.target.addScaledVector(up, e.movementY * orbit.dist * 0.002);
      }
      applyCamera();
    });
    canvas.addEventListener("pointerup", (e) => {
      const wasDrag = dragging;
      dragging = 0;
      canvas.releasePointerCapture(e.pointerId);
      // Clic simple (pas un drag) : sélection par raycast.
      if (wasDrag === 1 && !moved && stateRef.current?.graph) {
        const rect = canvas.getBoundingClientRect();
        const ndc = new THREE.Vector2(
          ((e.clientX - rect.left) / rect.width) * 2 - 1,
          -((e.clientY - rect.top) / rect.height) * 2 + 1,
        );
        const ray = new THREE.Raycaster();
        ray.setFromCamera(ndc, camera);
        const hits = ray.intersectObject(stateRef.current.graph.root, true);
        const hit = hits.find((h) => h.object.userData.entityIndex !== undefined);
        if (hit) onSelect(hit.object.userData.entityIndex as number);
      }
    });
    canvas.addEventListener("wheel", (e) => {
      e.preventDefault();
      orbit.dist = Math.min(4000, Math.max(120, orbit.dist * (e.deltaY > 0 ? 1.1 : 0.9)));
      applyCamera();
    });

    let raf = 0;
    const loop = () => {
      raf = requestAnimationFrame(loop);
      const s = stateRef.current;
      if (!s) return;
      if (s.graph) {
        updateLightUniforms(s.graph.materials, s.camera, s.graph.lighting.toward);
      }
      s.highlight?.update();
      s.renderer.render(s.three, s.camera);
    };
    loop();
    return () => {
      cancelAnimationFrame(raf);
      renderer.dispose();
      stateRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* (Re)construction du graphe quand la scène change. */
  useEffect(() => {
    const s = stateRef.current;
    if (!s) return;
    s.three.clear();
    s.graph = null;
    s.highlight = null;
    if (!scene) return;
    const graph = buildSceneGraph(scene);
    s.three.add(graph.root);
    s.three.background = new THREE.Color(
      scene.background[0] / 255,
      scene.background[1] / 255,
      scene.background[2] / 255,
    );
    s.graph = graph;
  }, [scene]);

  /* Transforms éditées + surlignage de sélection. */
  useEffect(() => {
    const s = stateRef.current;
    if (!s?.graph || !scene) return;
    scene.entities.forEach((entity, i) => {
      const o = overrides.get(i);
      applyEntityTransform(
        s.graph!.entityGroups[i],
        o?.pos ?? entity.pos,
        o?.rot ?? entity.rot,
        o?.scale ?? entity.scale,
      );
    });
    if (s.highlight) {
      s.three.remove(s.highlight);
      s.highlight = null;
    }
    if (selected >= 0) {
      const target = s.graph.entityGroups[selected];
      const helper = new THREE.BoxHelper(target, 0xffcc00);
      s.three.add(helper);
      s.highlight = helper;
    }
  }, [scene, overrides, selected]);

  return (
    <canvas
      ref={canvasRef}
      className="viewport-canvas"
      width={PS1_RESOLUTION.x}
      height={PS1_RESOLUTION.y}
    />
  );
}
