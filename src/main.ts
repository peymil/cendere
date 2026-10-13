import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type Analysis,
  type ApplyReport,
  type JobItem,
  type Options,
  type ProcessFinished,
  type ProcessProgress,
  type ScanItem,
} from "./api";
import { compact, drawBars, drawHistogram } from "./charts";
import { Compare } from "./compare";

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el as T;
};

const input = (id: string) => $<HTMLInputElement>(id);
const select = (id: string) => $<HTMLSelectElement>(id);

const state = {
  rows: [] as ScanItem[],
  included: new Set<string>(),
  applyChecked: new Set<string>(),
  items: new Map<string, JobItem>(),
  running: false,
  busy: false,
  stale: false,
  selected: null as string | null,
  // Paths shown in the list. null = all scanned files; set to the processed files after Process.
  listFilter: null as Set<string> | null,
  sort: null as { key: SortKey; dir: "asc" | "desc" } | null,
  thumbs: new Map<string, string>(),
  thumbEls: new Map<string, HTMLImageElement>(),
  analyses: new Map<string, Analysis>(),
};

const THUMB_CONCURRENCY = 4;
const thumbQueue: string[] = [];
let thumbActive = 0;

const compare = new Compare(
  $("cmp-stage"),
  $<HTMLImageElement>("cmp-before"),
  $<HTMLImageElement>("cmp-after"),
  $("cmp-line"),
);

// ---------- helpers ----------

function setStatus(text: string, kind: "" | "error" = ""): void {
  const el = $("status");
  el.textContent = text;
  el.className = `statusbar ${kind}`;
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

function pct(n: number): string {
  return `${(n * 100).toFixed(1)}%`;
}

function esc(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function readOptions(): Options {
  const num = (id: string, fallback: number) => {
    const v = Number(input(id).value);
    return Number.isFinite(v) ? v : fallback;
  };
  return {
    lossless: {
      enabled: input("ll-enabled").checked,
      level: Number(select("ll-level").value),
      stripMetadata: input("ll-strip").checked,
      removeInterlace: input("ll-interlace").checked,
      zopfli: input("ll-zopfli").checked,
    },
    alpha: { enabled: input("al-enabled").checked, low: num("al-low", 8), high: num("al-high", 247) },
    resize: {
      enabled: input("rs-enabled").checked,
      mode: select("rs-mode").value as "scale" | "custom",
      scale: Number(select("rs-scale").value),
      width: num("rs-w", 1024),
      height: num("rs-h", 1024),
      keepAspect: input("rs-aspect").checked,
      filter: select("rs-filter").value as "lanczos3" | "nearest",
    },
    palette: {
      enabled: input("pl-enabled").checked,
      mode: select("pl-mode").value as "auto" | "fixed",
      colors: num("pl-colors", 256),
      dithering: input("pl-dither").checked,
    },
  };
}

function optionError(o: Options): string | null {
  if (!o.lossless.enabled && !o.alpha.enabled && !o.resize.enabled && !o.palette.enabled) {
    return "Enable at least one optimization";
  }
  if (o.alpha.enabled && o.alpha.low >= o.alpha.high) {
    return "Alpha: transparent threshold must be below the opaque threshold";
  }
  if (o.resize.enabled && o.resize.mode === "custom" && (o.resize.width < 1 || o.resize.height < 1)) {
    return "Resize: width and height must be at least 1";
  }
  if (o.palette.enabled && o.palette.mode === "fixed" && (o.palette.colors < 2 || o.palette.colors > 256)) {
    return "Palette: color count must be 2-256";
  }
  return null;
}

// ---------- options panel ----------

function syncOptionVisibility(): void {
  $("rs-scale-row").classList.toggle("hidden", select("rs-mode").value !== "scale");
  $("rs-custom-row").classList.toggle("hidden", select("rs-mode").value !== "custom");
  $("pl-fixed-row").classList.toggle("hidden", select("pl-mode").value !== "fixed");
}

function onOptionsChanged(): void {
  syncOptionVisibility();
  if (state.items.size > 0 && !state.running) {
    state.stale = true;
  }
  updateButtons();
}

// ---------- buttons ----------

function processableRows(): ScanItem[] {
  return state.rows.filter((r) => !r.skipReason);
}

function visibleRows(): ScanItem[] {
  const filter = state.listFilter;
  return filter ? state.rows.filter((r) => filter.has(r.path)) : state.rows;
}

// ---------- sorting ----------

type SortKey = "name" | "size" | "result" | "status";

const STATUS_RANK: Record<string, number> = { ready: 0, noGain: 1, failed: 2, none: 3, skip: 4 };

// null = no value; such rows always sort last, whatever the direction.
function sortValue(row: ScanItem, key: SortKey): string | number | null {
  const item = state.items.get(row.path);
  switch (key) {
    case "name":
      return row.name;
    case "size":
      return row.size;
    case "result":
      return item && item.status !== "failed" ? item.resultSize : null;
    case "status":
      if (row.skipReason) return STATUS_RANK.skip;
      if (!item) return STATUS_RANK.none;
      return STATUS_RANK[item.status] ?? STATUS_RANK.failed;
  }
}

function displayRows(): ScanItem[] {
  const rows = visibleRows();
  const sort = state.sort;
  if (!sort) return rows;
  const dir = sort.dir === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    const ka = sortValue(a, sort.key);
    const kb = sortValue(b, sort.key);
    if (ka === null || kb === null) {
      if (ka === kb) return a.name.localeCompare(b.name);
      return ka === null ? 1 : -1;
    }
    const c = typeof ka === "string" && typeof kb === "string" ? ka.localeCompare(kb) : (ka as number) - (kb as number);
    return c !== 0 ? c * dir : a.name.localeCompare(b.name);
  });
}

