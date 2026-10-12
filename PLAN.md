# Cendere: PNG optimizer

Desktop app (Windows, macOS, Linux) that optimizes PNG files and overwrites them in place.

## Stack
- Tauri 2: Rust backend, TypeScript + Vite frontend (no UI framework).
- Rust crates: `png`, `oxipng` (lossless), `imagequant` (palette), `fast_image_resize` (resize), `rayon`, `walkdir`, `sha2`.
- License: GPL-3.0-or-later (required, because imagequant is GPL and the project is open source).

## Workflow
1. **Scan**: address bar gets a directory, Scan lists every PNG under it (subfolders included) as a flat list.
2. **Select**: checkbox per file. All files are selected after a scan.
3. **Options**: user enables optimizations and sets parameters. The app runs them in a fixed order.
4. **Process**: writes results to a staging folder. Original files are not touched.
5. **Review**: each row shows the size result. Click a file for details and a compare slider.
   Checked = will be applied. Remove / Remove unchecked = drop staged results.
6. **Apply**: confirm, then atomically overwrite checked files (same path, same name). Staging is deleted.

Changing any option marks results stale. Apply is disabled until Process runs again.

## Pipeline (fixed order)
1. **Lossless pass** (oxipng): strip metadata (keeps color profile), remove interlacing, exact color-type and bit-depth reduction, optional Zopfli.
2. **Alpha cleanup**: alpha <= low becomes fully transparent with RGB zeroed; alpha >= high becomes opaque. Defaults 8 / 247.
3. **Resize**: 1x / 2x / 3x / 4x, where the original is 4x (so 1x = 25%, 4x = 100%), or custom W x H.
   Custom with "keep aspect" fits inside the W x H box. Premultiplied alpha. Lanczos3 or Nearest.
4. **Palette reduction** (libimagequant): Auto (smallest palette that reaches quality 85, minimum 50) or fixed 2-256 colors. Dithering off by default.
5. **Final lossless encode** (oxipng). If the result is not smaller and no resize happened, it is marked "no gain" and unchecked.

Steps 2-4 work on decoded 8-bit RGBA. A 16-bit input becomes 8-bit when one of them runs.

## Details panel (selected file)
- Image: dimensions, color type, bit depth, interlacing, ICC profile, metadata chunks, APNG.
- Compression: file size, bits per pixel, ratio vs raw RGBA8, result size and saving.
- Color palette: palette entries, unique colors, top 12 colors (bar chart).
- Alpha: opaque / semi-transparent / transparent share, alpha histogram.
- Color distribution: R, G, B histogram.
- Size before / after bar chart (after Process).
- Compare: wipe slider (original left, result right), wheel zoom, drag pan, double-click reset.

## Safety
- Originals change only on Apply.
- Apply writes the staged bytes verbatim, so the result is exactly what was reviewed.
- Apply checks the file still matches its SHA-256 from Process. If not, it is skipped and reported.
- Atomic replace: temp file in the same folder, fsync, copy permissions, rename.
- Staging is deleted after Apply, Remove, new scan, and app exit.
- Per-file errors are reported; the batch continues.
- Skipped: animated PNG (APNG), symlinks, images over 1 GB decoded.

## Not included (by request)
- Undo, redo, timeline.
- Several output sizes in one run (one size per run, same file name).

## Decisions made without a question
- Browse button next to the address bar (fills the path; Scan lists files).
- Confirmation dialog before Apply.
- Process replaces the previous review results.
- List shows file name and thumbnail; full path is in the details panel.
- UI in English.

## Testing
- Rust unit tests: lossless pixel identity, alpha rules, resize sizes, palette color limit, scan rules, apply and hash check, staging cleanup.
- Frontend: type check and production build.
- Launch the app and check the UI with a screenshot (macOS).
- Windows and Linux: CI workflow included, not run locally.
