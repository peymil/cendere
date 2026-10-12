# TODO

## 1. Setup
- [ ] Node project (Vite + TypeScript) and Tauri 2 project in `src-tauri/`
- [ ] App icon, `.gitignore`, `LICENSE` (GPL-3.0-or-later)

## 2. Core PNG layer (Rust)
- [ ] Read IHDR header without decoding (dimensions, color type, bit depth, interlace)
- [ ] Chunk listing (metadata chunks, ICC, APNG detection)
- [ ] Decode to RGBA8 (palette, gray, 16-bit expanded)
- [ ] Encode RGBA8 and indexed PNG, keep ICC profile

## 3. Pipeline (Rust)
- [ ] Options model (serde, camelCase)
- [ ] Lossless pass with oxipng (strip, deinterlace, exact reduction, Zopfli)
- [ ] Alpha cleanup
- [ ] Resize: scale 1x-4x, custom W x H, keep aspect, premultiplied alpha
- [ ] Palette reduction with imagequant (auto / fixed count, dithering)
- [ ] Final lossless encode and "no gain" rule
- [ ] Unit tests for every step and for the order

## 4. Scan and analysis (Rust)
- [ ] Recursive scan, PNG signature check, symlinks skipped
- [ ] Thumbnails (cached)
- [ ] Analysis: palette, unique colors, alpha stats, RGB histogram, top colors (cached)

## 5. Process and Apply (Rust)
- [ ] Job with staging folder, progress events, cancel
- [ ] Discard result / discard job
- [ ] Apply: SHA-256 check, atomic replace, parallel writes, staging cleanup
- [ ] Remove staging on app exit and clean leftovers on start
- [ ] Tests for job flow and apply

## 6. Frontend
- [ ] Layout: address bar, options, file list, details, status bar
- [ ] Address bar with Browse and Scan
- [ ] Flat virtual file list with thumbnails, checkboxes, status column, keyboard navigation
- [ ] Options panel with enable toggles, defaults, validation
- [ ] Process, progress, cancel, stale-result banner
- [ ] Details panel: info, compression, palette, alpha, charts
- [ ] Compare wipe slider with zoom and pan
- [ ] Remove result, Remove unchecked, Apply with confirmation, result messages

## 7. Verification
- [ ] `cargo test` passes
- [ ] TypeScript check and production build pass
- [ ] Debug build runs; UI checked by screenshot

## 8. Delivery
- [ ] README (run, build)
- [ ] GitHub Actions build for Windows, macOS, Linux (not run locally)