// Click cycles: ascending, descending, unsorted.
function toggleSort(key: SortKey): void {
  const cur = state.sort;
  if (!cur || cur.key !== key) state.sort = { key, dir: "asc" };
  else if (cur.dir === "asc") state.sort = { key, dir: "desc" };
  else state.sort = null;
  renderRows();
}

function updateSortHeaders(): void {
  document.querySelectorAll<HTMLElement>("#files th[data-sort]").forEach((th) => {
    const active = state.sort?.key === th.dataset.sort;
    th.classList.toggle("sorted", active && state.sort?.dir === "asc");
    th.classList.toggle("sorted-desc", active && state.sort?.dir === "desc");
  });
}

function updateButtons(): void {
  const hasRows = state.rows.length > 0;
  const error = optionError(readOptions());
  const includedCount = processableRows().filter((r) => state.included.has(r.path)).length;
  const readyPaths = [...state.items.values()].filter((i) => i.status === "ready").map((i) => i.path);
  const checkedReady = readyPaths.filter((p) => state.applyChecked.has(p)).length;

  $<HTMLButtonElement>("process").disabled = state.running || state.busy || !hasRows || includedCount === 0 || error !== null;
  $("process").textContent = state.items.size > 0 ? "Process again" : "Process";
  $<HTMLButtonElement>("show-all").disabled = state.listFilter === null || state.running;
  $("cancel").classList.toggle("hidden", !state.running);
  $("progress-wrap").classList.toggle("hidden", !state.running);
  $("stale-note").classList.toggle("hidden", !state.stale);
  $<HTMLButtonElement>("apply").disabled = state.running || state.busy || state.stale || checkedReady === 0;
  $<HTMLButtonElement>("remove-unchecked").disabled = state.running || state.busy || state.items.size === 0;
  $<HTMLButtonElement>("select-all").disabled = !hasRows || state.running;
  $<HTMLButtonElement>("select-none").disabled = !hasRows || state.running;
  $<HTMLButtonElement>("check-ready").disabled = readyPaths.length === 0 || state.running;
  $<HTMLButtonElement>("uncheck-all").disabled = readyPaths.length === 0 || state.running;
  const shown = state.listFilter ? `${visibleRows().length} processed shown · ` : "";
  $("count").textContent = hasRows
    ? `${shown}${state.rows.length} PNG files · ${includedCount} selected for Process · ${state.items.size} results`
    : "No folder scanned";
  if (error && hasRows) setStatus(error, "error");
}

