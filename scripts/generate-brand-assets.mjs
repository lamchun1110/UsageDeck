// UsageDeck brand asset pipeline.
// Source of truth: assets/usagedeck-banner.png (the official logo artwork).
//
// Emits:
//   assets/usagedeck-tray.svg    renderer contract for src-tauri/src/tray_icon.rs
//   assets/usagedeck-appicon.png 1024 app-icon master (black rounded square + mark)
//   src/assets/usagedeck-mark.png transparent official mark for the in-app logo
//
// The mark is extracted from the banner by un-premultiplying against black, so
// the background becomes transparent and the knockout bars stay true holes.
// Regenerate the platform icons afterwards with:
//   corepack pnpm tauri icon assets/usagedeck-appicon.png
// Run: node scripts/generate-brand-assets.mjs
import fs from 'node:fs';
import pngjs from 'pngjs';

const { PNG } = pngjs;
const fmt = (n) => Number(n.toFixed(4)).toString();
const K = 0.5522847498307936; // circle-to-cubic kappa

// ---- Tray mark (24x24). Contract consumed by src-tauri/src/tray_icon.rs:
// root #usagedeck-tray, #track > track-segment-N, #fill > fill-segment-N
// (N=1..6), plus one static path #brand-mark. Colors come from the Rust
// renderer; fill attributes here are cosmetic previews only. The static glyph
// is the official four-bar chart, sized to stay inside the gauge ring.
function polar(cx, cy, r, deg) {
  const a = ((deg - 90) * Math.PI) / 180;
  return [cx + r * Math.cos(a), cy + r * Math.sin(a)];
}

function arcCubic(cx, cy, r, startDeg, endDeg, reverse = false) {
  const theta = Math.abs(endDeg - startDeg);
  const handle = (4 / 3) * Math.tan(((theta / 2) * Math.PI) / 180) * r;
  const t0 = reverse ? endDeg : startDeg;
  const t1 = reverse ? startDeg : endDeg;
  const dir = reverse ? -1 : 1;
  const tangent = (deg) => {
    const a = ((deg - 90) * Math.PI) / 180;
    return [-Math.sin(a), Math.cos(a)];
  };
  const [x1, y1] = polar(cx, cy, r, t0);
  const [x2, y2] = polar(cx, cy, r, t1);
  const [u0x, u0y] = tangent(t0);
  const [u1x, u1y] = tangent(t1);
  return {
    x1,
    y1,
    c1x: x1 + dir * handle * u0x,
    c1y: y1 + dir * handle * u0y,
    c2x: x2 - dir * handle * u1x,
    c2y: y2 - dir * handle * u1y,
    x2,
    y2,
  };
}

function annularSector(cx, cy, rOut, rIn, startDeg, endDeg) {
  const o = arcCubic(cx, cy, rOut, startDeg, endDeg);
  const i = arcCubic(cx, cy, rIn, startDeg, endDeg, true);
  return (
    `M${fmt(o.x1)} ${fmt(o.y1)}` +
    `C${fmt(o.c1x)} ${fmt(o.c1y)} ${fmt(o.c2x)} ${fmt(o.c2y)} ${fmt(o.x2)} ${fmt(o.y2)}` +
    `L${fmt(i.x1)} ${fmt(i.y1)}` +
    `C${fmt(i.c1x)} ${fmt(i.c1y)} ${fmt(i.c2x)} ${fmt(i.c2y)} ${fmt(i.x2)} ${fmt(i.y2)}` +
    'Z'
  );
}

