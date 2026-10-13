const dpr = () => window.devicePixelRatio || 1;

export function sizeCanvas(canvas: HTMLCanvasElement): CanvasRenderingContext2D | null {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  canvas.width = Math.max(1, Math.floor(w * dpr()));
  canvas.height = Math.max(1, Math.floor(h * dpr()));
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.setTransform(dpr(), 0, 0, dpr(), 0, 0);
  ctx.clearRect(0, 0, w, h);
  return ctx;
}

export interface Bar {
  label: string;
  value: number;
  color: string;
}

/** Vertical bars with labels under them. Values scaled to the max. */
export function drawBars(canvas: HTMLCanvasElement, bars: Bar[]): void {
  const ctx = sizeCanvas(canvas);
  if (!ctx) return;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  const pad = { top: 14, bottom: 18, left: 4, right: 4 };
  const max = Math.max(1, ...bars.map((b) => b.value));
  const slot = (w - pad.left - pad.right) / Math.max(1, bars.length);
  ctx.font = "11px system-ui, sans-serif";
  ctx.textAlign = "center";
  bars.forEach((b, i) => {
    const bh = ((h - pad.top - pad.bottom) * b.value) / max;
    const x = pad.left + i * slot + slot * 0.2;
    const bw = slot * 0.6;
    const y = h - pad.bottom - bh;
    ctx.fillStyle = b.color;
    ctx.fillRect(x, y, bw, bh);
    ctx.fillStyle = "#333";
    ctx.fillText(b.label, x + bw / 2, h - 4);
    ctx.fillText(compact(b.value), x + bw / 2, Math.max(10, y - 3));
  });
}

/** Filled area per channel, normalized to the global max. */
export function drawHistogram(canvas: HTMLCanvasElement, channels: { values: number[]; color: string }[]): void {
  const ctx = sizeCanvas(canvas);
  if (!ctx) return;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  const max = Math.max(1, ...channels.flatMap((c) => c.values));
  ctx.globalAlpha = 0.55;
  for (const ch of channels) {
    ctx.beginPath();
    ctx.moveTo(0, h);
    ch.values.forEach((v, i) => ctx.lineTo((i / (ch.values.length - 1)) * w, h - (v / max) * (h - 4)));
    ctx.lineTo(w, h);
    ctx.closePath();
    ctx.fillStyle = ch.color;
    ctx.fill();
  }
  ctx.globalAlpha = 1;
}

export interface Slice {
  label: string;
  value: number;
  color: string;
}

/** Pie chart; slices are drawn clockwise from 12 o'clock. The canvas should be wider than tall. */
export function drawPie(canvas: HTMLCanvasElement, slices: Slice[]): void {
  const ctx = sizeCanvas(canvas);
  if (!ctx) return;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  const total = slices.reduce((sum, s) => sum + s.value, 0);
  if (total <= 0) return;
  const cx = w / 2;
  const cy = h / 2;
  const r = Math.max(4, Math.min(w, h) / 2 - 4);
  let angle = -Math.PI / 2;
  for (const s of slices) {
    if (s.value <= 0) continue;
    const span = (s.value / total) * Math.PI * 2;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.arc(cx, cy, r, angle, angle + span);
    ctx.closePath();
    ctx.fillStyle = s.color;
    ctx.fill();
    angle += span;
  }
}

export function compact(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(Math.round(n));
}