// ---------- thumbnails ----------

const thumbObserver = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      const el = e.target as HTMLElement;
      thumbObserver.unobserve(el);
      requestThumb(el.dataset.path ?? "");
    }
  },
  { root: document.querySelector(".table-wrap"), rootMargin: "200px" },
);

function requestThumb(path: string): void {
  const cached = state.thumbs.get(path);
  if (cached) {
    state.thumbEls.get(path)?.setAttribute("src", cached);
    return;
  }
  if (!thumbQueue.includes(path)) thumbQueue.push(path);
  pumpThumbs();
}

function pumpThumbs(): void {
  while (thumbActive < THUMB_CONCURRENCY && thumbQueue.length > 0) {
    const path = thumbQueue.shift() as string;
    thumbActive++;
    api
      .thumbnail(path, 96)
      .then((url) => {
        state.thumbs.set(path, url);
        state.thumbEls.get(path)?.setAttribute("src", url);
      })
      .catch(() => state.thumbEls.get(path)?.closest(".thumb")?.classList.add("broken"))
      .finally(() => {
        thumbActive--;
        pumpThumbs();
      });
  }
}

// ---------- list ----------

function statusHtml(row: ScanItem): string {
  if (row.skipReason) return `<span class="badge skip" title="${esc(row.skipReason)}">Skipped</span>`;
  const item = state.items.get(row.path);
  if (!item) return `<span class="muted">Not processed</span>`;
  if (item.status === "failed") {
    return `<span class="badge error" title="${esc(item.error ?? "")}">Failed</span>`;
  }
  if (item.status === "noGain") return `<span class="badge muted-badge">No gain</span>`;
  const saved = 1 - item.resultSize / item.sourceSize;
  const note = item.notes.length ? ` title="${esc(item.notes.join("; "))}"` : "";
  const cls = saved >= 0 ? "ready" : "grew";
  return `<span class="badge ${cls}"${note}>${saved >= 0 ? "−" : "+"}${pct(Math.abs(saved))}</span>`;
}

function renderRows(): void {
  const tbody = $("rows");
  tbody.innerHTML = "";
  state.thumbEls.clear();
  updateSortHeaders();
  for (const row of displayRows()) {
    const item = state.items.get(row.path);
    const tr = document.createElement("tr");
    tr.dataset.path = row.path;
    if (row.path === state.selected) tr.classList.add("selected");
    if (state.stale && item) tr.classList.add("stale");
    const canApply = !!item && item.status === "ready" && !state.stale;
    tr.innerHTML = `
      <td class="c-proc"><input type="checkbox" class="inc" ${state.included.has(row.path) ? "checked" : ""} ${row.skipReason || state.running ? "disabled" : ""} /></td>
      <td class="c-thumb"><div class="thumb" data-path="${esc(row.path)}"><img alt="" /></div></td>
      <td class="name" title="${esc(row.path)}">${esc(row.name)}<span class="dim">${row.width}×${row.height} · ${esc(row.colorType)}</span></td>
      <td class="num">${fmtBytes(row.size)}</td>
      <td class="num">${item && item.status !== "failed" ? fmtBytes(item.resultSize) : "–"}</td>
      <td class="c-status">${statusHtml(row)}</td>
      <td class="c-apply"><input type="checkbox" class="app" ${state.applyChecked.has(row.path) ? "checked" : ""} ${canApply ? "" : "disabled"} /></td>`;
    const thumbDiv = tr.querySelector<HTMLElement>(".thumb");
    const img = tr.querySelector<HTMLImageElement>(".thumb img");
    if (img && thumbDiv) {
      state.thumbEls.set(row.path, img);
      const cached = state.thumbs.get(row.path);
      if (cached) img.src = cached;
      else thumbObserver.observe(thumbDiv);
    }
    tbody.appendChild(tr);
  }
  updateButtons();
}

