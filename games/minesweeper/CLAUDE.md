# Minesweeper

AI minesweeper, no player input. `games/minesweeper` is a thin binary (35-line
`main.rs`) — essentially all logic (board, solver, rendering, CLI, `run`/`run_headless`)
lives in `lib/minesweeper`, a separate workspace crate. Nothing else depends on
`lib/minesweeper`, so this doc covers both together.

Package name `minesweeper-game` with `[[bin]] name = "minesweeper"` — can't be named
`minesweeper` itself since that collides with the `lib/minesweeper` package in the same
workspace.

## Source layout

| File | Contents |
|------|----------|
| `games/minesweeper/src/main.rs` | `conf()`, native/WASM `main()` — just wires CLI args into `minesweeper::run`/`run_headless` |
| `lib/minesweeper/src/board.rs` | `GridKind`, `Cell`/`CellState`, `Phase`, `Board` (mine placement, `neighbors`, flood-fill `reveal`) |
| `lib/minesweeper/src/solver.rs` | `next_action` — constraint-satisfaction (subset-reduction) solver + probability fallback |
| `lib/minesweeper/src/lib.rs` | `CliArgs`, `run`/`run_headless`, all rendering (`draw_board`/`draw_square`/`draw_hex_grid`/HUD) |

## Square vs Hex

One `Board`/solver, `GridKind::Square` (26x20, 83 mines) or `GridKind::Hex` (18x15, 43
mines, flat-top hexagons, odd-column shifted down) picked at `Board::new`. `neighbors()`
branches on `kind` for adjacency (8-neighbor for Square, 6-neighbor hex offsets for
Hex) — every other board/solver method (`reveal`, `place_mines`, `next_action`,
`update_probs`) is grid-shape-agnostic, working purely off `neighbors()`. Rendering
branches per-kind in `draw_board` (`draw_square` vs `draw_hex_grid`).

`V` (`is_key_pressed(KeyCode::V)`) cycles `GridKind::cycle()` and rebuilds the board
immediately — same "restart now, not at next episode boundary" behavior as klondike's
`V`. `--variant <square|hex>` (native-only) pins the starting kind; default is Square.
On WASM, `initial_wasm_variant()` reads a `hcg_initial_variant_is_hex` JS import (wired
by `xtask::variant_query_bridge`) so a page loaded with `?variant=hex` starts in Hex —
this is how `static/minesweeper-hex/index.html`'s redirect stub (see below) lands
correctly. `V` still cycles from there like any other session.

## Solver (`solver.rs`)

Constraint-satisfaction, not probability-only: `build_constraints` turns each revealed
numbered cell into a `{hidden neighbor cells, remaining mine count}` constraint;
`reduce` derives new constraints from subset pairs (`A ⊆ B` → `B\A` must contain
`B.mine_count - A.mine_count` mines) until no more subsets are found. `next_action`
checks constraints for a certain-mine (`mine_count == cells.len()`) or certain-safe
(`mine_count == 0`) cell first: flag or open outright, no guessing needed. Only when no
constraint resolves does it fall back to guessing the globally-lowest `mine_prob`
hidden cell. `update_probs` (called after every real move, not inside the search) does
the same constraint pass to refresh every hidden cell's displayed probability — this is
what the board's color-coded mine-probability shading (`prob_color`) reads from.

## Retired game URLs

`games/minesweeper-square`/`games/minesweeper-hex` used to be two separate binaries;
merged into this one crate with the `V`-cycle. `static/minesweeper-square/index.html`
and `static/minesweeper-hex/index.html` are meta-refresh + `location.replace()` redirect
stubs (copied verbatim into `dist/` by `mise run deploy`) pointing at
`../minesweeper/` / `../minesweeper/?variant=hex` — `generate_index`'s "only counts a
`dist/` dir as a game if it has a `.wasm` file" filter automatically excludes these from
the homepage/sitemap. Reuse this same stub pattern (see `static/2048/index.html` for
another instance) for any future game rename/merge rather than inventing a new redirect
mechanism.

## Running

```bash
mise run run minesweeper                              # native, Square
mise run build-wasm minesweeper                        # WASM → dist/minesweeper/
target/release/minesweeper --variant hex --no-ui --once --debug
```
