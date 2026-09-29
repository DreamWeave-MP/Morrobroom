// Morrobroom's hero: OpenMW's crescent holding TrenchBroom's crate, as on Morrobroom's logo, in the
// bronze and brass they really are, rendered live with three.js.
//
// The crescent keeps OpenMW's proportions: a broad brushed face rising to a ridge, a steep inner
// bevel, two tapering points, in anisotropic bronze. The crate is TrenchBroom's icon: varnished
// planks, a knurled rim, a carved emblem, dark slots at its foot and the editor's grid across every
// face. Their colour, normal, roughness, metalness and cavity maps are baked here on canvases, and a
// room of light panels, filtered into an environment map, gives the metal something to reflect.
//
// The crate turns on its own axis while the mark sways. The pointer is a lamp the mark leans
// towards, and a click spins the crate. Every few seconds a band of light climbs the crate and its
// grid glows as it passes, like a lightmap bake. Beneath lies TrenchBroom's grid as a floor that
// reflects the mark, and embers rise through the scene.
//
// The scene renders to a half-float target; a bright pass and four blur passes make the bloom, and
// the composite applies ACES tone mapping and dithering. Colours come from the site's CSS tokens.
// The mark stands beside the hero's text, measured at every layout, or above it on a phone, where
// sass/brand.sass leaves it room. Nothing runs until the hero is on screen or while the tab is
// hidden, and the resolution drops if frames run slow. Under prefers-reduced-motion one frame is
// drawn. Until the first frame, and without WebGL, a still of the mark stands in its place.

import * as THREE from './vendor/three.module.min.js';

// OpenMW's crescent, measured from its launcher logo: the outer circle has radius 1, and the inner
// circle is 0.52 to its right with radius 1.113. The crescent is the part of the outer disc outside
// the inner one; its points are where the circles cross.
const CRESCENT = { offset: 0.5217, inner: 1.113, ridge: 0.6, height: 0.17, edge: 0.014, rows: 180 };
// TrenchBroom's crate, placed where the logo has it: in the crescent's opening, a little forward.
export const CRATE = { size: 0.78, x: 0.1, y: -0.08, z: 0.2, tilt: 0.55 };
// The mark spans x from -1 to 0.61 and y from -1 to 1; this is its middle. Turning, the crate
// reaches 0.14 further right.
export const MARK_CENTER = new THREE.Vector2(-0.195, 0);
const MARK_ASPECT = 1.75 / 2;
const EMBERS = 240;

const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;

function cssColor(name, fallback) {
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const color = new THREE.Color(fallback);
  if (raw) {
    try { color.setStyle(raw); } catch { /* an unparsable token keeps the fallback */ }
  }
  return color;
}

// Texture baking ---------------------------------------------------------------------------------

function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function valueNoise(seed) {
  const next = random(seed);
  const table = new Float32Array(256 * 256);
  for (let i = 0; i < table.length; i++) table[i] = next();
  return (x, y) => {
    const xi = Math.floor(x);
    const yi = Math.floor(y);
    const fx = x - xi;
    const fy = y - yi;
    const u = fx * fx * (3 - 2 * fx);
    const v = fy * fy * (3 - 2 * fy);
    const x0 = xi & 255;
    const y0 = yi & 255;
    const x1 = (x0 + 1) & 255;
    const y1 = (y0 + 1) & 255;
    const a = table[y0 * 256 + x0];
    const b = table[y0 * 256 + x1];
    const c = table[y1 * 256 + x0];
    const d = table[y1 * 256 + x1];
    return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
  };
}

function fbm(noise, x, y, octaves) {
  let sum = 0;
  let amplitude = 0.5;
  let frequency = 1;
  let norm = 0;
  for (let i = 0; i < octaves; i++) {
    sum += amplitude * noise(x * frequency, y * frequency);
    norm += amplitude;
    amplitude *= 0.5;
    frequency *= 2.03;
  }
  return sum / norm;
}

function boxBlur(source, width, height, radius) {
  const span = radius * 2 + 1;
  const rows = new Float32Array(source.length);
  const out = new Float32Array(source.length);
  for (let y = 0; y < height; y++) {
    const row = y * width;
    let sum = 0;
    for (let x = -radius; x <= radius; x++) sum += source[row + Math.min(width - 1, Math.max(0, x))];
    for (let x = 0; x < width; x++) {
      rows[row + x] = sum / span;
      sum += source[row + Math.min(width - 1, x + radius + 1)] - source[row + Math.max(0, x - radius)];
    }
  }
  for (let x = 0; x < width; x++) {
    let sum = 0;
    for (let y = -radius; y <= radius; y++) sum += rows[Math.min(height - 1, Math.max(0, y)) * width + x];
    for (let y = 0; y < height; y++) {
      out[y * width + x] = sum / span;
      sum += rows[Math.min(height - 1, y + radius + 1) * width + x] - rows[Math.max(0, y - radius) * width + x];
    }
  }
  return out;
}

const clamp01 = (value) => Math.min(1, Math.max(0, value));
const byte = (value) => Math.round(clamp01(value) * 255);

// A height field's slopes as a tangent-space normal map, +v up, as three.js reads one.
function normalPixels(height, width, rows, strength) {
  const data = new Uint8ClampedArray(width * rows * 4);
  for (let y = 0; y < rows; y++) {
    const up = Math.max(0, y - 1) * width;
    const down = Math.min(rows - 1, y + 1) * width;
    for (let x = 0; x < width; x++) {
      const i = y * width + x;
      const dx = (height[y * width + Math.min(width - 1, x + 1)] - height[y * width + Math.max(0, x - 1)]) * strength;
      const dy = (height[down + x] - height[up + x]) * strength;
      const inverse = 1 / Math.hypot(dx, dy, 1);
      data[i * 4] = byte(-dx * inverse * 0.5 + 0.5);
      data[i * 4 + 1] = byte(dy * inverse * 0.5 + 0.5);
      data[i * 4 + 2] = byte(inverse * 0.5 + 0.5);
      data[i * 4 + 3] = 255;
    }
  }
  return data;
}

function canvasTexture(data, width, rows, colorSpace, anisotropy) {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = rows;
  canvas.getContext('2d').putImageData(new ImageData(data, width, rows), 0, 0);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = colorSpace;
  texture.anisotropy = anisotropy;
  return texture;
}

// The crescent's surface: u runs along the arc, v across it from the outer edge (v 0) to the inner.
// Brushing follows the arc; mottling, pits and grime at the edges come from OpenMW's own bronze.
function crescentMaps(width, rows, anisotropy) {
  const count = width * rows;
  const height = new Float32Array(count);
  const color = new Uint8ClampedArray(count * 4);
  const surface = new Uint8ClampedArray(count * 4);
  const brush = valueNoise(3);
  const mottle = valueNoise(17);
  const pits = valueNoise(41);
  for (let y = 0; y < rows; y++) {
    const across = 1 - (y + 0.5) / rows;
    const bevel = across > CRESCENT.ridge ? 1 : 0;
    const rim = Math.max(1 - across / 0.05, 1 - (1 - across) / 0.05, 0);
    for (let x = 0; x < width; x++) {
      const along = (x + 0.5) / width;
      const i = y * width + x;
      const brushed = fbm(brush, along * 5, across * 170, 4);
      const patch = fbm(mottle, along * 34, across * 7 + bevel * 3, 5);
      const pit = Math.max(0, fbm(pits, along * 260, across * 70, 2) - 0.67) * 4;
      const worn = clamp01(pit + rim * 0.8);
      height[i] = brushed * 0.55 + patch * 0.25 - pit * 0.5;
      const light = 0.5 + (patch - 0.5) * 1.25 + (brushed - 0.5) * 0.4;
      let r = 0.44 + 0.3 * light;
      let g = 0.37 + 0.28 * light;
      let b = 0.19 + 0.19 * light;
      if (bevel) {
        r *= 0.5;
        g *= 0.44;
        b *= 0.36;
      }
      r = r * (1 - worn * 0.55) + 0.1 * worn * 0.55;
      g = g * (1 - worn * 0.55) + 0.075 * worn * 0.55;
      b = b * (1 - worn * 0.55) + 0.04 * worn * 0.55;
      color[i * 4] = byte(r);
      color[i * 4 + 1] = byte(g);
      color[i * 4 + 2] = byte(b);
      color[i * 4 + 3] = 255;
      surface[i * 4] = byte(1 - pit * 0.35);
      surface[i * 4 + 1] = byte(0.26 + 0.16 * patch + 0.1 * (1 - brushed) + bevel * 0.12 + worn * 0.3);
      surface[i * 4 + 2] = byte(1 - worn * 0.45);
      surface[i * 4 + 3] = 255;
    }
  }
  return {
    map: canvasTexture(color, width, rows, THREE.SRGBColorSpace, anisotropy),
    surface: canvasTexture(surface, width, rows, THREE.NoColorSpace, anisotropy),
    normal: canvasTexture(normalPixels(height, width, rows, 2.2), width, rows, THREE.NoColorSpace, anisotropy),
  };
}