function onRowsClick(ev: MouseEvent): void {
  const target = ev.target as HTMLElement;
  const tr = target.closest("tr");
  const path = tr?.dataset.path;
  if (!path) return;
  if (target instanceof HTMLInputElement && target.classList.contains("inc")) {
    if (target.checked) state.included.add(path);
    else state.included.delete(path);
    updateButtons();
    return;
  }
  if (target instanceof HTMLInputElement && target.classList.contains("app")) {
    if (target.checked) state.applyChecked.add(path);
    else state.applyChecked.delete(path);
    updateButtons();
    return;
  }
  selectFile(path);
}

function markSelectedRow(): void {
  document.querySelectorAll("#rows tr").forEach((tr) => {
    tr.classList.toggle("selected", (tr as HTMLElement).dataset.path === state.selected);
  });
}

// ---------- scan ----------

async function scan(): Promise<void> {
  const dir = input("dir").value.trim();
  if (!dir) {
    setStatus("Enter a folder path or use Browse", "error");
    return;
  }
  if (state.running) return;
  state.busy = true;
  setStatus("Scanning…");
  updateButtons();
  try {
    await api.discardAll().catch(() => undefined);
    state.items.clear();
    state.applyChecked.clear();
    state.stale = false;
    state.selected = null;
    state.listFilter = null;
    $("details").classList.add("hidden");
    state.rows = await api.scanDir(dir);
    state.included = new Set(processableRows().map((r) => r.path));
    const skipped = state.rows.filter((r) => r.skipReason).length;
    setStatus(
      `Found ${state.rows.length} PNG files${skipped ? ` (${skipped} skipped)` : ""} under ${dir}`,
    );
    renderRows();
  } catch (e) {
    state.rows = [];
    renderRows();
    setStatus(String(e), "error");
  } finally {
    state.busy = false;
    updateButtons();
  }
}

async function browse(): Promise<void> {
  const picked = await open({ directory: true, multiple: false });
  if (typeof picked === "string") {
    input("dir").value = picked;
    await scan();
  }
}

// ---------- process ----------

async function startProcess(): Promise<void> {
  const options = readOptions();
  const error = optionError(options);
  if (error) {
    setStatus(error, "error");
    return;
  }
  const paths = processableRows()
    .filter((r) => state.included.has(r.path))
    .map((r) => r.path);
  if (paths.length === 0) return;
  state.running = true;
  state.stale = false;
  state.selected = null;
  $("details").classList.add("hidden");
  state.listFilter = new Set(paths);
  renderRows();
  setProgress(0, paths.length);
  updateButtons();
  try {
    await api.startProcess(paths, options);
    setStatus(`Processing ${paths.length} files…`);
  } catch (e) {
    state.running = false;
    setStatus(String(e), "error");
    updateButtons();
  }
}

function setProgress(done: number, total: number): void {
  const pctDone = total ? (done / total) * 100 : 0;
  $("progress-bar").style.width = `${pctDone}%`;
  setStatus(`Processing ${done} / ${total}`);
}

async function loadJob(): Promise<void> {
  const view = await api.getJob();
  state.items = new Map(view.items.map((i) => [i.path, i]));
}

async function onProcessFinished(done: ProcessFinished): Promise<void> {
  state.running = false;
  await loadJob();
  state.listFilter = state.items.size > 0 ? new Set(state.items.keys()) : null;
  state.applyChecked = new Set(
    [...state.items.values()].filter((i) => i.status === "ready").map((i) => i.path),
  );
  renderRows();
  const parts = [`${done.ready} ready`, `${done.noGain} no gain`, `${done.failed} failed`];
  setStatus(`${done.cancelled ? "Cancelled · " : ""}${parts.join(" · ")}. Review, then Apply.`, done.failed ? "error" : "");
  if (state.selected) void selectFile(state.selected);
}

async function removeResults(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  await api.discardResults(paths);
  for (const p of paths) {
    state.items.delete(p);
    state.applyChecked.delete(p);
    state.listFilter?.delete(p);
  }
  renderRows();
  if (state.selected && paths.includes(state.selected)) void selectFile(state.selected);
  setStatus(`Removed ${paths.length} result(s)`);
}

// ---------- apply ----------

