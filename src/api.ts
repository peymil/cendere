import { invoke } from "@tauri-apps/api/core";

export interface ScanItem {
  path: string;
  name: string;
  size: number;
  width: number;
  height: number;
  colorType: string;
  bitDepth: number;
  skipReason: string | null;
}

export interface PngInfo {
  width: number;
  height: number;
  bitDepth: number;
  colorType: number;
  colorTypeName: string;
  interlaced: boolean;
  isApng: boolean;
  hasIcc: boolean;
  hasTrns: boolean;
  paletteEntries: number | null;
  chunks: string[];
}

export interface ColorCount {
  hex: string;
  count: number;
}

export interface Analysis {
  path: string;
  fileSize: number;
  info: PngInfo;
  bitsPerPixel: number;
  rawRatio: number;
  uniqueColors: number;
  opaquePixels: number;
  semiPixels: number;
  transparentPixels: number;
  alphaHistogram: number[];
  redHistogram: number[];
  greenHistogram: number[];
  blueHistogram: number[];
  topColors: ColorCount[];
  chunkSizes: ChunkSize[];
}

export interface ChunkSize {
  kind: string;
  bytes: number;
}

export type ItemStatus = "ready" | "noGain" | "failed";

export interface JobItem {
  path: string;
  status: ItemStatus;
  sourceSize: number;
  resultSize: number;
  stagedPath: string | null;
  resized: boolean;
  notes: string[];
  error: string | null;
}

export interface JobView {
  id: string | null;
  running: boolean;
  items: JobItem[];
}

export interface ApplyReport {
  applied: string[];
  skipped: { path: string; reason: string }[];
}

export interface Options {
  lossless: { enabled: boolean; level: number; stripMetadata: boolean; removeInterlace: boolean; zopfli: boolean };
  alpha: { enabled: boolean; low: number; high: number };
  resize: {
    enabled: boolean;
    mode: "scale" | "custom";
    scale: number;
    width: number;
    height: number;
    keepAspect: boolean;
    filter: "lanczos3" | "nearest";
  };
  palette: { enabled: boolean; mode: "auto" | "fixed"; colors: number; dithering: boolean };
}

export interface ProcessProgress {
  done: number;
  total: number;
}

export interface ProcessFinished {
  id: string;
  cancelled: boolean;
  ready: number;
  noGain: number;
  failed: number;
}

export const api = {
  scanDir: (dir: string) => invoke<ScanItem[]>("scan_dir", { dir }),
  analyze: (path: string) => invoke<Analysis>("analyze_png", { path }),
  thumbnail: (path: string, max: number) => invoke<string>("thumbnail", { path, max }),
  imageDataUrl: (path: string) => invoke<string>("image_data_url", { path }),
  startProcess: (paths: string[], options: Options) => invoke<string>("start_process", { paths, options }),
  cancelProcess: () => invoke<void>("cancel_process"),
  getJob: () => invoke<JobView>("get_job"),
  discardResults: (paths: string[]) => invoke<void>("discard_results", { paths }),
  discardAll: () => invoke<void>("discard_all"),
  applyResults: (paths: string[]) => invoke<ApplyReport>("apply_results", { paths }),
};