function roundedRect(x, y, w, h, r) {
  const k = K * r;
  return (
    `M${fmt(x + r)} ${fmt(y)}` +
    `L${fmt(x + w - r)} ${fmt(y)}` +
    `C${fmt(x + w - r + k)} ${fmt(y)} ${fmt(x + w)} ${fmt(y + r - k)} ${fmt(x + w)} ${fmt(y + r)}` +
    `L${fmt(x + w)} ${fmt(y + h - r)}` +
    `C${fmt(x + w)} ${fmt(y + h - r + k)} ${fmt(x + w - r + k)} ${fmt(y + h)} ${fmt(x + w - r)} ${fmt(y + h)}` +
    `L${fmt(x + r)} ${fmt(y + h)}` +
    `C${fmt(x + r - k)} ${fmt(y + h)} ${fmt(x)} ${fmt(y + h - r + k)} ${fmt(x)} ${fmt(y + h - r)}` +
    `L${fmt(x)} ${fmt(y + r)}` +
    `C${fmt(x)} ${fmt(y + r - k)} ${fmt(x + r - k)} ${fmt(y)} ${fmt(x + r)} ${fmt(y)}` +
    'Z'
  );
}

const CX = 12;
const CY = 12;
const R_OUT = 11;
const R_IN = 8.35;
const SEGMENTS = 6;
const GAP_DEG = 9;

let trackPaths = '';
let fillPaths = '';
for (let index = 0; index < SEGMENTS; index += 1) {
  const start = index * (360 / SEGMENTS) + GAP_DEG / 2;
  const end = (index + 1) * (360 / SEGMENTS) - GAP_DEG / 2;
  const d = annularSector(CX, CY, R_OUT, R_IN, start, end);
  trackPaths += `<path id="track-segment-${index + 1}" d="${d}" fill="#161616"/>`;
  fillPaths += `<path id="fill-segment-${index + 1}" d="${d}" fill="#1689EF"/>`;
}

const TRAY_BAR_W = 1.9;
const TRAY_BAR_GAP = 1.18;
const TRAY_BAR_BASE = 16.25;
const TRAY_BAR_HEIGHTS = [2.7, 4.5, 6.3, 8.1];
const TRAY_BARS_W =
  TRAY_BAR_HEIGHTS.length * TRAY_BAR_W + (TRAY_BAR_HEIGHTS.length - 1) * TRAY_BAR_GAP;
const TRAY_BARS_X = CX - TRAY_BARS_W / 2;
const trayBars = TRAY_BAR_HEIGHTS.map((height, index) =>
  roundedRect(
    TRAY_BARS_X + index * (TRAY_BAR_W + TRAY_BAR_GAP),
    TRAY_BAR_BASE - height,
    TRAY_BAR_W,
    height,
    TRAY_BAR_W / 2,
  ),
).join(' ');

fs.writeFileSync(
  'assets/usagedeck-tray.svg',
  `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
<g id="usagedeck-tray">
<g id="track">${trackPaths}</g>
<g id="fill">${fillPaths}</g>
<path id="brand-mark" d="${trayBars}" fill="#FFFFFF"/>
</g>
</svg>
`,
);

// ---- Extract the official mark from the banner.
const banner = PNG.sync.read(fs.readFileSync('assets/usagedeck-banner.png'));
const BRIGHT = 120; // card pixels only; the navy wordmark and black bg stay below this

// Per-column bright-pixel presence, then merge column runs separated by small
// gaps. The mark is the first merged run; the wordmark sits after a wide gap
// and is excluded.
const columnHasBright = new Array(banner.width).fill(false);
for (let y = 0; y < banner.height; y += 1) {
  for (let x = 0; x < banner.width; x += 1) {
    const i = (y * banner.width + x) * 4;
    const bright = Math.max(banner.data[i], banner.data[i + 1], banner.data[i + 2]);
    if (bright > BRIGHT) columnHasBright[x] = true;
  }
}
const GAP_TOLERANCE = 12;
let markEnd = -1;
let markStart = -1;
let lastBrightColumn = -1;
for (let x = 0; x < banner.width; x += 1) {
  if (!columnHasBright[x]) continue;
  if (markStart < 0) {
    markStart = x;
  } else if (x - lastBrightColumn > GAP_TOLERANCE) {
    break; // gap before the wordmark: stop at the end of the mark
  }
  lastBrightColumn = x;
  markEnd = x;
}
if (markEnd < 0) throw new Error('No mark pixels found in assets/usagedeck-banner.png');