function confirmApply(count: number): Promise<boolean> {
  const dialog = $<HTMLDialogElement>("confirm");
  $("confirm-text").textContent = `${count} file(s) will be replaced with the reviewed results (same name, same folder).`;
  dialog.showModal();
  return new Promise((resolve) => {
    dialog.addEventListener("close", () => resolve(dialog.returnValue === "ok"), { once: true });
  });
}

async function applyChecked(): Promise<void> {
  const paths = [...state.applyChecked].filter((p) => state.items.get(p)?.status === "ready");
  if (paths.length === 0) return;
  if (!(await confirmApply(paths.length))) return;
  state.busy = true;
  setStatus("Applying…");
  updateButtons();
  let report: ApplyReport;
  try {
    report = await api.applyResults(paths);
  } catch (e) {
    state.busy = false;
    setStatus(String(e), "error");
    updateButtons();
    return;
  }
  const skipped = report.skipped.slice(0, 3).map((s) => `${baseName(s.path)}: ${s.reason}`);
  const extra = report.skipped.length > 3 ? ` (+${report.skipped.length - 3} more)` : "";
  setStatus(
    `Applied ${report.applied.length} file(s).${report.skipped.length ? ` Skipped ${report.skipped.length}: ${skipped.join("; ")}${extra}` : ""}`,
    report.skipped.length ? "error" : "",
  );
  state.items.clear();
  state.applyChecked.clear();
  state.stale = false;
  state.busy = false;
  await scanQuiet();
}

async function scanQuiet(): Promise<void> {
  const dir = input("dir").value.trim();
  try {
    state.rows = await api.scanDir(dir);
    state.included = new Set(processableRows().map((r) => r.path));
    state.thumbs.clear();
    state.analyses.clear();
    state.listFilter = null;
    state.selected = null;
    $("details").classList.add("hidden");
  } catch (e) {
    setStatus(String(e), "error");
  }
  renderRows();
}

// ---------- details ----------

async function analysisFor(path: string): Promise<Analysis> {
  const cached = state.analyses.get(path);
  if (cached) return cached;
  const a = await api.analyze(path);
  state.analyses.set(path, a);
  return a;
}

async function selectFile(path: string): Promise<void> {
  state.selected = path;
  markSelectedRow();
  const row = state.rows.find((r) => r.path === path);
  if (!row) return;
  const details = $("details");
  details.classList.remove("hidden");
  $("d-name").textContent = row.name;
  $("d-path").textContent = row.path;
  const item = state.items.get(path);
  const note = $("d-note");
  const notes = [...(item?.notes ?? []), ...(item?.error ? [item.error] : [])];
  note.textContent = notes.join(" · ");
  note.classList.toggle("hidden", notes.length === 0);
  $<HTMLButtonElement>("d-remove").classList.toggle("hidden", !item);

  const compareBox = $("d-compare");
  const noCompare = $("d-nocompare");
  if (item?.status === "ready" && item.stagedPath) {
    compareBox.classList.remove("hidden");
    noCompare.classList.add("hidden");
    const [before, after] = await Promise.all([api.imageDataUrl(path), api.imageDataUrl(item.stagedPath)]);
    if (state.selected === path) compare.setImages(before, after);
  } else {
    compareBox.classList.add("hidden");
    noCompare.textContent = !item
      ? "Not processed yet. Press Process to see the result here."
      : item.status === "failed"
        ? "Processing failed. See the note above."
        : "No gain: the original is kept, so there is nothing to compare.";
    noCompare.classList.remove("hidden");
  }

  renderSizeSummary(row.size, item);
  if (row.skipReason) {
    $("d-info").innerHTML = `<tr><td colspan="3">${esc(row.skipReason)}</td></tr>`;
    return;
  }
  const original = await analysisFor(path);
  if (state.selected !== path) return;
  const result = item?.status === "ready" && item.stagedPath ? await analysisFor(item.stagedPath) : null;
  if (state.selected !== path) return;
  renderInfo(original, result);
  renderPalette(original);
  renderAlpha(original);
  renderRgb(original);
}

