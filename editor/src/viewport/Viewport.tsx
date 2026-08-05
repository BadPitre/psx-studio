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
import {
  ENTITY_FLAG_CAMERA,
  ENTITY_FLAG_LIGHT,
  ENTITY_FLAG_LIGHT_POINT,
  PS1_DEFAULT_FOV,
  type PscScene,
} from "../formats/psc";
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
  /** Dithering Bayer + sortie 15 bits, comme la console (défaut : on). */
  dither?: boolean;
  /** Back-face culling console (défaut : off = double face, ergonomique). */
  culling?: boolean;
  /** Drop d'un modèle du panneau Project : instancier à la position visée
   * (coordonnées monde PS1, posée sur le plan du sol y=0). */
  onModelDrop?: (out: string, pos: [number, number, number]) => void;
}

const MOVE_SPEED = 420; // unités monde / seconde
const LOOK_SPEED = 0.0045;
/* Les groupes vivent sous la racine miroir (scale.y = -1, monde PS1 ->
 * three). Pour poser une caméra three sur une entité il faut conjuguer :
 * M_three = M_groupe · S(1,-1,1) — translation intacte, orientation
 * remise en base directe (det > 0), sinon l'image sort à l'envers. */
const MIRROR_Y = new THREE.Matrix4().makeScale(1, -1, 1);