// TrenchBroom's emblem, carved into each face: a sun wheel, a runic T and the name.
function emblemMask(size, side) {
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext('2d');
  context.fillStyle = '#000';
  context.fillRect(0, 0, size, size);
  context.fillStyle = '#fff';
  context.strokeStyle = '#fff';
  const middle = side ? 0.43 : 0.5;
  context.save();
  context.scale(size, size);
  context.lineWidth = 0.011;
  context.strokeRect(0.165, 0.165, 0.67, side ? 0.555 : 0.67);
  const sunX = 0.345;
  const sunY = middle - 0.03;
  context.lineWidth = 0.026;
  context.beginPath();
  context.arc(sunX, sunY, 0.068, 0, Math.PI * 2);
  context.stroke();
  context.beginPath();
  context.arc(sunX, sunY, 0.028, 0, Math.PI * 2);
  context.fill();
  for (let k = 0; k < 8; k++) {
    const angle = k * Math.PI / 4 + Math.PI / 8;
    context.beginPath();
    context.moveTo(sunX + Math.cos(angle - 0.2) * 0.092, sunY + Math.sin(angle - 0.2) * 0.092);
    context.quadraticCurveTo(sunX + Math.cos(angle - 0.06) * 0.15, sunY + Math.sin(angle - 0.06) * 0.15, sunX + Math.cos(angle + 0.14) * 0.17, sunY + Math.sin(angle + 0.14) * 0.17);
    context.lineTo(sunX + Math.cos(angle + 0.2) * 0.092, sunY + Math.sin(angle + 0.2) * 0.092);
    context.closePath();
    context.fill();
  }
  const stem = 0.665;
  context.fillRect(0.55, middle - 0.17, 0.25, 0.052);
  context.fillRect(stem - 0.03, middle - 0.17, 0.06, 0.33);
  context.fillRect(stem - 0.078, middle + 0.118, 0.156, 0.044);
  context.fillRect(0.55, middle - 0.125, 0.034, 0.075);
  context.fillRect(0.766, middle - 0.125, 0.034, 0.075);
  context.fillRect(stem - 0.1, middle - 0.02, 0.2, 0.03);
  context.restore();
  context.font = `bold ${Math.round(size * 0.04)}px Georgia, "Times New Roman", serif`;
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  context.fillText('TRENCHBROOM', sunX * size, (sunY + 0.215) * size);
  const pixels = context.getImageData(0, 0, size, size).data;
  const mask = new Float32Array(size * size);
  for (let i = 0; i < mask.length; i++) mask[i] = pixels[i * 4] / 255;
  return mask;
}

function gridLine(fraction, size) {
  const nearest = Math.round(fraction * 4) / 4;
  const weight = nearest === 0 || nearest === 1 ? 0.45 : 1;
  return clamp01(1.25 - Math.abs(fraction - nearest) * size * 0.9) * weight;
}

// One face of the crate. The sides have the foot band with its slots; the top and bottom do not.
function crateMaps(size, side, anisotropy) {
  const count = size * size;
  const height = new Float32Array(count);
  const albedo = new Float32Array(count * 3);
  const rough = new Float32Array(count);
  const metal = new Float32Array(count);
  const glow = new Float32Array(count);
  const grain = valueNoise(11);
  const fibre = valueNoise(29);
  const wear = valueNoise(47);
  const plankTone = random(7);
  const tones = Array.from({ length: 8 }, () => plankTone() - 0.5);
  const carve = boxBlur(emblemMask(size, side), size, size, Math.max(1, Math.round(size / 340)));
  const band = 0.1;
  const footTop = 0.765;
  for (let y = 0; y < size; y++) {
    const fy = (y + 0.5) / size;
    for (let x = 0; x < size; x++) {
      const fx = (x + 0.5) / size;
      const i = y * size + x;
      const edge = Math.min(fx, fy, 1 - fx, 1 - fy);
      const scuff = fbm(wear, fx * 9, fy * 9, 3);
      let h;
      let r;
      let g;
      let b;
      let ro;
      let me;
      if (edge < band) {
        const across = edge / band;
        let along;
        if (edge === fy) along = fx;
        else if (edge === 1 - fy) along = 1 - fx;
        else if (edge === fx) along = 1 - fy;
        else along = fy;
        const ridge = 0.5 + 0.5 * Math.cos((along * 36 + across * 1.5) * Math.PI * 2);
        const bulge = Math.sin(Math.PI * Math.min(1, across * 1.04));
        const groove = across > 0.9 ? (across - 0.9) * 10 : 0;
        h = 0.5 + 0.24 * bulge + 0.12 * ridge * ridge * bulge - groove * 0.3;
        const lit = clamp01(0.25 + 0.75 * ridge * bulge - groove * 0.4 + (scuff - 0.5) * 0.3);
        r = 0.14 + 0.26 * lit;
        g = 0.085 + 0.16 * lit;
        b = 0.03 + 0.06 * lit;
        ro = 0.6 + 0.2 * (1 - ridge);
        me = 0.12;
      } else if (side && fy > footTop) {
        const inSlot = (fy > 0.79 && fy < 0.875) && [[0.17, 0.33], [0.42, 0.58], [0.67, 0.83]].some(([from, to]) => fx > from && fx < to);
        const seam = fy < footTop + 0.008 ? 1 : 0;
        const tone = fbm(grain, fx * 5, fy * 120, 3);
        h = inSlot ? 0.02 : 0.45 + (tone - 0.5) * 0.06 - seam * 0.2;
        r = inSlot ? 0.025 : 0.28 + 0.12 * tone;
        g = inSlot ? 0.016 : 0.18 + 0.08 * tone;
        b = inSlot ? 0.008 : 0.06 + 0.03 * tone;
        ro = inSlot ? 0.95 : 0.62;
        me = 0;
      } else {
        const panelBottom = side ? footTop : 1 - band;
        const across = (fy - band) / (panelBottom - band);
        const plank = Math.min(4, Math.floor(across * 5));
        const within = across * 5 - plank;
        const seam = Math.max(0, 1 - Math.min(within, 1 - within) / 0.04);
        const figure = fbm(grain, fx * 3.5 + plank * 3.1, fy * 150 + plank * 11, 4);
        const streak = fbm(fibre, fx * 36, fy * 430, 2);
        h = 0.5 + (figure - 0.5) * 0.08 + (streak - 0.5) * 0.03 - seam * 0.22;
        const tone = clamp01(0.52 + (figure - 0.5) * 1.3 + (streak - 0.5) * 0.4 + tones[plank] * 0.35 - seam * 0.7);
        r = 0.3 + 0.32 * tone;
        g = 0.2 + 0.24 * tone;
        b = 0.05 + 0.08 * tone;
        ro = 0.74 + (streak - 0.5) * 0.2;
        me = 0;
      }
      const cut = carve[i];
      if (cut > 0) {
        h -= 0.42 * cut;
        r += (0.12 - r) * cut * 0.9;
        g += (0.066 - g) * cut * 0.9;
        b += (0.02 - b) * cut * 0.9;
        ro += (0.82 - ro) * cut;
        me *= 1 - cut;
      }
      const line = Math.max(gridLine(fx, size), gridLine(fy, size));
      glow[i] = line;
      r += (0.93 - r) * line * 0.3;
      g += (0.84 - g) * line * 0.3;
      b += (0.52 - b) * line * 0.3;
      height[i] = h;
      albedo[i * 3] = r;
      albedo[i * 3 + 1] = g;
      albedo[i * 3 + 2] = b;
      rough[i] = ro + (scuff - 0.5) * 0.12;
      metal[i] = me;
    }
  }
  const cavity = boxBlur(height, size, size, Math.max(2, Math.round(size / 96)));
  const color = new Uint8ClampedArray(count * 4);
  const surface = new Uint8ClampedArray(count * 4);
  const emission = new Uint8ClampedArray(count * 4);
  for (let i = 0; i < count; i++) {
    const occlusion = clamp01(1 - Math.max(0, cavity[i] - height[i]) * 4.5);
    color[i * 4] = byte(albedo[i * 3] * (0.55 + 0.45 * occlusion));
    color[i * 4 + 1] = byte(albedo[i * 3 + 1] * (0.55 + 0.45 * occlusion));
    color[i * 4 + 2] = byte(albedo[i * 3 + 2] * (0.55 + 0.45 * occlusion));
    color[i * 4 + 3] = 255;
    surface[i * 4] = byte(occlusion);
    surface[i * 4 + 1] = byte(rough[i]);
    surface[i * 4 + 2] = byte(metal[i]);
    surface[i * 4 + 3] = 255;
    emission[i * 4] = byte(glow[i]);
    emission[i * 4 + 1] = byte(glow[i]);
    emission[i * 4 + 2] = byte(glow[i]);
    emission[i * 4 + 3] = 255;
  }
  return {
    map: canvasTexture(color, size, size, THREE.SRGBColorSpace, anisotropy),
    surface: canvasTexture(surface, size, size, THREE.NoColorSpace, anisotropy),
    normal: canvasTexture(normalPixels(height, size, size, size / 22), size, size, THREE.NoColorSpace, anisotropy),
    glow: canvasTexture(emission, size, size, THREE.SRGBColorSpace, anisotropy),
  };
}

