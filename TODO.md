# TODO

## 1. Setup
- [x] Node project (Vite + TypeScript) and Tauri 2 project in `src-tauri/`
- [x] App icon, `.gitignore`, `LICENSE` (GPL-3.0-or-later)

## 2. Core PNG layer (Rust)
- [x] Read IHDR header without decoding (dimensions, color type, bit depth, interlace)
- [x] Chunk listing (metadata chunks, ICC, APNG detection)
- [x] Decode to RGBA8 (palette, gray, 16-bit expanded)
- [x] Encode RGBA8 and indexed PNG, keep ICC profile

## 3. Pipeline (Rust)
- [x] Options model (serde, camelCase)
- [x] Lossless pass with oxipng (strip, deinterlace, exact reduction, Zopfli)
- [x] Alpha cleanup
- [x] Resize: scale 1x-4x, custom W x H, keep aspect, premultiplied alpha
- [x] Palette reduction with imagequant (auto / fixed count, dithering)
- [x] Final lossless encode and "no gain" rule
- [x] Unit tests for every step and for the order

## 4. Scan and analysis (Rust)
- [x] Recursive scan, PNG signature check, symlinks skipped
- [x] Thumbnails (cached)
- [x] Analysis: palette, unique colors, alpha stats, RGB histogram, top colors (cached)

## 5. Process and Apply (Rust)
- [x] Job with staging folder, progress events, cancel
- [x] Discard result / discard job
- [x] Apply: SHA-256 check, atomic replace, parallel writes, staging cleanup
- [x] Remove staging on app exit and clean leftovers on start
- [x] Tests for job flow and apply

## 6. Frontend
- [x] Layout: address bar, options, file list, details, status bar
- [x] Address bar with Browse and Scan
- [x] Flat virtual file list with thumbnails, checkboxes, status column, keyboard navigation
- [x] Options panel with enable toggles, defaults, validation
- [x] Process, progress, cancel, stale-result banner
- [x] Details panel: info, compression, palette, alpha, charts
- [x] Compare wipe slider with zoom and pan
- [x] Remove result, Remove unchecked, Apply with confirmation, result messages

## 7. Verification
- [x] `cargo test` passes
- [x] TypeScript check and production build pass
- [x] Debug build runs (launch OK, no panic); UI screenshot NOT captured: macOS screen-recording permission denied for screencapture

## 8. Delivery
- [x] README (run, build)
- [x] GitHub Actions build for Windows, macOS, Linux (workflow file added; not run locally)