export function Viewport({
  scene,
  overrides,
  selected,
  onSelect,
  gizmoMode = "translate",
  onTransform,
  onGizmoDragging,
  dither = true,
  culling = false,
  onModelDrop,
}: ViewportProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const overlayRef = useRef<HTMLCanvasElement>(null);
  const pipRef = useRef<HTMLCanvasElement>(null);
  const stateRef = useRef<{
    renderer: THREE.WebGLRenderer;
    camera: THREE.PerspectiveCamera;
    three: THREE.Scene;
    /** Calque pleine résolution au-dessus du 320x240 : gizmo, surlignage. */
    overlayScene: THREE.Scene;
    graph: SceneGraph | null;
    highlight: THREE.BoxHelper | null;
    gizmo: TransformControls;
    /** Marqueurs de composants (flèche/sphère lumière, frustum caméra). */
    flagHelpers: {
      kind: "light" | "point" | "camera";
      entity: number;
      obj: THREE.Object3D;
      cam?: THREE.PerspectiveCamera;
    }[];
  } | null>(null);

  /* Callbacks/état accessibles depuis les closures d'init (une seule fois). */
  const liveRef = useRef({ selected, onTransform, onGizmoDragging });
  useEffect(() => {
    liveRef.current = { selected, onTransform, onGizmoDragging };
  });

  /* Init renderer + boucle + contrôles caméra (une seule fois). */
  useEffect(() => {
    const canvas = canvasRef.current!;
    const overlay = overlayRef.current!;
    const renderer = new THREE.WebGLRenderer({ canvas, antialias: false });
    renderer.setSize(PS1_RESOLUTION.x, PS1_RESOLUTION.y, false);
    // Sortie linéaire : couleurs brutes comme sur console (pas de courbe sRGB).
    renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    const camera = new THREE.PerspectiveCamera(53, 320 / 240, 10, 8192);
    const three = new THREE.Scene();

    /* Calque d'édition pleine résolution : la scène reste rendue en
     * 320x240 pixelisé, mais gizmo et surlignage sont dessinés nets
     * au-dessus, avec la même caméra. L'overlay reçoit la souris. */
    const overlayRenderer = new THREE.WebGLRenderer({
      canvas: overlay,
      antialias: true,
      alpha: true,
    });
    overlayRenderer.setPixelRatio(window.devicePixelRatio);
    overlayRenderer.setClearColor(0x000000, 0);
    const overlayScene = new THREE.Scene();
    const resize = () => {
      const r = overlay.getBoundingClientRect();
      if (r.width > 0) overlayRenderer.setSize(r.width, r.height, false);
    };
    resize();
    const resizeObs = new ResizeObserver(resize);
    resizeObs.observe(overlay);

    /* Prévisualisation caméra (PiP) : la vue de l'entité caméra
     * sélectionnée, rendue en vrai 320x240 pixelisé, façon Unreal. */
    const pip = pipRef.current!;
    const pipRenderer = new THREE.WebGLRenderer({ canvas: pip, antialias: false });
    pipRenderer.setSize(PS1_RESOLUTION.x, PS1_RESOLUTION.y, false);
    pipRenderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    const pipCam = new THREE.PerspectiveCamera(PS1_DEFAULT_FOV, 320 / 240, 10, 8192);
    pipCam.matrixAutoUpdate = false;

    /* Gizmo de manipulation (position/rotation/échelle). Les entités
     * portent leur transform locale PS1 directement sur leur groupe :
     * on la relit telle quelle après manipulation. */
    const gizmo = new TransformControls(camera, overlay);
    gizmo.setSize(0.85);
    overlayScene.add(gizmo.getHelper());
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

    stateRef.current = {
      renderer,
      camera,
      three,
      overlayScene,
      graph: null,
      highlight: null,
      gizmo,
      flagHelpers: [],
    };

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

    /* Souris (sur l'overlay, qui couvre le canvas 320x240). */
    let dragButton = -1;
    let moved = false;
    const keys = new Set<string>();
    overlay.tabIndex = 0; // focus clavier
    overlay.addEventListener("contextmenu", (e) => e.preventDefault());
    overlay.addEventListener("pointerdown", (e) => {
      // Le gizmo a priorité : survol d'un axe ou drag en cours.
      if (gizmo.dragging || gizmo.axis) {
        overlay.focus();
        return;
      }
      dragButton = e.button;
      moved = false;
      overlay.setPointerCapture(e.pointerId);
      overlay.focus();
    });
    overlay.addEventListener("pointermove", (e) => {
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
    overlay.addEventListener("pointerup", (e) => {
      const was = dragButton;
      dragButton = -1;
      overlay.releasePointerCapture(e.pointerId);
      if (was === 0 && !moved && stateRef.current?.graph) {
        const rect = overlay.getBoundingClientRect();
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
    overlay.addEventListener("wheel", (e) => {
      e.preventDefault();
      const step = cam.dist * (e.deltaY > 0 ? 0.1 : -0.1);
      cam.pos.addScaledVector(forward(), -step);
      cam.dist = THREE.MathUtils.clamp(cam.dist + step, 60, 4000);
      applyCamera();
    });

    /* Clavier (codes physiques : ZQSD azerty = WASD qwerty). */
    overlay.addEventListener("keydown", (e) => {
      keys.add(e.code);
      if (
        ["KeyW", "KeyA", "KeyS", "KeyD", "KeyQ", "KeyE", "Space"].includes(e.code)
      ) {
        e.preventDefault();
      }
    });
    overlay.addEventListener("keyup", (e) => keys.delete(e.code));
    overlay.addEventListener("blur", () => keys.clear());

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

      // Lumière 0 = soleil des settings ; 1-2 = entités-lumières, dont
      // la direction suit la rotation courante du groupe (gizmo,
      // overrides) : +Z local en monde three (le miroir Y de la racine
      // est inclus dans matrixWorld).
      const currentLights = (graph: SceneGraph) => {
        const t = graph.lighting.toward;
        const lights = [
          {
            towardThree: new THREE.Vector3(t[0], -t[1], t[2]),
            color: graph.lighting.color,
          },
        ];
        for (const light of graph.lights) {
          if (graph.entityFlags[light.entity] & ENTITY_FLAG_LIGHT_POINT) continue;
          const group = graph.entityGroups[light.entity];
          if (!group || lights.length >= 3) continue;
          lights.push({
            towardThree: new THREE.Vector3(0, 0, 1).transformDirection(
              group.matrixWorld,
            ),
            // Intensité appliquée à la couleur (le shader clampe la somme).
            color: light.color.map((c) => c * light.intensity) as [
              number,
              number,
              number,
            ],
          });
        }
        return lights;
      };
      const currentPoints = (graph: SceneGraph) => {
        const points = [];
        for (const light of graph.lights) {
          if (!(graph.entityFlags[light.entity] & ENTITY_FLAG_LIGHT_POINT)) continue;
          const group = graph.entityGroups[light.entity];
          if (!group || points.length >= 4) continue;
          points.push({
            posThree: new THREE.Vector3().setFromMatrixPosition(group.matrixWorld),
            color: light.color.map((c) => c * light.intensity) as [
              number,
              number,
              number,
            ],
            radius: graph.entityLightRadius[light.entity] || 300,
          });
        }
        return points;
      };

      if (s.graph) {
        s.graph.root.updateMatrixWorld(true);
        updateLightUniforms(
          s.graph.materials,
          s.camera,
          currentLights(s.graph),
          currentPoints(s.graph),
        );

        // Marqueurs de composants : suivent la transform courante.
        for (const h of s.flagHelpers) {
          const group = s.graph.entityGroups[h.entity];
          if (!group) continue;
          if (h.kind === "light") {
            const arrow = h.obj as THREE.ArrowHelper;
            arrow.position.setFromMatrixPosition(group.matrixWorld);
            arrow.setDirection(
              new THREE.Vector3(0, 0, -1).transformDirection(group.matrixWorld),
            );
          } else if (h.kind === "point") {
            h.obj.position.setFromMatrixPosition(group.matrixWorld);
          } else if (h.cam) {
            // Convention unique : l'entité regarde vers -Z local, comme
            // les caméras three (conjugaison du miroir racine).
            h.cam.matrixWorld.copy(group.matrixWorld).multiply(MIRROR_Y);
            h.obj.matrixWorldNeedsUpdate = true;
          }
        }
      }
      s.highlight?.update();
      s.renderer.render(s.three, s.camera);
      overlayRenderer.render(s.overlayScene, s.camera);

      /* PiP : rendu depuis l'entité caméra sélectionnée. Les uniforms de
       * lumière sont en espace vue -> les recalculer pour cette caméra
       * (ils sont refaits pour la vue éditeur au tick suivant). */
      const sel = liveRef.current.selected;
      const pipActive =
        s.graph !== null &&
        sel >= 0 &&
        (s.graph.entityFlags[sel] & ENTITY_FLAG_CAMERA) !== 0;
      pip.style.display = pipActive ? "block" : "none";
      if (pipActive && s.graph) {
        const group = s.graph.entityGroups[sel];
        pipCam.matrixWorld.copy(group.matrixWorld).multiply(MIRROR_Y);
        pipCam.matrixWorldInverse.copy(pipCam.matrixWorld).invert();
        const fov = s.graph.entityCamFov[sel] || PS1_DEFAULT_FOV;
        // Distance d'affichage : le far plane du PiP simule le culling.
        const far = s.graph.entityCamDraw[sel] || 8192;
        if (Math.abs(pipCam.fov - fov) > 0.01 || Math.abs(pipCam.far - far) > 1) {
          pipCam.fov = fov;
          pipCam.far = far;
          pipCam.updateProjectionMatrix();
        }
        updateLightUniforms(
          s.graph.materials,
          pipCam,
          currentLights(s.graph),
          currentPoints(s.graph),
        );
        pipRenderer.render(s.three, pipCam);
      }
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("keydown", onSnapKey);
      window.removeEventListener("keyup", onSnapKey);
      resizeObs.disconnect();
      gizmo.dispose();
      pipRenderer.dispose();
      overlayRenderer.dispose();
      renderer.dispose();
      stateRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* Mode du gizmo. */
  useEffect(() => {
    stateRef.current?.gizmo.setMode(gizmoMode);
  }, [gizmoMode]);

  /* Options de rendu (dithering console, culling). Le graphe est
   * reconstruit à chaque scène : re-appliquer aussi dans ce cas. */
  useEffect(() => {
    const s = stateRef.current;
    if (!s?.graph) return;
    for (const m of s.graph.materials) {
      m.uniforms.uDither.value = dither;
      // La racine porte scale.y = -1 (monde PS1 -> three) : le winding vu
      // par GL est inversé, la face avant console correspond à BackSide.
      m.side = culling ? THREE.BackSide : THREE.DoubleSide;
    }
  }, [scene, dither, culling]);

  /* (Re)construction du graphe quand la scène change. */
  useEffect(() => {
    const s = stateRef.current;
    if (!s) return;
    s.gizmo.detach();
    s.three.clear();
    if (s.highlight) s.overlayScene.remove(s.highlight);
    for (const h of s.flagHelpers) s.overlayScene.remove(h.obj);
    s.flagHelpers = [];
    s.graph = null;
    s.highlight = null;
    if (!scene) return;
    const graph = buildSceneGraph(scene);
    s.three.add(graph.root);

    /* Marqueurs de composants dans le calque net. */
    graph.entityFlags.forEach((flags, i) => {
      if (flags & ENTITY_FLAG_LIGHT) {
        const c = graph.lights.find((l) => l.entity === i)?.color ?? [255, 255, 255];
        const hex = new THREE.Color(c[0] / 255, c[1] / 255, c[2] / 255).getHex();
        if (flags & ENTITY_FLAG_LIGHT_POINT) {
          // Torche : sphère filaire au rayon d'action.
          const radius = graph.entityLightRadius[i] || 300;
          const sphere = new THREE.LineSegments(
            new THREE.WireframeGeometry(new THREE.SphereGeometry(radius, 12, 8)),
            new THREE.LineBasicMaterial({ color: hex, transparent: true, opacity: 0.35 }),
          );
          s.overlayScene.add(sphere);
          s.flagHelpers.push({ kind: "point", entity: i, obj: sphere });
        } else {
          const arrow = new THREE.ArrowHelper(
            new THREE.Vector3(0, 0, -1),
            new THREE.Vector3(),
            130,
            hex,
            36,
            20,
          );
          s.overlayScene.add(arrow);
          s.flagHelpers.push({ kind: "light", entity: i, obj: arrow });
        }
      }
      if (flags & ENTITY_FLAG_CAMERA) {
        const fov = graph.entityCamFov[i] || PS1_DEFAULT_FOV;
        // Le frustum matérialise la distance d'affichage quand elle est
        // bornée (sinon une longueur d'aperçu raisonnable).
        const far = graph.entityCamDraw[i] || 300;
        const cam = new THREE.PerspectiveCamera(fov, 4 / 3, 30, far);
        cam.matrixAutoUpdate = false;
        cam.updateProjectionMatrix();
        const helper = new THREE.CameraHelper(cam);
        s.overlayScene.add(helper);
        s.flagHelpers.push({ kind: "camera", entity: i, obj: helper, cam });
      }
    });
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
      s.overlayScene.remove(s.highlight);
      s.highlight = null;
    }
    if (selected >= 0 && selected < s.graph.entityGroups.length) {
      const helper = new THREE.BoxHelper(s.graph.entityGroups[selected], 0xffcc00);
      s.overlayScene.add(helper);
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
    <div className="viewport-stack">
      <canvas
        ref={canvasRef}
        className="viewport-canvas"
        width={PS1_RESOLUTION.x}
        height={PS1_RESOLUTION.y}
      />
      <canvas
        ref={overlayRef}
        className="viewport-overlay"
        title="Gizmo : 1 déplacer · 2 rotation · 3 échelle · Ctrl = snap — Clic gauche : orbite/sélection · Clic droit tenu : caméra FPS (ZQSD, E/Espace ↑, Q ↓, Shift rapide) · Molette : avancer · Clic milieu : pan"
        onDragOver={(e) => {
          if (onModelDrop) e.preventDefault();
        }}
        onDrop={(e) => {
          const st = stateRef.current;
          const out = e.dataTransfer.getData("text/psx-model");
          if (!onModelDrop || !st || !out) return;
          e.preventDefault();
          const rect = e.currentTarget.getBoundingClientRect();
          const ndc = new THREE.Vector2(
            ((e.clientX - rect.left) / rect.width) * 2 - 1,
            -((e.clientY - rect.top) / rect.height) * 2 + 1,
          );
          const ray = new THREE.Raycaster();
          ray.setFromCamera(ndc, st.camera);
          // Sol du monde PS1 : plan y=0 (le miroir racine n'y change rien
          // — x et z passent tels quels).
          const hit = new THREE.Vector3();
          if (ray.ray.intersectPlane(new THREE.Plane(new THREE.Vector3(0, 1, 0), 0), hit)) {
            onModelDrop(out, [Math.round(hit.x), 0, Math.round(hit.z)]);
          } else {
            onModelDrop(out, [0, 0, 0]);
          }
        }}
      />
      <canvas
        ref={pipRef}
        className="viewport-pip"
        width={PS1_RESOLUTION.x}
        height={PS1_RESOLUTION.y}
        title="Vue de la caméra sélectionnée (rendu console 320x240)"
      />
    </div>
  );
}