// Geometry ---------------------------------------------------------------------------------------

// The crescent as a swept blade: at each step along the arc, a cross-section from the outer edge
// up a broad face to the ridge and down a steep bevel to the inner edge, mirrored behind. Each
// facet has its own vertices, so the ridge stays sharp; the ends close on the points.
function crescentGeometry() {
  const { offset, inner, ridge, rows } = CRESCENT;
  const tipX = (offset * offset + 1 - inner * inner) / (2 * offset);
  const tipAngle = Math.atan2(Math.sqrt(1 - tipX * tipX), tipX);
  const sweep = Math.PI * 2 - tipAngle * 2;
  const widest = 1 - (inner - offset);
  const section = (along) => {
    const angle = tipAngle + sweep * along;
    const cos = Math.cos(angle);
    const sin = Math.sin(angle);
    const toward = offset * cos;
    const reach = toward + Math.sqrt(toward * toward - offset * offset + inner * inner);
    const width = 1 - reach;
    const taper = Math.pow(Math.max(width, 0) / widest, 0.72);
    return { cos, sin, reach, width, top: Math.max(CRESCENT.edge * Math.sqrt(taper), CRESCENT.height * taper), edge: CRESCENT.edge * Math.sqrt(taper) };
  };
  let tipLimit = 0;
  for (let low = 0, high = 0.5, i = 0; i < 40; i++) {
    tipLimit = (low + high) / 2;
    if (section(tipLimit).width < 0.018) low = tipLimit;
    else high = tipLimit;
  }
  const steps = [];
  for (let i = 0; i <= rows; i++) steps.push(tipLimit + (1 - tipLimit * 2) * (0.5 - 0.5 * Math.cos(Math.PI * i / rows)));
  const sections = steps.map(section);

  const positions = [];
  const uvs = [];
  const indices = [];
  const point = (s, across, z) => {
    const radius = 1 - across * s.width;
    return [s.cos * radius, s.sin * radius, z];
  };
  const faceZ = (s, across) => (across <= ridge
    ? s.edge + (s.top - s.edge) * Math.sin(Math.PI / 2 * across / ridge)
    : s.top - (s.top - s.edge) * (across - ridge) / (1 - ridge));
  function patch(acrossValues, zFor, front) {
    const base = positions.length / 3;
    const columns = acrossValues.length;
    sections.forEach((s, i) => {
      for (const across of acrossValues) {
        positions.push(...point(s, across, zFor(s, across)));
        uvs.push(steps[i], across);
      }
    });
    for (let i = 0; i < sections.length - 1; i++) {
      for (let j = 0; j < columns - 1; j++) {
        const a = base + i * columns + j;
        const b = a + columns;
        if (front) indices.push(a, b, a + 1, b, b + 1, a + 1);
        else indices.push(a, a + 1, b, b, a + 1, b + 1);
      }
    }
  }
  const span = (from, to, count) => Array.from({ length: count + 1 }, (_, k) => from + (to - from) * k / count);
  patch(span(0, ridge, 12), faceZ, true);
  patch(span(ridge, 1, 6), faceZ, true);
  patch(span(0, ridge, 12), (s, across) => -faceZ(s, across), false);
  patch(span(ridge, 1, 6), (s, across) => -faceZ(s, across), false);
  // The walls: two rows of vertices each, front edge then back edge.
  function wall(across, outward) {
    const base = positions.length / 3;
    sections.forEach((s, i) => {
      positions.push(...point(s, across, s.edge), ...point(s, across, -s.edge));
      uvs.push(steps[i], across, steps[i], across + (across < 0.5 ? 0.004 : -0.004));
    });
    for (let i = 0; i < sections.length - 1; i++) {
      const a = base + i * 2;
      const b = a + 2;
      if (outward) indices.push(a, a + 1, b, a + 1, b + 1, b);
      else indices.push(a, b, a + 1, a + 1, b, b + 1);
    }
  }
  wall(0, true);
  wall(1, false);
  // The points: each end's cross-section closes on the tip, where the two circles cross.
  const loop = [
    ...span(0, ridge, 12).map((across) => [across, 1]),
    ...span(ridge, 1, 6).slice(1).map((across) => [across, 1]),
    ...span(1, ridge, 6).map((across) => [across, -1]),
    ...span(ridge, 0, 12).slice(1).map((across) => [across, -1]),
  ];
  for (const [index, sign] of [[0, -1], [sections.length - 1, 1]]) {
    const s = sections[index];
    const tip = sign < 0 ? section(0) : section(1);
    const base = positions.length / 3;
    positions.push(tip.cos, tip.sin, 0);
    uvs.push(sign < 0 ? 0 : 1, 0.5);
    for (const [across, side] of loop) {
      positions.push(...point(s, across, side * faceZ(s, across)));
      uvs.push(steps[index], across);
    }
    // Wind the fan so it faces away from the crescent, towards its point.
    const at = (k) => new THREE.Vector3(positions[(base + k) * 3], positions[(base + k) * 3 + 1], positions[(base + k) * 3 + 2]);
    const middle = new THREE.Vector3(...point(s, 0.5, 0));
    const outward = at(0).sub(middle);
    const facing = at(2).sub(at(0)).cross(at(1).sub(at(0))).dot(outward) > 0;
    for (let k = 0; k < loop.length; k++) {
      const a = base + 1 + k;
      const b = base + 1 + ((k + 1) % loop.length);
      if (facing) indices.push(base, b, a);
      else indices.push(base, a, b);
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  const normals = geometry.getAttribute('normal');
  for (let i = 0; i < normals.count; i++) {
    if (Math.hypot(normals.getX(i), normals.getY(i), normals.getZ(i)) < 1e-6) normals.setXYZ(i, 0, 0, positions[i * 3 + 2] < 0 ? -1 : 1);
  }
  geometry.computeTangents();
  const tangents = geometry.getAttribute('tangent');
  for (let i = 0; i < tangents.count; i++) {
    if (!(Math.hypot(tangents.getX(i), tangents.getY(i), tangents.getZ(i)) > 1e-6)) tangents.setXYZW(i, 1, 0, 0, 1);
  }
  return geometry;
}

// Light --------------------------------------------------------------------------------------------

// A dark room with softboxes: a warm key above left, cool and accent rim strips behind, a long
// horizon strip, and bronze bounce from the floor. Filtered once into an environment map.
export function environment(renderer, accent) {
  const scene = new THREE.Scene();
  const disposables = [];
  const add = (geometry, color, position) => {
    const material = new THREE.MeshBasicMaterial({ color, side: THREE.DoubleSide });
    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.set(...position);
    mesh.lookAt(0, 0, 0);
    scene.add(mesh);
    disposables.push(geometry, material);
  };
  const room = new THREE.Mesh(new THREE.BoxGeometry(26, 16, 26), new THREE.MeshBasicMaterial({ color: new THREE.Color(0.05, 0.045, 0.04), side: THREE.BackSide }));
  scene.add(room);
  disposables.push(room.geometry, room.material);
  add(new THREE.PlaneGeometry(12, 8), new THREE.Color(1.0, 0.86, 0.66).multiplyScalar(4), [-7, 6, 7]);
  add(new THREE.PlaneGeometry(8, 6), new THREE.Color(0.9, 0.92, 1.0).multiplyScalar(2.2), [7, 3, 8]);
  add(new THREE.PlaneGeometry(12, 0.7), new THREE.Color(0.62, 0.78, 1.0).multiplyScalar(6), [8, 2.5, -8]);
  add(new THREE.PlaneGeometry(12, 0.6), accent.clone().multiplyScalar(5), [-9, 1, -6]);
  add(new THREE.PlaneGeometry(18, 0.4), new THREE.Color(1.0, 0.8, 0.55).multiplyScalar(3), [0, -0.4, -12]);
  add(new THREE.PlaneGeometry(24, 24), new THREE.Color(0.42, 0.28, 0.12).multiplyScalar(0.35), [0, -7.5, 0]);
  const generator = new THREE.PMREMGenerator(renderer);
  const target = generator.fromScene(scene, 0.035);
  generator.dispose();
  for (const item of disposables) item.dispose();
  return target;
}

// Shaders ----------------------------------------------------------------------------------------

const FULLSCREEN_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = vec4(position.xy, 0.0, 1.0);
  }
`;

const NOISE = /* glsl */ `
  float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
  float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
  }
  float fbm(vec2 p) {
    float sum = 0.0;
    float amplitude = 0.5;
    for (int i = 0; i < 5; i++) {
      sum += amplitude * noise(p);
      p = p * 2.03 + vec2(17.1, 9.2);
      amplitude *= 0.5;
    }
    return sum;
  }
