// Vignettes du panneau Project : aperçus rendus depuis les fichiers déjà
// convertis de Library/ (les mêmes octets que la console verra).
// - texture : le TIM décodé (donc la palette quantifiée réelle, pas le PNG)
// - modèle : rendu offscreen three.js du PMD, texture appariée par nom de
//   sortie (guy.pmd -> guy.tim), caméra cadrée sur la sphère englobante
// Un seul renderer WebGL partagé (96x96), résultats en dataURL, mémorisés
// par (chemin, taille, sortie) — une réimport change la taille et invalide.

import * as THREE from "three";
import { parsePmd } from "./formats/pmd";
import { parseTim, type TimTexture } from "./formats/tim";
import { modelToGeometry } from "./viewport/scene3d";
import { api, type ProjectFile } from "./bridge";

const SIZE = 96;
const cache = new Map<string, Promise<string | null>>();
let renderer: THREE.WebGLRenderer | null = null;

function getRenderer(): THREE.WebGLRenderer {
  if (!renderer) {
    const canvas = document.createElement("canvas");
    canvas.width = SIZE;
    canvas.height = SIZE;
    renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      alpha: true,
      preserveDrawingBuffer: true,
    });
    renderer.setSize(SIZE, SIZE, false);
    renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    renderer.setClearColor(0x000000, 0);
  }
  return renderer;
}

async function libraryBytes(projectDir: string, name: string): Promise<DataView> {
  const bytes = await api.readLibraryFile(projectDir, name);
  return new DataView(new Uint8Array(bytes).buffer);
}

export function timThumb(tim: TimTexture): string {
  const src = document.createElement("canvas");
  src.width = tim.width;
  src.height = tim.height;
  src.getContext("2d")!.putImageData(new ImageData(tim.rgba, tim.width, tim.height), 0, 0);
  const dst = document.createElement("canvas");
  dst.width = SIZE;
  dst.height = SIZE;
  const ctx = dst.getContext("2d")!;
  // Nearest : une texture PS1 se lit mieux avec ses texels francs.
  ctx.imageSmoothingEnabled = false;
  const scale = Math.min(SIZE / tim.width, SIZE / tim.height);
  const w = tim.width * scale;
  const h = tim.height * scale;
  ctx.drawImage(src, (SIZE - w) / 2, (SIZE - h) / 2, w, h);
  return dst.toDataURL();
}

export function modelThumb(modelData: DataView, tim: TimTexture | null): string {
  const model = parsePmd(modelData);
  const geo = modelToGeometry(model, tim?.width ?? 256, tim?.height ?? 256);
  // MeshLambert attend l'attribut "color" (0-255 -> 0-1).
  const base = geo.getAttribute("baseColor");
  const colors = new Float32Array(base.count * 3);
  for (let i = 0; i < colors.length; i++) colors[i] = (base.array as Float32Array)[i] / 255;
  geo.setAttribute("color", new THREE.BufferAttribute(colors, 3));

  let texture: THREE.DataTexture | null = null;
  if (tim) {
    texture = new THREE.DataTexture(tim.rgba, tim.width, tim.height, THREE.RGBAFormat);
    texture.magFilter = THREE.NearestFilter;
    texture.minFilter = THREE.NearestFilter;
    texture.flipY = false;
    texture.needsUpdate = true;
  }
  const material = texture
    ? new THREE.MeshLambertMaterial({ map: texture })
    : new THREE.MeshLambertMaterial({ vertexColors: true });
  material.side = THREE.DoubleSide;

  const scene = new THREE.Scene();
  // Monde PS1 (+Y bas) -> three (+Y haut), comme le viewport.
  const root = new THREE.Group();
  root.scale.set(1, -1, 1);
  root.add(new THREE.Mesh(geo, material));
  scene.add(root);
  scene.add(new THREE.AmbientLight(0xffffff, 1.1));
  const sun = new THREE.DirectionalLight(0xffffff, 2.0);
  sun.position.set(1, 1.4, 1);
  scene.add(sun);

  geo.computeBoundingSphere();
  const sphere = geo.boundingSphere!;
  const center = sphere.center.clone();
  center.y = -center.y; // miroir racine
  const cam = new THREE.PerspectiveCamera(35, 1, 1, Math.max(sphere.radius * 20, 100));
  cam.position
    .copy(center)
    .addScaledVector(new THREE.Vector3(1, 0.72, 1).normalize(), sphere.radius * 3.4);
  cam.lookAt(center);

  const r = getRenderer();
  r.render(scene, cam);
  const url = r.domElement.toDataURL();
  geo.dispose();
  material.dispose();
  texture?.dispose();
  return url;
}

/** Vignette d'un fichier du panneau (null : garder l'icône). */
export function thumbFor(
  projectDir: string,
  file: ProjectFile,
  allFiles: ProjectFile[],
): Promise<string | null> {
  if (!file.out || !file.exists || (file.kind !== "model" && file.kind !== "texture")) {
    return Promise.resolve(null);
  }
  const key = `${projectDir}:${file.path}:${file.size}:${file.out}`;
  let pending = cache.get(key);
  if (!pending) {
    pending = build().catch(() => null);
    cache.set(key, pending);
  }
  return pending;

  async function build(): Promise<string | null> {
    if (file.kind === "texture") {
      return timThumb(parseTim(await libraryBytes(projectDir, file.out!)));
    }
    // Modèle : texture appariée par nom de sortie (guy.pmd -> guy.tim).
    const timOut = file.out!.replace(/\.pmd$/i, ".tim");
    const timFile = allFiles.find((f) => f.out === timOut);
    const tim = timFile ? parseTim(await libraryBytes(projectDir, timOut)) : null;
    return modelThumb(await libraryBytes(projectDir, file.out!), tim);
  }
}
