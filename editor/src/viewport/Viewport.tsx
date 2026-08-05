// Viewport : rendu natif 320x240 (upscalé en pixels nets), sélection au
// clic, et caméra style Unity :
//   clic gauche (drag)   orbite autour du point visé
//   clic gauche (clic)   sélection d'entité
//   clic droit (tenu)    regard FPS + déplacement ZQSD/WASD (codes
//                        physiques : marche en AZERTY comme en QWERTY),
//                        E/Espace monter, Q descendre, Shift = rapide
//   clic milieu (drag)   pan
//   molette              avancer/reculer (dolly)

import { useEffect, useRef } from "react";
import * as THREE from "three";
import { TransformControls } from "three/examples/jsm/controls/TransformControls.js";
import type { PscScene } from "../formats/psc";
import { buildSceneGraph, applyEntityTransform, type SceneGraph } from "./scene3d";
import { updateLightUniforms, PS1_RESOLUTION } from "./ps1material";

export type GizmoMode = "translate" | "rotate" | "scale";

type Transform = {
  pos: [number, number, number];
  rot: [number, number, number];
  scale: [number, number, number];
};

export interface ViewportProps {
  scene: PscScene | null;
  /** Transforms éditées (écrasent celles du fichier), index par entité. */
  overrides: Map<number, { pos: [number, number, number]; rot: [number, number, number]; scale: [number, number, number] }>;
  selected: number;
  onSelect: (index: number) => void;
  /** Mode du gizmo de manipulation (défaut : translate). */
  gizmoMode?: GizmoMode;
  /** Édition par gizmo : transform locale PS1 de l'entité sélectionnée. */
  onTransform?: (index: number, t: Transform) => void;
  /** Début/fin d'un drag de gizmo (permet de différer les rebuilds). */
  onGizmoDragging?: (dragging: boolean) => void;
}

const MOVE_SPEED = 420; // unités monde / seconde
const LOOK_SPEED = 0.0045;