`;

// The backdrop: the hero's own gradient, a bronze glow behind the mark, slow haze, and shafts of
// the key light falling from above left.
const SKY_FRAGMENT = /* glsl */ `
  uniform vec3 uTop;
  uniform vec3 uBottom;
  uniform vec3 uGlow;
  uniform vec3 uAccent;
  uniform vec2 uCenter;
  uniform vec2 uResolution;
  uniform float uRadius;
  uniform float uTime;
  varying vec2 vUv;
  ${NOISE}
  void main() {
    vec3 color = mix(uBottom, uTop, vUv.y);
    vec2 aspect = vec2(uResolution.x / max(uResolution.y, 1.0), 1.0);
    vec2 d = (vUv - uCenter) * aspect / max(uRadius, 0.001);
    float r2 = dot(d, d);
    float glow = exp(-r2 * 0.55);
    float haze = fbm(d * 1.4 + vec2(uTime * 0.012, -uTime * 0.02));
    vec2 shaftAxis = vec2(0.42, 0.91);
    float shafts = fbm(vec2(dot(d, vec2(shaftAxis.y, -shaftAxis.x)) * 2.2 + uTime * 0.015, uTime * 0.01));
    shafts = smoothstep(0.45, 0.85, shafts) * exp(-r2 * 0.18) * smoothstep(-1.5, 0.8, dot(d, -shaftAxis));
    color += uGlow * glow * (0.55 + 0.45 * haze);
    color += uAccent * exp(-r2 * 0.12) * 0.035 * haze;
    color += vec3(1.0, 0.82, 0.58) * shafts * 0.03;
    gl_FragColor = vec4(color, 1.0);
  }
`;

// TrenchBroom's grid as a floor under the mark: minor lines every quarter unit, major every unit,
// fading with distance, with the mark's reflection and a soft contact shadow. Premultiplied.
const FLOOR_VERTEX = /* glsl */ `
  varying vec3 vWorld;
  void main() {
    vec4 world = modelMatrix * vec4(position, 1.0);
    vWorld = world.xyz;
    gl_Position = projectionMatrix * viewMatrix * world;
  }
`;

const FLOOR_FRAGMENT = /* glsl */ `
  uniform sampler2D tReflect;
  uniform vec2 uResolution;
  uniform vec3 uCenter;
  uniform float uScale;
  uniform vec3 uMinor;
  uniform vec3 uMajor;
  uniform float uOpacity;
  uniform float uSweep;
  varying vec3 vWorld;
  float gridLines(vec2 p) {
    vec2 w = max(fwidth(p), vec2(1e-4));
    vec2 l = 1.0 - smoothstep(vec2(0.0), w * 1.2, abs(fract(p - 0.5) - 0.5));
    return max(l.x, l.y) * clamp(1.4 - max(w.x, w.y) * 3.0, 0.0, 1.0);
  }
  void main() {
    vec2 p = (vWorld.xz - uCenter.xz) / max(uScale, 1e-4);
    float r2 = dot(p, p);
    float fade = exp(-r2 * 0.5);
    float minor = gridLines(p * 4.0);
    float major = gridLines(p);
    float ringOffset = sqrt(r2) - uSweep * 2.2;
    float ring = exp(-ringOffset * ringOffset * 18.0) * step(0.001, uSweep) * (1.0 - uSweep);
    vec3 color = (uMinor * minor * 0.55 + uMajor * major) * fade * (1.0 + ring * 3.0);
    vec2 screen = gl_FragCoord.xy / uResolution;
    vec2 texel = 1.5 / uResolution;
    vec3 reflection = texture2D(tReflect, screen).rgb * 0.4;
    reflection += texture2D(tReflect, screen + vec2(texel.x, 0.0)).rgb * 0.15;
    reflection += texture2D(tReflect, screen - vec2(texel.x, 0.0)).rgb * 0.15;
    reflection += texture2D(tReflect, screen + vec2(0.0, texel.y)).rgb * 0.15;
    reflection += texture2D(tReflect, screen - vec2(0.0, texel.y)).rgb * 0.15;
    color += reflection * exp(-r2 * 0.6) * 0.5;
    float shadow = exp(-r2 * 2.4) * 0.55;
    gl_FragColor = vec4(color, shadow) * uOpacity;
  }
