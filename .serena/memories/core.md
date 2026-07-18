# EasyMill — Core

Gerber RS-274X → 3 PNGs → G-code (Fanuc-style) PCB milling desktop app. Iced 0.14 GUI + Rust.

## Source Map

| Path | Role |
|------|------|
| `src/main.rs` | Iced app: `AppState`, `update()`, `view()`, `subscription()`, `theme()` |
| `src/lib.rs` | Library root, re-exports `PngLayerResults` |
| `src/gerber.rs` | Gerber RS-274X + Excellon parser, lyon tessellation, image rasterizer |
| `src/conversion.rs` | Full pipeline: Gerber→3 PNGs → distance transform → contour vectorization → offset toolpaths → G-code |
| `src/logging.rs` | Tracing init via `OnceLock` (idempotent) |
| `src/ui/widgets.rs` | All UI widgets using `super::palette` / `super::styles` (TokyoNight) |

## Known Issues (compile-breaking)
- `src/ui/mod.rs` does **not exist** — the directory has only `widgets.rs`
- `src/ui/widgets.rs` imports `super::palette` and `super::styles` — these require `src/ui/mod.rs` defining those submodules
- The project **will not compile** until `src/ui/mod.rs` exists

## Pipeline
`.gbr/.gtl/.gbl/.drl` → lyon tessellation → 3 aligned PNGs (traces/drills/outline) → distance transform → contour vectorization → offset toolpaths → Fanuc-style G-code

## State Machine
Each pipeline stage: `StepState` (`Idle → Ready → Running → Complete`).
Progress via `Arc<AtomicU32>` + 100ms poll subscription.

## Key Defaults
1000 DPI, 0.4mm tool, 300mm/min feed, 120mm/min plunge, 12000 RPM, 25M pixel cap.

## Test Fixtures
- `test_files/inputs/gerber.zip` — input for standalone binary `test_gerber_direct.rs`
- `test_files/expected/png/` — reference PNGs for visual diff
- Standalone test binary: `test_gerber_direct.rs` (not a `#[cfg(test)]` module, but an independent binary)

## Vendored Repos
- `repos/gerber2png/` — upstream reference
- `repos/mods/` — upstream reference