export function Viewport({
  scene,
  overrides,
  selected,
  onSelect,
  gizmoMode = "translate",
  onTransform,
  onGizmoDragging,
}: ViewportProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const stateRef = useRef<{
    renderer: THREE.WebGLRenderer;
    camera: THREE.PerspectiveCamera;
    three: THREE.Scene;
    graph: SceneGraph | null;
    highlight: THREE.BoxHelper | null;
    gizmo: TransformControls;
  } | null>(null);

  /* Callbacks/état accessibles depuis les closures d'init (une seule fois). */
  const liveRef = useRef({ selected, onTransform, onGizmoDragging });
  useEffect(() => {
    liveRef.current = { selected, onTransform, onGizmoDragging };
  });

  /* Init renderer + boucle + contrôles caméra (une seule fois). */
  useEffect(() => {
    const canvas = canvasRef.current!;
    const renderer = new THREE.WebGLRenderer({ canvas, antialias: false });
    renderer.setSize(PS1_RESOLUTION.x, PS1_RESOLUTION.y, false);
    // Sortie linéaire : couleurs brutes comme sur console (pas de courbe sRGB).
    renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    const camera = new THREE.PerspectiveCamera(53, 320 / 240, 10, 8192);
    const three = new THREE.Scene();

    /* Gizmo de manipulation (position/rotation/échelle). Les entités
     * portent leur transform locale PS1 directement sur leur groupe :
     * on la relit telle quelle après manipulation. */
    const gizmo = new TransformControls(camera, canvas);
    gizmo.setSize(0.85);
    three.add(gizmo.getHelper());
    gizmo.addEventListener("dragging-changed", (e) => {
      liveRef.current.onGizmoDragging?.(Boolean((e as { value: unknown }).value));
    });
    gizmo.addEventListener("objectChange", () => {
      const obj = gizmo.object;
      const live = liveRef.current;
      if (!obj || live.selected < 0 || !live.onTransform) return;
      const toUnits = (rad: number) => {
        let u = Math.round((rad / (Math.PI * 2)) * 4096) % 4096;
        if (u < 0) u += 4096;
        return u;
      };
      const round3 = (x: number) => Math.round(x * 1000) / 1000;
      live.onTransform(live.selected, {
        pos: [Math.round(obj.position.x), Math.round(obj.position.y), Math.round(obj.position.z)],
        rot: [toUnits(obj.rotation.x), toUnits(obj.rotation.y), toUnits(obj.rotation.z)],
        scale: [round3(obj.scale.x), round3(obj.scale.y), round3(obj.scale.z)],
      });
    });
    /* Ctrl tenu : snap (10 unités, 15°, 0.1). */
    const onSnapKey = (e: KeyboardEvent) => {
      if (e.key !== "Control") return;
      const down = e.type === "keydown";
      gizmo.translationSnap = down ? 10 : null;
      gizmo.rotationSnap = down ? Math.PI / 12 : null;
      gizmo.scaleSnap = down ? 0.1 : null;
    };
    window.addEventListener("keydown", onSnapKey);
    window.addEventListener("keyup", onSnapKey);

    stateRef.current = { renderer, camera, three, graph: null, highlight: null, gizmo };

    /* État caméra : position + regard (yaw/pitch), pivot d'orbite à
     * distance `dist` devant la caméra. */
    const cam = {
      pos: new THREE.Vector3(),
      yaw: 0.5,
      pitch: -0.45,
      dist: 900,
    };
    const forward = () =>
      new THREE.Vector3(
        Math.sin(cam.yaw) * Math.cos(cam.pitch),
        Math.sin(cam.pitch),
        -Math.cos(cam.yaw) * Math.cos(cam.pitch),
      );
    // Position initiale : reproduit l'ancienne vue orbitale par défaut.
    {
      const target = new THREE.Vector3(0, 60, 150);
      cam.pos.copy(target).addScaledVector(forward(), -cam.dist);
    }
    const applyCamera = () => {
      camera.position.copy(cam.pos);
      camera.rotation.set(cam.pitch, -cam.yaw, 0, "YXZ");
    };
    applyCamera();

    /* Souris. */
    let dragButton = -1;
    let moved = false;
    const keys = new Set<string>();
    canvas.tabIndex = 0; // focus clavier
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    canvas.addEventListener("pointerdown", (e) => {
      // Le gizmo a priorité : survol d'un axe ou drag en cours.
      if (gizmo.dragging || gizmo.axis) {
        canvas.focus();
        return;
      }
      dragButton = e.button;
      moved = false;
      canvas.setPointerCapture(e.pointerId);
      canvas.focus();
    });
    canvas.addEventListener("pointermove", (e) => {
      if (dragButton < 0) return;
      if (Math.abs(e.movementX) + Math.abs(e.movementY) > 1) moved = true;
      if (dragButton === 0) {
        // Orbite autour du pivot devant la caméra.
        const pivot = cam.pos.clone().addScaledVector(forward(), cam.dist);
        cam.yaw += e.movementX * LOOK_SPEED;
        cam.pitch = THREE.MathUtils.clamp(
          cam.pitch - e.movementY * LOOK_SPEED,
          -1.45,
          1.45,
        );
        cam.pos.copy(pivot).addScaledVector(forward(), -cam.dist);
      } else if (dragButton === 2) {
        // Regard FPS : la position ne bouge pas.
        cam.yaw += e.movementX * LOOK_SPEED;
        cam.pitch = THREE.MathUtils.clamp(
          cam.pitch - e.movementY * LOOK_SPEED,
          -1.45,
          1.45,
        );
      } else {
        // Pan.
        const right = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 0);
        const up = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 1);
        cam.pos.addScaledVector(right, -e.movementX * cam.dist * 0.0018);
        cam.pos.addScaledVector(up, e.movementY * cam.dist * 0.0018);
      }
      applyCamera();
    });
    canvas.addEventListener("pointerup", (e) => {
      const was = dragButton;
      dragButton = -1;
      canvas.releasePointerCapture(e.pointerId);
      if (was === 0 && !moved && stateRef.current?.graph) {
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
      const step = cam.dist * (e.deltaY > 0 ? 0.1 : -0.1);
      cam.pos.addScaledVector(forward(), -step);
      cam.dist = THREE.MathUtils.clamp(cam.dist + step, 60, 4000);
      applyCamera();
    });

    /* Clavier (codes physiques : ZQSD azerty = WASD qwerty). */
    canvas.addEventListener("keydown", (e) => {
      keys.add(e.code);
      if (
        ["KeyW", "KeyA", "KeyS", "KeyD", "KeyQ", "KeyE", "Space"].includes(e.code)
      ) {
        e.preventDefault();
      }
    });
    canvas.addEventListener("keyup", (e) => keys.delete(e.code));
    canvas.addEventListener("blur", () => keys.clear());

    /* Boucle : déplacement continu + rendu. */
    let raf = 0;
    let last = performance.now();
    const loop = (now: number) => {
      raf = requestAnimationFrame(loop);
      const dt = Math.min((now - last) / 1000, 0.1);
      last = now;

      const s = stateRef.current;
      if (!s) return;

      if (keys.size > 0) {
        const speed = MOVE_SPEED * dt * (keys.has("ShiftLeft") || keys.has("ShiftRight") ? 3 : 1);
        const fwd = forward();
        const right = new THREE.Vector3(Math.cos(cam.yaw), 0, Math.sin(cam.yaw));
        if (keys.has("KeyW")) cam.pos.addScaledVector(fwd, speed);
        if (keys.has("KeyS")) cam.pos.addScaledVector(fwd, -speed);
        if (keys.has("KeyA")) cam.pos.addScaledVector(right, -speed);
        if (keys.has("KeyD")) cam.pos.addScaledVector(right, speed);
        if (keys.has("KeyE") || keys.has("Space")) cam.pos.y += speed;
        if (keys.has("KeyQ")) cam.pos.y -= speed;
        applyCamera();
      }

      if (s.graph) {
        updateLightUniforms(s.graph.materials, s.camera, s.graph.lighting.toward);
      }
      s.highlight?.update();
      s.renderer.render(s.three, s.camera);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("keydown", onSnapKey);
      window.removeEventListener("keyup", onSnapKey);
      gizmo.dispose();
      renderer.dispose();
      stateRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* Mode du gizmo. */
  useEffect(() => {
    stateRef.current?.gizmo.setMode(gizmoMode);
  }, [gizmoMode]);

  /* (Re)construction du graphe quand la scène change. */
  useEffect(() => {
    const s = stateRef.current;
    if (!s) return;
    s.gizmo.detach();
    s.three.clear();
    s.three.add(s.gizmo.getHelper()); // clear() l'a retiré
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
    if (selected >= 0 && selected < s.graph.entityGroups.length) {
      const helper = new THREE.BoxHelper(s.graph.entityGroups[selected], 0xffcc00);
      s.three.add(helper);
      s.highlight = helper;
      // Attacher le gizmo (sans churn pendant un drag : même groupe = no-op).
      if (s.gizmo.object !== s.graph.entityGroups[selected]) {
        s.gizmo.attach(s.graph.entityGroups[selected]);
      }
    } else {
      s.gizmo.detach();
    }
  }, [scene, overrides, selected]);

  return (
    <canvas
      ref={canvasRef}
      className="viewport-canvas"
      width={PS1_RESOLUTION.x}
      height={PS1_RESOLUTION.y}
      title="Gizmo : 1 déplacer · 2 rotation · 3 échelle · Ctrl = snap — Clic gauche : orbite/sélection · Clic droit tenu : caméra FPS (ZQSD, E/Espace ↑, Q ↓, Shift rapide) · Molette : avancer · Clic milieu : pan"
    />
  );
}
