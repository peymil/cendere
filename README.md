# Cendere

Cross-platform desktop app (Windows, macOS, Linux) that optimizes PNG files in bulk.

- Scan a folder recursively; every PNG is listed in one flat table with thumbnails.
- Pick the steps you want; the app always runs them in a fixed order:
  1. Lossless pass (oxipng)
  2. Clear faded alpha (thresholds, default 8 / 247)
  3. Resize (scale 1x-4x where original = 4x, or custom W x H)
  4. Palette reduction (imagequant; auto quality 50-85, or fixed 2-256 colors)
  5. Final lossless encode
- Process writes results to a staging folder. Compare each result with the original (wipe slider, zoom, pan), remove what you do not want, then Apply to overwrite the originals with the same file name.
- Staged results are deleted after Apply, on Remove, and on exit.

Skipped: APNG (animated PNG), symlinks, images over 1 GB decoded.

## Stack

Tauri 2 (Rust backend, `src-tauri/`) with TypeScript + Vite frontend (`src/`).
Crates: png, oxipng, imagequant, fast_image_resize, rayon, walkdir, sha2, base64, rustc-hash, serde, anyhow.

## Develop

Requirements: Node.js 22+, Rust stable, and the Tauri prerequisites for your OS
(see https://tauri.app/start/prerequisites/).

```sh
npm install
npm run tauri dev
```

## Test

```sh
npm run build                                   # TypeScript check + production build
cargo test --manifest-path src-tauri/Cargo.toml # Rust unit tests
```

## Build

```sh
npm run tauri build
```

Installers are written under `src-tauri/target/release/bundle/`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE). imagequant is GPL-licensed, so the app is distributed under the same terms.