`;

// Embers rise from the floor, drift and flicker out; their paths are computed on the GPU.
const EMBER_VERTEX = /* glsl */ `
  uniform float uTime;
  uniform float uScale;
  uniform float uPixel;
  uniform vec3 uCenter;
  attribute vec4 aSeed;
  varying float vAlpha;
  varying float vHeat;
  void main() {
    float life = 5.0 + aSeed.x * 6.0;
    float age = mod(uTime * (0.55 + aSeed.y * 0.5) + aSeed.z * life, life);
    float k = age / life;
    vec3 p;
    p.x = (aSeed.w - 0.5) * 3.4 + sin(uTime * 0.6 + aSeed.x * 23.0) * 0.18 * k;
    p.y = -1.15 + k * 3.1;
    p.z = (fract(aSeed.x * 7.13 + aSeed.w * 3.7) - 0.5) * 1.8;
    vec4 view = viewMatrix * vec4(uCenter + p * uScale, 1.0);
    gl_Position = projectionMatrix * view;
    vAlpha = smoothstep(0.0, 0.12, k) * (1.0 - smoothstep(0.62, 1.0, k)) * (0.35 + 0.65 * fract(aSeed.y * 13.7));
    vHeat = 1.0 - k;
    float flicker = 0.75 + 0.25 * sin(uTime * (6.0 + aSeed.w * 7.0) + aSeed.z * 40.0);
    gl_PointSize = uPixel * 0.014 * uScale * (0.6 + aSeed.w * 1.6) * flicker / max(-view.z, 0.1);
  }
`;

const EMBER_FRAGMENT = /* glsl */ `
  uniform float uIntensity;
  varying float vAlpha;
  varying float vHeat;
  void main() {
    vec2 d = gl_PointCoord - 0.5;
    float falloff = exp(-dot(d, d) * 22.0);
    vec3 color = mix(vec3(1.0, 0.32, 0.08), vec3(1.0, 0.8, 0.45), vHeat);
    gl_FragColor = vec4(color * falloff * vAlpha * uIntensity, 1.0);
  }
`;

// Any NaN or infinity a driver produces is zeroed and bright values capped before the bloom, which
// would otherwise smear a single bad pixel into a black square.
const SCRUB = /* glsl */ `
  vec3 scrub(vec3 c) {
    if (any(isnan(c)) || any(isinf(c)) || c.r != c.r || c.g != c.g || c.b != c.b) return vec3(0.0);
    return clamp(c, 0.0, 64.0);
  }
`;

const BRIGHT_FRAGMENT = /* glsl */ `
  uniform sampler2D tInput;
  uniform float uThreshold;
  varying vec2 vUv;
  ${SCRUB}
  void main() {
    vec3 c = scrub(texture2D(tInput, vUv).rgb);
    float luma = dot(c, vec3(0.2126, 0.7152, 0.0722));
    gl_FragColor = vec4(c * smoothstep(uThreshold, uThreshold + 0.7, luma), 1.0);
  }
`;

const BLUR_FRAGMENT = /* glsl */ `
  uniform sampler2D tInput;
  uniform vec2 uDirection;
  varying vec2 vUv;
  void main() {
    vec3 sum = texture2D(tInput, vUv).rgb * 0.2270270270;
    sum += texture2D(tInput, vUv + uDirection * 1.3846153846).rgb * 0.3162162162;
    sum += texture2D(tInput, vUv - uDirection * 1.3846153846).rgb * 0.3162162162;
    sum += texture2D(tInput, vUv + uDirection * 3.2307692308).rgb * 0.0702702703;
    sum += texture2D(tInput, vUv - uDirection * 3.2307692308).rgb * 0.0702702703;
    gl_FragColor = vec4(sum, 1.0);
  }