function renderSizeSummary(size: number, item: JobItem | undefined): void {
  const box = $("d-size");
  if (!item || item.status === "failed") {
    box.innerHTML = `<div>Original: <b>${fmtBytes(size)}</b></div><div class="muted">${item ? "Processing failed" : "Not processed yet"}</div>`;
    $("c-size").classList.add("hidden");
    return;
  }
  $("c-size").classList.remove("hidden");
  const saved = 1 - item.resultSize / item.sourceSize;
  box.innerHTML = `<div>Original <b>${fmtBytes(item.sourceSize)}</b> → Result <b>${fmtBytes(item.resultSize)}</b></div>
    <div>${item.status === "noGain" ? "No gain (original kept)" : `Saving <b>${pct(saved)}</b>${item.resized ? " · resized" : ""}`}</div>`;
  drawBars($("c-size") as HTMLCanvasElement, [
    { label: "Original", value: item.sourceSize, color: "#8a94a6" },
    { label: "Result", value: item.resultSize, color: item.status === "noGain" ? "#c9a227" : "#2f9e6e" },
  ]);
}

function renderInfo(a: Analysis, r: Analysis | null): void {
  const i = a.info;
  const ri = r?.info;
  const rows: [string, string, string | null][] = [
    ["File size", fmtBytes(a.fileSize), r ? fmtBytes(r.fileSize) : null],
    ["Dimensions", `${i.width} × ${i.height}`, ri ? `${ri.width} × ${ri.height}` : null],
    ["Color type", i.colorTypeName, ri ? ri.colorTypeName : null],
    ["Bit depth", String(i.bitDepth), ri ? String(ri.bitDepth) : null],
    ["Palette entries", i.paletteEntries === null ? "–" : String(i.paletteEntries), ri ? (ri.paletteEntries === null ? "–" : String(ri.paletteEntries)) : null],
    ["Unique colors", String(a.uniqueColors), r ? String(r.uniqueColors) : null],
    ["Bits per pixel", a.bitsPerPixel.toFixed(2), r ? r.bitsPerPixel.toFixed(2) : null],
    ["Ratio vs raw RGBA8", pct(a.rawRatio), r ? pct(r.rawRatio) : null],
    ["Interlaced", i.interlaced ? "yes" : "no", ri ? (ri.interlaced ? "yes" : "no") : null],
    ["ICC profile", i.hasIcc ? "yes" : "no", null],
    ["Transparency (tRNS)", i.hasTrns ? "yes" : "no", null],
    ["Animated (APNG)", i.isApng ? "yes" : "no", null],
    ["Chunks", i.chunks.join(", "), null],
  ];
  const head = r ? `<tr><th></th><th>Original</th><th>Result</th></tr>` : `<tr><th></th><th>Original</th></tr>`;
  $("d-info").innerHTML =
    head +
    rows
      .map(([k, v, rv]) => `<tr><td>${esc(k)}</td><td>${esc(v)}</td>${r ? `<td>${rv !== null ? esc(rv) : ""}</td>` : ""}</tr>`)
      .join("");
}

function renderPalette(a: Analysis): void {
  const i = a.info;
  $("d-palette-meta").textContent =
    i.paletteEntries !== null
      ? `Indexed palette: ${i.paletteEntries} entries · ${a.uniqueColors} unique colors`
      : `Direct color (${i.colorTypeName}) · ${a.uniqueColors} unique colors`;
  const max = Math.max(1, ...a.topColors.map((c) => c.count));
  $("d-top").innerHTML = a.topColors
    .map(
      (c) => `<div class="swatch-row">
        <span class="swatch" style="background:${checker(c.hex)}"></span>
        <span class="hex">${c.hex}</span>
        <span class="bar"><span style="width:${((c.count / max) * 100).toFixed(1)}%"></span></span>
        <span class="cnt">${compact(c.count)}</span></div>`,
    )
    .join("");
}

function checker(hex: string): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const a = parseInt(hex.slice(7, 9), 16) / 255;
  return `rgba(${r}, ${g}, ${b}, ${a.toFixed(3)})`;
}