let minY = banner.height;
let maxY = -1;
for (let y = 0; y < banner.height; y += 1) {
  for (let x = markStart; x <= markEnd; x += 1) {
    const i = (y * banner.width + x) * 4;
    const bright = Math.max(banner.data[i], banner.data[i + 1], banner.data[i + 2]);
    if (bright > BRIGHT) {
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
  }
}

let minX = markStart;
let maxX = markEnd;

const pad = 2;
minX = Math.max(0, minX - pad);
minY = Math.max(0, minY - pad);
maxX = Math.min(banner.width - 1, maxX + pad);
maxY = Math.min(banner.height - 1, maxY + pad);
const cropW = maxX - minX + 1;
const cropH = maxY - minY + 1;

// Sample the banner crop with black un-premultiplied: alpha = max(r,g,b),
// color = c * 255 / alpha. Black background and knockout bars become
// transparent; gradients survive compositing on any surface.
function compositeMark(target, destX, destY, scale, opaque) {
  const drawW = Math.round(cropW * scale);
  const drawH = Math.round(cropH * scale);
  for (let y = 0; y < drawH; y += 1) {
    for (let x = 0; x < drawW; x += 1) {
      const sx = minX + Math.min(cropW - 1, Math.floor(x / scale));
      const sy = minY + Math.min(cropH - 1, Math.floor(y / scale));
      const si = (sy * banner.width + sx) * 4;
      const alpha = Math.max(banner.data[si], banner.data[si + 1], banner.data[si + 2]);
      if (alpha <= 8) continue;
      const ti = ((destY + y) * target.width + (destX + x)) * 4;
      for (let c = 0; c < 3; c += 1) {
        target.data[ti + c] = Math.min(255, Math.round((banner.data[si + c] * 255) / alpha));
      }
      target.data[ti + 3] = opaque ? 255 : alpha;
    }
  }
  return { drawW, drawH };
}

// Transparent mark, centered on a 512 square (in-app logo).
const MARK_SIZE = 512;
const mark = new PNG({ width: MARK_SIZE, height: MARK_SIZE });
const markScale = (MARK_SIZE * 0.92) / Math.max(cropW, cropH);
const { drawW: mw, drawH: mh } = compositeMark(
  mark,
  Math.round((MARK_SIZE - cropW * markScale) / 2),
  Math.round((MARK_SIZE - cropH * markScale) / 2),
  markScale,
  false,
);
fs.writeFileSync('src/assets/usagedeck-mark.png', PNG.sync.write(mark));

// App icon master: black rounded square + official mark (1024).
const S = 1024;
const icon = new PNG({ width: S, height: S });
const BG = [10, 10, 12];
const RADIUS = 232;
const insideRoundedSquare = (x, y) => {
  const rx = Math.min(x, S - 1 - x);
  const ry = Math.min(y, S - 1 - y);
  if (rx >= RADIUS || ry >= RADIUS) return true;
  const dx = RADIUS - rx;
  const dy = RADIUS - ry;
  return dx * dx + dy * dy <= RADIUS * RADIUS;
};
for (let y = 0; y < S; y += 1) {
  for (let x = 0; x < S; x += 1) {
    const i = (y * S + x) * 4;
    icon.data[i] = BG[0];
    icon.data[i + 1] = BG[1];
    icon.data[i + 2] = BG[2];
    icon.data[i + 3] = insideRoundedSquare(x, y) ? 255 : 0;
  }
}
const iconScale = (S * 0.62) / Math.max(cropW, cropH);
compositeMark(
  icon,
  Math.round((S - cropW * iconScale) / 2),
  Math.round((S - cropH * iconScale) / 2),
  iconScale,
  true,
);
fs.writeFileSync('assets/usagedeck-appicon.png', PNG.sync.write(icon));

console.log(
  `wrote assets/usagedeck-tray.svg, assets/usagedeck-appicon.png, src/assets/usagedeck-mark.png ` +
    `(mark crop ${cropW}x${cropH} at ${minX},${minY}; mark ${mw}x${mh})`,
);