`;

const COMPOSITE_FRAGMENT = /* glsl */ `
  uniform sampler2D tScene;
  uniform sampler2D tBloomNear;
  uniform sampler2D tBloomFar;
  uniform float uTime;
  varying vec2 vUv;
  vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
  }
  float dither(vec2 p) {
    return fract(sin(dot(p + fract(uTime), vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
  }
  ${SCRUB}
  void main() {
    vec3 color = scrub(texture2D(tScene, vUv).rgb);
    color += scrub(texture2D(tBloomNear, vUv).rgb) * 0.7 + scrub(texture2D(tBloomFar, vUv).rgb) * 0.55;
    color = aces(color * 0.92);
    color = pow(max(color, vec3(0.0)), vec3(1.0 / 2.2));
    color += dither(gl_FragCoord.xy) / 255.0;
    gl_FragColor = vec4(color, 1.0);
  }
`;

function fullscreenMaterial(fragmentShader, uniforms) {
  return new THREE.ShaderMaterial({ vertexShader: FULLSCREEN_VERTEX, fragmentShader, uniforms, depthTest: false, depthWrite: false });
}

// The crate's grid brightens where the bake band passes, and the band warms the wood a little.
function withBake(material, bake) {
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, bake);
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nvarying vec3 vBakeWorld;')
      .replace('#include <project_vertex>', '#include <project_vertex>\nvBakeWorld = (modelMatrix * vec4(transformed, 1.0)).xyz;');
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', '#include <common>\nvarying vec3 vBakeWorld;\nuniform float uBakeY;\nuniform float uBakeWidth;\nuniform float uBakeGain;')
      .replace('#include <emissivemap_fragment>', `#include <emissivemap_fragment>
        float bakeBand = exp(-pow2((vBakeWorld.y - uBakeY) / max(uBakeWidth, 1e-4))) * uBakeGain;
        totalEmissiveRadiance *= 1.0 + bakeBand * 7.0;
        totalEmissiveRadiance += vec3(1.0, 0.7, 0.36) * bakeBand * 0.22;`);
  };
  material.customProgramCacheKey = () => 'morrobroom-bake';
  return material;
}

// Layout -------------------------------------------------------------------------------------------

// Where the hero's words and controls are, so the mark can stand clear of them.
function textRects(text) {
  const rects = [];
  const range = document.createRange();
  const walker = document.createTreeWalker(text, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => (node.nodeValue.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
  });
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    range.selectNodeContents(node);
    for (const rect of range.getClientRects()) rects.push(rect);
  }
  for (const element of text.querySelectorAll('a, button, input, select, img, svg, .dw-command, .dw-badge')) rects.push(element.getBoundingClientRect());
  return rects.filter((rect) => rect.width > 0 && rect.height > 0);
}

// The largest square clear of the text: beside all of it, beside the title rows above the summary,
// or above it all where the stylesheet has left room on a phone. Returns it relative to the art.
function placement(root) {
  const hero = root.closest('.dw-hero') || root.parentElement;
  const box = root.getBoundingClientRect();
  const text = hero.querySelector('.dw-hero__text') || hero.querySelector('.dw-shell');
  const strip = hero.querySelector('.dw-strip');
  // The content's edges: the facts strip spans them; the shell's own box includes its gutters.
  const shellElement = hero.querySelector('.dw-hero__grid') || hero.querySelector('.dw-shell') || hero;
  const shellStyle = getComputedStyle(shellElement);
  const shellBox = shellElement.getBoundingClientRect();
  const shell = strip ? strip.getBoundingClientRect() : { left: shellBox.left + parseFloat(shellStyle.paddingLeft), right: shellBox.right - parseFloat(shellStyle.paddingRight) };
  const summary = hero.querySelector('.dw-hero__summary');
  const floor = strip ? strip.getBoundingClientRect().top : box.bottom - 24;
  const rects = text ? textRects(text) : [];
  if (!rects.length) return { x: box.width * 0.75, y: box.height * 0.45, size: Math.min(box.width * 0.3, box.height * 0.7), above: false };
  const gap = 32;
  const right = Math.max(...rects.map((rect) => rect.right));
  const top = Math.min(...rects.map((rect) => rect.top));
  const summaryTop = summary ? summary.getBoundingClientRect().top : floor;
  const headRects = rects.filter((rect) => rect.bottom <= summaryTop + 1);
  const headRight = headRects.length ? Math.max(...headRects.map((rect) => rect.right)) : right;
  const candidates = [
    { x0: right + gap, x1: shell.right, y0: box.top + 18, y1: floor - 18, above: false },
    { x0: headRight + gap, x1: shell.right, y0: box.top + 12, y1: summaryTop - 12, above: false },
    { x0: shell.left, x1: shell.right, y0: box.top + 10, y1: top - 14, above: true },
  ].map((region) => {
    const width = region.x1 - region.x0;
    const height = region.y1 - region.y0;
    const size = Math.max(0, Math.min(height, width / MARK_ASPECT));
    return { ...region, size };
  });
  const best = candidates.reduce((a, b) => (b.size > a.size ? b : a));
  const size = Math.min(best.size * 0.84, 400);
  const markWidth = size * MARK_ASPECT;
  const x = best.above ? (best.x0 + best.x1) / 2 : Math.min(best.x1 - markWidth / 2, (best.x0 + best.x1) / 2 + (best.x1 - best.x0 - markWidth) * 0.25);
  return { x: x - box.left, y: (best.y0 + best.y1) / 2 - box.top, size, above: best.above };
}

// The crescent, and the crate on its own pivot: the crate follows the mark's sway but not its
// turn, leaning back to show its top, as on the icon.
export function buildMark(anisotropy, small) {
  const bronze = crescentMaps(small ? 1024 : 2048, small ? 128 : 256, anisotropy);
  const crescent = new THREE.Mesh(crescentGeometry(), new THREE.MeshPhysicalMaterial({
    map: bronze.map,
    normalMap: bronze.normal,
    normalScale: new THREE.Vector2(0.6, 0.6),
    roughnessMap: bronze.surface,
    roughness: 1,
    metalnessMap: bronze.surface,
    metalness: 1,
    aoMap: bronze.surface,
    anisotropy: 0.85,
    clearcoat: 0.12,
    clearcoatRoughness: 0.35,
    envMapIntensity: 1.0,
  }));
  crescent.position.set(-MARK_CENTER.x, -MARK_CENTER.y, 0);

  const bake = { uBakeY: { value: -99 }, uBakeWidth: { value: 0.1 }, uBakeGain: { value: 0 } };
  const crateTexture = small ? 512 : 1024;
  const crateMaterial = (maps) => withBake(new THREE.MeshPhysicalMaterial({
    map: maps.map,
    normalMap: maps.normal,
    normalScale: new THREE.Vector2(0.75, 0.75),
    aoMap: maps.surface,
    roughnessMap: maps.surface,
    roughness: 1,
    metalnessMap: maps.surface,
    metalness: 1,
    emissiveMap: maps.glow,
    emissive: new THREE.Color(1.0, 0.82, 0.48),
    emissiveIntensity: 0.4,
    envMapIntensity: 0.5,
  }), bake);
  const side = crateMaterial(crateMaps(crateTexture, true, anisotropy));
  const end = crateMaterial(crateMaps(crateTexture, false, anisotropy));
  const crate = new THREE.Mesh(new THREE.BoxGeometry(CRATE.size, CRATE.size, CRATE.size), [side, side, end, end, side, side]);
  const crateTilt = new THREE.Group();
  crateTilt.add(crate);
  return { crescent, crate, crateTilt, bake };
}

// The scene --------------------------------------------------------------------------------------

function mount(root) {
  const still = document.createElement('img');
  still.className = 'mb-hero__still';
  still.alt = '';
  still.decoding = 'async';
  still.src = new URL('../img/morrobroom-logo.webp', import.meta.url).href;
  root.append(still);

  const canvas = document.createElement('canvas');
  canvas.className = 'mb-hero__canvas';
  let renderer;
  try {
    renderer = new THREE.WebGLRenderer({ canvas, antialias: false, alpha: false, powerPreference: 'high-performance' });
  } catch {
    placeStill();
    return;
  }
  if (!renderer.capabilities.isWebGL2) {
    renderer.dispose();
    placeStill();
    return;
  }
  renderer.autoClear = false;
  renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
  root.append(canvas);

  const floatTargets = renderer.extensions.has('EXT_color_buffer_float') || renderer.extensions.has('EXT_color_buffer_half_float');
  const targetType = floatTargets ? THREE.HalfFloatType : THREE.UnsignedByteType;
  const makeTarget = () => new THREE.WebGLRenderTarget(1, 1, { type: targetType, depthBuffer: false });
  const sceneTarget = new THREE.WebGLRenderTarget(1, 1, { type: targetType, samples: 4 });
  const reflectTarget = new THREE.WebGLRenderTarget(1, 1, { type: targetType });
  const bloomTargets = [makeTarget(), makeTarget(), makeTarget(), makeTarget()];
  const anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());

  const accent = cssColor('--dw-accent', '#e0b867');
  const top = cssColor('--dw-bg-1', '#15110b');
  const bottom = cssColor('--dw-bg-0', '#0e0b05');

  const camera = new THREE.PerspectiveCamera(30, 1, 0.1, 80);
  camera.position.set(0, 1.1, 10);
  camera.lookAt(0, 0, 0);

  const scene = new THREE.Scene();
  const mirrorScene = new THREE.Scene();
  const envTarget = environment(renderer, accent);
  scene.environment = envTarget.texture;
  mirrorScene.environment = envTarget.texture;

  const quad = new THREE.PlaneGeometry(2, 2);
  const skyUniforms = {
    uTop: { value: top },
    uBottom: { value: bottom },
    uGlow: { value: new THREE.Color(0.2, 0.12, 0.045).lerp(accent, 0.3).multiplyScalar(0.32) },
    uAccent: { value: accent },
    uCenter: { value: new THREE.Vector2(0.75, 0.5) },
    uResolution: { value: new THREE.Vector2(1, 1) },
    uRadius: { value: 0.3 },
    uTime: { value: 0 },
  };
  const sky = new THREE.Mesh(quad, fullscreenMaterial(SKY_FRAGMENT, skyUniforms));
  sky.frustumCulled = false;
  sky.renderOrder = -10;
  scene.add(sky);

  // The mark.
  const quality = { level: 1, slow: 0 };
  const small = Math.min(innerWidth, innerHeight) < 700;
  const { crescent, crate, crateTilt, bake } = buildMark(anisotropy, small);
  const crateOffset = new THREE.Vector3(CRATE.x - MARK_CENTER.x, CRATE.y - MARK_CENTER.y, CRATE.z);
  const mark = new THREE.Group();
  mark.add(crescent);
  scene.add(mark, crateTilt);

  // Its reflection: the same meshes, mirrored in the floor, drawn with the same camera.
  const mirrored = [crescent, crate].map((source) => {
    const copy = new THREE.Mesh(source.geometry, source.material);
    copy.matrixAutoUpdate = false;
    copy.matrixWorldAutoUpdate = false;
    mirrorScene.add(copy);
    return { source, copy };
  });
  const mirror = new THREE.Matrix4();

  // Lights: a warm key, cool and accent rims, and the pointer's lamp.
  const lights = (target) => {
    const key = new THREE.DirectionalLight(new THREE.Color(1.0, 0.88, 0.7), 1.7);
    key.position.set(-4, 5, 6);
    const rim = new THREE.DirectionalLight(new THREE.Color(0.62, 0.78, 1.0), 2.6);
    rim.position.set(5, 3, -6);
    const back = new THREE.DirectionalLight(accent, 2.0);
    back.position.set(-6, -1, -4);
    const lamp = new THREE.PointLight(new THREE.Color(1.0, 0.9, 0.75), 0, 0, 2);
    target.add(key, rim, back, lamp);
    return lamp;
  };
  const lamps = [lights(scene), lights(mirrorScene)];

  // The floor.
  const floorUniforms = {
    tReflect: { value: reflectTarget.texture },
    uResolution: { value: new THREE.Vector2(1, 1) },
    uCenter: { value: new THREE.Vector3() },
    uScale: { value: 1 },
    uMinor: { value: accent.clone().multiplyScalar(0.045) },
    uMajor: { value: accent.clone().multiplyScalar(0.12) },
    uOpacity: { value: 1 },
    uSweep: { value: 0 },
  };
  const floor = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), new THREE.ShaderMaterial({
    vertexShader: FLOOR_VERTEX,
    fragmentShader: FLOOR_FRAGMENT,
    uniforms: floorUniforms,
    transparent: true,
    depthWrite: false,
    blending: THREE.CustomBlending,
    blendSrc: THREE.OneFactor,
    blendDst: THREE.OneMinusSrcAlphaFactor,
  }));
  floor.rotation.x = -Math.PI / 2;
  floor.renderOrder = -5;
  scene.add(floor);

  // Embers.
  const emberCount = small ? Math.round(EMBERS * 0.5) : EMBERS;
  const emberGeometry = new THREE.BufferGeometry();
  const seeds = new Float32Array(emberCount * 4);
  const seedRandom = random(91);
  for (let i = 0; i < seeds.length; i++) seeds[i] = seedRandom();
  emberGeometry.setAttribute('position', new THREE.BufferAttribute(new Float32Array(emberCount * 3), 3));
  emberGeometry.setAttribute('aSeed', new THREE.BufferAttribute(seeds, 4));
  const emberUniforms = {
    uTime: { value: 0 },
    uScale: { value: 1 },
    uPixel: { value: 40 },
    uCenter: { value: new THREE.Vector3() },
    uIntensity: { value: 1.6 },
  };
  const embers = new THREE.Points(emberGeometry, new THREE.ShaderMaterial({
    vertexShader: EMBER_VERTEX,
    fragmentShader: EMBER_FRAGMENT,
    uniforms: emberUniforms,
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
  }));
  embers.frustumCulled = false;
  embers.renderOrder = 5;
  scene.add(embers);

  // Post-processing.
  const postScene = new THREE.Scene();
  const postCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  const postQuad = new THREE.Mesh(quad);
  postQuad.frustumCulled = false;
  postScene.add(postQuad);
  const brightMaterial = fullscreenMaterial(BRIGHT_FRAGMENT, { tInput: { value: sceneTarget.texture }, uThreshold: { value: 1.0 } });
  const blurMaterial = fullscreenMaterial(BLUR_FRAGMENT, { tInput: { value: null }, uDirection: { value: new THREE.Vector2() } });
  const copyMaterial = fullscreenMaterial(/* glsl */ `
    uniform sampler2D tInput;
    varying vec2 vUv;
    void main() { gl_FragColor = texture2D(tInput, vUv); }
  `, { tInput: { value: null } });
  const compositeMaterial = fullscreenMaterial(COMPOSITE_FRAGMENT, {
    tScene: { value: sceneTarget.texture },
    tBloomNear: { value: bloomTargets[0].texture },
    tBloomFar: { value: bloomTargets[2].texture },
    uTime: { value: 0 },
  });
  function pass(material, target) {
    postQuad.material = material;
    renderer.setRenderTarget(target);
    renderer.render(postScene, postCamera);
  }
  function blur(target, scratch, radius) {
    blurMaterial.uniforms.tInput.value = target.texture;
    blurMaterial.uniforms.uDirection.value.set(radius / target.width, 0);
    pass(blurMaterial, scratch);
    blurMaterial.uniforms.tInput.value = scratch.texture;
    blurMaterial.uniforms.uDirection.value.set(0, radius / target.height);
    pass(blurMaterial, target);
  }

  // Layout: the size of everything, and where the mark stands.
  let width = 1;
  let height = 1;
  let scale = 1;
  let place = { x: 0, y: 0, size: 0, above: false };
  const anchor = new THREE.Vector3();
  const raycaster = new THREE.Raycaster();
  const plane = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  const ndc = new THREE.Vector2();
  const tmp = new THREE.Vector3();

  function placeStill() {
    const spot = placement(root);
    Object.assign(still.style, {
      left: `${spot.x - spot.size / 2}px`,
      top: `${spot.y - spot.size / 2}px`,
      width: `${spot.size}px`,
      height: `${spot.size}px`,
    });
    root.classList.add('is-placed');
  }

  function layout() {
    const rect = root.getBoundingClientRect();
    width = Math.max(1, Math.round(rect.width));
    height = Math.max(1, Math.round(rect.height));
    const dpr = Math.min(window.devicePixelRatio || 1, 1.75) * quality.level;
    renderer.setPixelRatio(dpr);
    renderer.setSize(width, height, false);
    const w = Math.max(1, Math.floor(width * dpr));
    const h = Math.max(1, Math.floor(height * dpr));
    sceneTarget.setSize(w, h);
    reflectTarget.setSize(Math.max(1, w >> 1), Math.max(1, h >> 1));
    bloomTargets[0].setSize(Math.max(1, w >> 2), Math.max(1, h >> 2));
    bloomTargets[1].setSize(Math.max(1, w >> 2), Math.max(1, h >> 2));
    bloomTargets[2].setSize(Math.max(1, w >> 3), Math.max(1, h >> 3));
    bloomTargets[3].setSize(Math.max(1, w >> 3), Math.max(1, h >> 3));
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    camera.updateMatrixWorld();
    floorUniforms.uResolution.value.set(w, h);
    skyUniforms.uResolution.value.set(width, height);

    place = placement(root);
    placeStill();
    ndc.set(place.x / width * 2 - 1, -(place.y / height * 2 - 1));
    raycaster.setFromCamera(ndc, camera);
    raycaster.ray.intersectPlane(plane, anchor);
    const unitsPerPixel = 2 * camera.position.distanceTo(anchor) * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) / height;
    scale = Math.max(0.05, place.size * 0.94 * unitsPerPixel / 2);
    mark.scale.setScalar(scale);
    const floorY = anchor.y - 1.12 * scale;
    floor.position.set(anchor.x, floorY, anchor.z);
    floor.scale.setScalar(scale * 16);
    floorUniforms.uCenter.value.set(anchor.x, floorY, anchor.z);
    floorUniforms.uScale.value = scale;
    floorUniforms.uOpacity.value = place.above ? 0 : 1;
    mirror.makeTranslation(0, floorY, 0).multiply(new THREE.Matrix4().makeScale(1, -1, 1)).multiply(new THREE.Matrix4().makeTranslation(0, -floorY, 0));
    emberUniforms.uCenter.value.copy(anchor);
    emberUniforms.uScale.value = scale;
    emberUniforms.uPixel.value = height * dpr / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)));
    skyUniforms.uCenter.value.set(place.x / width, 1 - place.y / height);
    skyUniforms.uRadius.value = place.size / height * 0.55;
    for (const lamp of lamps) lamp.intensity = 0;
  }

  // The pointer: a lamp in front of the mark, which leans towards it. A click spins the crate and
  // runs a bake.
  const pointer = new THREE.Vector2(0, 0);
  let pointerActive = false;
  let lastPointer = 0;
  let presence = 0;
  const lampTarget = new THREE.Vector3();
  const lampPosition = new THREE.Vector3();
  const lean = new THREE.Vector2();
  let spin = Math.PI / 4;
  let spinBoost = 0;
  let bakeStart = 2.5;
  function onPointer(event) {
    const rect = root.getBoundingClientRect();
    pointer.set((event.clientX - rect.left) / rect.width * 2 - 1, -((event.clientY - rect.top) / rect.height * 2 - 1));
    pointerActive = true;
    lastPointer = performance.now();
  }
  function onPress(event) {
    onPointer(event);
    spinBoost = 1;
    bakeStart = time;
  }
  function onLeave() {
    pointerActive = false;
  }
  const hero = root.closest('.dw-hero') || root;
  if (!reduceMotion) {
    hero.addEventListener('pointermove', onPointer, { passive: true });
    hero.addEventListener('pointerdown', onPress, { passive: true });
    hero.addEventListener('pointerleave', onLeave, { passive: true });
  }

  // The loop.
  const clock = new THREE.Clock();
  let time = reduceMotion ? 4.2 : 0;
  let visible = false;
  let running = false;
  let first = true;
  let lost = false;

  function frame() {
    running = false;
    if (lost) return;
    const rawDt = clock.getDelta();
    const dt = Math.min(rawDt, 0.05);
    if (!reduceMotion && rawDt < 0.5) {
      quality.slow = rawDt > 1 / 40 ? quality.slow + rawDt : Math.max(0, quality.slow - rawDt * 0.5);
      if (quality.slow > 1.5 && quality.level > 0.5) {
        quality.level = Math.max(0.5, quality.level - 0.2);
        quality.slow = 0;
        layout();
      }
    }
    if (!reduceMotion) time += dt;
    skyUniforms.uTime.value = time;
    emberUniforms.uTime.value = time;
    compositeMaterial.uniforms.uTime.value = time;

    // The lamp: the pointer while it moves over the hero, a slow orbit of the mark otherwise.
    const idle = !pointerActive || performance.now() - lastPointer > 4000;
    if (idle) {
      tmp.set(anchor.x + Math.sin(time * 0.4) * 1.1 * scale, anchor.y + Math.cos(time * 0.29) * 0.8 * scale, anchor.z + 1.3 * scale);
      lampTarget.copy(tmp);
    } else {
      raycaster.setFromCamera(pointer, camera);
      plane.constant = -(anchor.z + 1.2 * scale);
      if (raycaster.ray.intersectPlane(plane, tmp)) lampTarget.copy(tmp);
      plane.constant = 0;
    }
    const wanted = idle ? 0.35 : 1;
    presence += (wanted - presence) * (reduceMotion ? 1 : Math.min(1, dt * 3));
    lampPosition.lerp(lampTarget, reduceMotion ? 1 : Math.min(1, dt * 6));
    for (const lamp of lamps) {
      lamp.position.copy(lampPosition);
      lamp.intensity = presence * 18 * scale * scale;
    }

    // The mark faces front, leaning a little towards the lamp and swaying; the crate turns.
    const dx = (lampPosition.x - anchor.x) / scale;
    const dy = (lampPosition.y - anchor.y) / scale;
    const follow = reduceMotion ? 1 : Math.min(1, dt * 2.5);
    lean.x += (THREE.MathUtils.clamp(-dy * 0.05, -0.08, 0.08) - lean.x) * follow;
    lean.y += (THREE.MathUtils.clamp(dx * 0.05, -0.08, 0.08) - lean.y) * follow;
    mark.rotation.set(lean.x + Math.sin(time * 0.33) * 0.02, lean.y + Math.sin(time * 0.21) * 0.05, Math.sin(time * 0.27) * 0.01);
    mark.position.set(anchor.x, anchor.y + Math.sin(time * 0.7) * 0.025 * scale, anchor.z);
    spin += dt * (0.38 + spinBoost * 7);
    spinBoost *= Math.exp(-dt * 1.4);
    mark.updateMatrixWorld();
    crateTilt.position.copy(crateOffset);
    mark.localToWorld(crateTilt.position);
    crateTilt.scale.setScalar(scale);
    crateTilt.rotation.set(CRATE.tilt + lean.x * 0.5, 0, 0);
    crate.rotation.y = spin + mark.rotation.y;
    scene.updateMatrixWorld();

    // The bake: every nine seconds a band climbs the crate, and a ring runs out across the floor.
    if (!reduceMotion && time - bakeStart > 9) bakeStart = time;
    const bakeAge = (time - bakeStart) / 1.8;
    const crateCentre = crateTilt.getWorldPosition(tmp).y;
    const reach = CRATE.size * 0.85 * scale;
    if (bakeAge >= 0 && bakeAge <= 1 && !reduceMotion) {
      bake.uBakeY.value = crateCentre - reach + bakeAge * reach * 2;
      bake.uBakeGain.value = Math.sin(Math.PI * bakeAge);
      floorUniforms.uSweep.value = Math.max(0.001, bakeAge);
    } else {
      bake.uBakeGain.value = 0;
      floorUniforms.uSweep.value = 0;
    }
    bake.uBakeWidth.value = 0.12 * scale;

    // The reflection, then the scene, then the bloom and the composite.
    for (const { source, copy } of mirrored) copy.matrixWorld.multiplyMatrices(mirror, source.matrixWorld);
    if (floorUniforms.uOpacity.value > 0) {
      renderer.setRenderTarget(reflectTarget);
      renderer.setClearColor(0x000000, 1);
      renderer.clear();
      renderer.render(mirrorScene, camera);
    }
    renderer.setRenderTarget(sceneTarget);
    renderer.clear();
    renderer.render(scene, camera);

    pass(brightMaterial, bloomTargets[0]);
    blur(bloomTargets[0], bloomTargets[1], 1.0);
    blur(bloomTargets[0], bloomTargets[1], 2.0);
    copyMaterial.uniforms.tInput.value = bloomTargets[0].texture;
    pass(copyMaterial, bloomTargets[2]);
    blur(bloomTargets[2], bloomTargets[3], 1.5);
    blur(bloomTargets[2], bloomTargets[3], 3.0);
    pass(compositeMaterial, null);

    if (first) {
      first = false;
      root.classList.add('is-live');
    }
    if (visible && !reduceMotion && !document.hidden) requestFrame();
  }

  function requestFrame() {
    if (running || lost) return;
    running = true;
    requestAnimationFrame(frame);
  }

  canvas.addEventListener('webglcontextlost', (event) => {
    event.preventDefault();
    lost = true;
    root.classList.remove('is-live');
  });
  canvas.addEventListener('webglcontextrestored', () => {
    canvas.remove();
    still.remove();
    root.classList.remove('is-live', 'is-placed');
    mount(root);
  });

  layout();
  new ResizeObserver(() => {
    layout();
    requestFrame();
  }).observe(root);
  if (document.fonts) {
    document.fonts.ready.then(() => {
      layout();
      requestFrame();
    });
  }
  new IntersectionObserver((entries) => {
    visible = entries.some((entry) => entry.isIntersecting);
    if (visible) {
      clock.getDelta();
      requestFrame();
    }
  }).observe(root);
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden && visible) {
      clock.getDelta();
      requestFrame();
    }
  });
}

for (const root of document.querySelectorAll('[data-dw-hero-art]')) mount(root);
