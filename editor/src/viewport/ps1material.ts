// Matériau "PS1 authentique" : vertex snapping sur la grille 320x240,
// mapping affine (annulation de la correction de perspective via uv*w),
// éclairage Gouraud par sommet (1 directionnelle + ambiante, mêmes
// formules que le runtime GTE et l'outil preview Rust).

import * as THREE from "three";

export interface Ps1Lighting {
  /** Vecteur unitaire VERS la source (espace monde PS1). */
  toward: [number, number, number];
  color: [number, number, number]; // 0-255
  ambient: [number, number, number]; // 0-255
}

export const PS1_RESOLUTION = new THREE.Vector2(320, 240);

const vertexShader = /* glsl */ `
  uniform vec2 uResolution;
  uniform vec3 uLightViewDir;   // vers la source, espace vue
  uniform vec3 uLightColor;     // 0-1
  uniform vec3 uAmbient;        // 0-1
  attribute vec3 baseColor;     // couleur de base du paquet, 128 = neutre
  varying vec3 vColor;
  varying vec3 vUvW;            // uv * w (mapping affine) + w

  void main() {
    vec4 clip = projectionMatrix * modelViewMatrix * vec4(position, 1.0);

    // Vertex snapping : coordonnées écran entières, le "jitter" PS1.
    vec2 half_res = uResolution * 0.5;
    clip.xy = floor(clip.xy / clip.w * half_res) / half_res * clip.w;

    // Gouraud : CC = ambiante + couleur lumière * max(0, N.L), comme le GTE.
    vec3 n = normalize(normalMatrix * normal);
    float intensity = max(dot(n, uLightViewDir), 0.0);
    // Le GTE clampe CC à 255 avant la modulation GPU.
    vec3 cc = min(uAmbient + uLightColor * intensity, vec3(1.0));
    // Modulation GPU : 128 = neutre (facteur (base/128) * (CC*255/128)).
    vColor = (baseColor / 128.0) * (cc * 255.0 / 128.0);

    // Affine : uv pré-multipliées par w, re-divisées au fragment.
    vUvW = vec3(uv * clip.w, clip.w);
    gl_Position = clip;
  }
`;

const fragmentShader = /* glsl */ `
  uniform sampler2D uMap;
  uniform bool uTextured;
  varying vec3 vColor;
  varying vec3 vUvW;

  void main() {
    vec3 color = vColor;
    if (uTextured) {
      vec2 uv = vUvW.xy / vUvW.z;
      vec4 texel = texture2D(uMap, uv);
      if (texel.a < 0.5) discard;   // texel 0x0000 = transparent
      color *= texel.rgb;
    } else {
      color *= 0.5; // base non texturée : 0-255 directs (pas de modulation)
    }
    gl_FragColor = vec4(min(color, 1.0), 1.0);
  }
`;

export function makePs1Material(
  texture: THREE.DataTexture | null,
  lighting: Ps1Lighting,
): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader,
    fragmentShader,
    uniforms: {
      uResolution: { value: PS1_RESOLUTION.clone() },
      uMap: { value: texture },
      uTextured: { value: texture !== null },
      uLightViewDir: { value: new THREE.Vector3() },
      uLightColor: {
        value: new THREE.Vector3(...lighting.color.map((c) => c / 255)),
      },
      uAmbient: {
        value: new THREE.Vector3(...lighting.ambient.map((c) => c / 255)),
      },
    },
    side: THREE.DoubleSide,
  });
}

/** À appeler chaque frame : lumière monde (PS1) → espace vue caméra. */
export function updateLightUniforms(
  materials: THREE.ShaderMaterial[],
  camera: THREE.Camera,
  toward: [number, number, number],
) {
  // Monde PS1 (+Y bas) → monde three (+Y haut) : Y négé par le groupe
  // racine, donc la direction lumière doit l'être aussi.
  const world = new THREE.Vector3(toward[0], -toward[1], toward[2]);
  const view = world
    .clone()
    .transformDirection(camera.matrixWorldInverse)
    .normalize();
  for (const m of materials) {
    (m.uniforms.uLightViewDir.value as THREE.Vector3).copy(view);
  }
}