function renderAlpha(a: Analysis): void {
  const total = a.opaquePixels + a.semiPixels + a.transparentPixels || 1;
  $("d-alpha-meta").textContent = `Opaque ${pct(a.opaquePixels / total)} · Semi-transparent ${pct(a.semiPixels / total)} · Transparent ${pct(a.transparentPixels / total)}`;
  drawBars(
    $("c-alpha") as HTMLCanvasElement,
    a.alphaHistogram.map((v, idx) => ({ label: String(idx * 16), value: v, color: "#6c8ebf" })),
  );
}

function renderRgb(a: Analysis): void {
  drawHistogram($("c-rgb") as HTMLCanvasElement, [
    { values: a.redHistogram, color: "#e05252" },
    { values: a.greenHistogram, color: "#4caf50" },
    { values: a.blueHistogram, color: "#4a7bd6" },
  ]);
}

function closeDetails(): void {
  state.selected = null;
  markSelectedRow();
  $("details").classList.add("hidden");
}

// ---------- wiring ----------

async function init(): Promise<void> {
  const optionIds = document.querySelectorAll<HTMLElement>(".options input, .options select");
  optionIds.forEach((el) => el.addEventListener("change", onOptionsChanged));

  $("address").addEventListener("submit", (e) => {
    e.preventDefault();
    void scan();
  });
  $("browse").addEventListener("click", () => void browse());
  $("process").addEventListener("click", () => void startProcess());
  $("cancel").addEventListener("click", () => void api.cancelProcess());
  $("apply").addEventListener("click", () => void applyChecked());
  document.querySelectorAll<HTMLElement>("#files th[data-sort]").forEach((th) =>
    th.addEventListener("click", () => toggleSort(th.dataset.sort as SortKey)),
  );
  $("show-all").addEventListener("click", () => {
    state.listFilter = null;
    renderRows();
  });
  $("select-all").addEventListener("click", () => {
    visibleRows().filter((r) => !r.skipReason).forEach((r) => state.included.add(r.path));
    renderRows();
  });
  $("select-none").addEventListener("click", () => {
    visibleRows().forEach((r) => state.included.delete(r.path));
    renderRows();
  });
  $("check-ready").addEventListener("click", () => {
    [...state.items.values()].filter((i) => i.status === "ready").forEach((i) => state.applyChecked.add(i.path));
    renderRows();
  });
  $("uncheck-all").addEventListener("click", () => {
    state.applyChecked.clear();
    renderRows();
  });
  $("remove-unchecked").addEventListener("click", () => {
    const paths = [...state.items.keys()].filter((p) => !state.applyChecked.has(p));
    void removeResults(paths);
  });
  $("d-remove").addEventListener("click", () => {
    if (state.selected) void removeResults([state.selected]);
  });
  $("d-close").addEventListener("click", closeDetails);
  $("rows").addEventListener("click", onRowsClick);

  document.addEventListener("keydown", (ev) => {
    if (ev.target instanceof HTMLInputElement || ev.target instanceof HTMLSelectElement) return;
    if (ev.key === "Escape" && !$("details").classList.contains("hidden")) {
      closeDetails();
      return;
    }
    if (ev.key !== "ArrowDown" && ev.key !== "ArrowUp") return;
    const rows = displayRows();
    if (rows.length === 0) return;
    const idx = rows.findIndex((r) => r.path === state.selected);
    const next = ev.key === "ArrowDown" ? Math.min(rows.length - 1, idx + 1) : Math.max(0, idx - 1);
    ev.preventDefault();
    void selectFile(rows[next].path);
    document.querySelector<HTMLElement>(`#rows tr[data-path="${CSS.escape(rows[next].path)}"]`)?.scrollIntoView({ block: "nearest" });
  });

  window.addEventListener("resize", () => {
    if (state.selected && !$("details").classList.contains("hidden")) void selectFile(state.selected);
  });

  await listen<ProcessProgress>("process-progress", (e) => setProgress(e.payload.done, e.payload.total));
  await listen<ProcessFinished>("process-finished", (e) => void onProcessFinished(e.payload));

  const view = await api.getJob().catch(() => null);
  if (view && view.items.length > 0) {
    state.items = new Map(view.items.map((i) => [i.path, i]));
  }
  syncOptionVisibility();
  updateButtons();
  setStatus("Ready. Enter a folder and press Scan.");
}

void init();
