# hcg — Hypercasual Games

Rust workspace of small games targeting native and WASM via macroquad. Each game lives in `games/<name>/` as a separate crate.

## Workspace layout

```
Cargo.toml          workspace root (resolver = "2", opt-level = "z" + lto for release)
mise.toml           task runner — primary interface for builds
games/
  snake/            one crate per game
dist/               WASM build output (git-ignored), served via python HTTP
```

## Task runner (mise)

All tasks take the game name as `$1` unless noted.

| Command | What it does |
|---------|-------------|
| `mise run run snake` | Native debug build + launch |
| `mise run build-wasm snake` | Release WASM → `dist/snake/` (fetches mq_js_bundle.js) |
| `mise run deploy` | Rebuilds all games into `dist/` |
| `mise run serve` | `python3 -m http.server 8080 --directory dist` |

WASM target: `wasm32-unknown-unknown`. **Not** wasm-bindgen — macroquad uses miniquad's own JS bundle.

## Adding a new game

1. `cargo new --bin games/<name>`
2. Add `"games/<name>"` to workspace `members` in root `Cargo.toml`
3. Add `macroquad = "0.4"` to the game's `Cargo.toml`
4. Copy `games/snake/index.html` and `games/snake/Trunk.toml` as templates; adjust binary name
5. `mise run run <name>` to test natively; `mise run build-wasm <name>` for WASM

## Trunk (per-game, for live-reload dev)

Each game has its own `Trunk.toml`. Run from inside `games/<name>/`:

```
trunk serve   # live-reloads HTML; Rust changes need manual retrigger
trunk build --release
```

Trunk pre-build hook compiles via `cargo build --target wasm32-unknown-unknown` and caches `vendor/mq_js_bundle.js`.

## WASM caveats

- `std::time::SystemTime::now()` **panics on WASM** — use `macroquad::miniquad::date::now() as u64` for timestamps/seeds.
- No filesystem access in WASM — avoid `std::fs`.
- `rand::srand(...)` must be called at startup to seed the RNG; quad-rand's default seed is fixed.
