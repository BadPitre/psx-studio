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
  // 3 lumières directionnelles, comme le GTE (soleil + 2 entités).
  uniform vec3 uLightViewDir[3];  // vers la source, espace vue
  uniform vec3 uLightColor[3];    // 0-1 (noir = ligne inactive)
  uniform vec3 uAmbient;          // 0-1
  attribute vec3 baseColor;       // couleur de base du paquet, 128 = neutre
  varying vec3 vColor;
  varying vec3 vUvW;              // uv * w (mapping affine) + w

  void main() {
    vec4 clip = projectionMatrix * modelViewMatrix * vec4(position, 1.0);

    // Vertex snapping : coordonnées écran entières, le "jitter" PS1.
    vec2 half_res = uResolution * 0.5;
    clip.xy = floor(clip.xy / clip.w * half_res) / half_res * clip.w;

    // Gouraud : CC = ambiante + somme des lumières * max(0, N.L), comme
    // les matrices lumière/couleur du GTE.
    vec3 n = normalize(normalMatrix * normal);
    vec3 cc = uAmbient;
    for (int l = 0; l < 3; l++) {
      cc += uLightColor[l] * max(dot(n, uLightViewDir[l]), 0.0);
    }
    // Le GTE clampe CC à 255 avant la modulation GPU.
    cc = min(cc, vec3(1.0));
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
  uniform bool uDither;
  varying vec3 vColor;
  varying vec3 vUvW;

  // Bayer 2x2 : [0 2 / 3 1] = (2x + 3y) mod 4.
  float bayer2(vec2 p) {
    return mod(2.0 * p.x + 3.0 * p.y, 4.0);
  }

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
    color = min(color, 1.0);

    if (uDither) {
      // Dithering console : le GPU ajoute la matrice 4x4 (-4..+3) au 24
      // bits avant de tronquer en 15 bits (5 bits par canal). On rend en
      // 320x240 natif : gl_FragCoord EST le pixel console.
      vec2 p = floor(mod(gl_FragCoord.xy, 4.0));
      float bayer4 = 4.0 * bayer2(mod(p, 2.0)) + bayer2(floor(p * 0.5));
      float offset = floor(bayer4 * 0.5) - 4.0;  // la matrice exacte PS1
      vec3 c8 = color * 255.0 + offset;
      color = floor(clamp(c8, 0.0, 255.0) / 8.0) / 31.0;
    }
    gl_FragColor = vec4(color, 1.0);
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
      uDither: { value: true },
      uLightViewDir: {
        value: [new THREE.Vector3(), new THREE.Vector3(), new THREE.Vector3()],
      },
      uLightColor: {
        value: [
          new THREE.Vector3(...lighting.color.map((c) => c / 255)),
          new THREE.Vector3(),
          new THREE.Vector3(),
        ],
      },
      uAmbient: {
        value: new THREE.Vector3(...lighting.ambient.map((c) => c / 255)),
      },
    },
    side: THREE.DoubleSide,
  });
}

/** Une lumière prête pour les uniforms : direction en monde three. */
export interface ViewportLight {
  /** Vers la source, espace monde three (+Y haut). */
  towardThree: THREE.Vector3;
  /** 0-255. */
  color: [number, number, number];
}

/** À appeler chaque frame : lumières monde three → espace vue caméra. */
export function updateLightUniforms(
  materials: THREE.ShaderMaterial[],
  camera: THREE.Camera,
  lights: ViewportLight[],
) {
  const view: THREE.Vector3[] = [];
  const colors: [number, number, number][] = [];
  for (let l = 0; l < 3; l++) {
    const light = lights[l];
    view.push(
      light
        ? light.towardThree
            .clone()
            .transformDirection(camera.matrixWorldInverse)
            .normalize()
        : new THREE.Vector3(),
    );
    colors.push(light ? light.color : [0, 0, 0]);
  }
  for (const m of materials) {
    for (let l = 0; l < 3; l++) {
      (m.uniforms.uLightViewDir.value as THREE.Vector3[])[l].copy(view[l]);
      (m.uniforms.uLightColor.value as THREE.Vector3[])[l].set(
        colors[l][0] / 255,
        colors[l][1] / 255,
        colors[l][2] / 255,
      );
    }
  }
}
