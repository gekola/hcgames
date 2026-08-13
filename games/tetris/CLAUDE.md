# Tetris

Self-playing classic 10x20 Tetris, `beam_solver`-driven (depth 2). Own narrower 600x720
canvas (not the shared 900x720 default) since the board is inherently tall and narrow —
see `xtask::native_size`/`max_fit_scale`'s `"tetris"` arms.

## Source layout

| File | Contents |
|------|----------|
| `src/game.rs` | `Piece`, `Board`, rotation-shape derivation, `Game` (rules/state), scoring |
| `src/generator.rs` | `PieceGenerator`/`GenMode` — the 4 piece-randomizer algorithms |
| `src/solver.rs` | `beam_solver::SearchState` impl + the board-evaluation heuristic |
| `src/lib.rs` | `Session`/`View`/`FallAnim`, `VariantMode` (piece-gen mode cycle), CLI, rendering |
| `src/main.rs` | Thin standalone binary — `tetris::start()` and nothing else |

## Piece generation (`generator.rs`)

`GenMode` matches the RNG shape of real Tetris implementations, not one made-up
distribution — `V`-cycles like other games' variant switches (`VariantMode::Auto`
rotates all 4 by generation, same pattern as Sudoku's difficulty cycle):

| Mode | Behavior |
|---|---|
| `Bag7` | Modern Guideline: shuffle all 7 into a bag, deal one-by-one, reshuffle — every piece exactly once per 7 |
| `Classic` | 1989 NES-style: uniform, single reroll on immediate repeat — can still drought (esp. `I`) |
| `Tgm` | Arika Grand Master: reroll up to 4x against the last-4-piece history; never opens on `S`/`Z` |
| `Memoryless` | Pure uniform, no history — can drought/flood, kept specifically to be behaviorally distinct from the other three |

## Rotation shapes (`game.rs`)

Derived generically, not hand-transcribed: one hardcoded spawn-orientation shape per
piece (`Piece::base_shape`) plus a standard `(x,y) -> (size-1-y, x)` rotate-in-box
transform (`rotate_cw`) applied up to 4x, deduplicated by sorted-cell-set comparison
(`rotation_states`). No wall-kick tables — the solver enumerates every (rotation,
column) pair and hard-drops fresh from the top every time, never rotating a piece
already resting against neighbors. 3 unit tests cover rotation-count-matches-symmetry,
4-connectivity, and 4-rotations-return-to-start.

## Solver (`solver.rs`)

`BEAM_WIDTH = 12`, `BEAM_DEPTH = 2` (current piece + one known-next lookahead —
`Game::queue`/`LOOKAHEAD` keeps real pieces pre-generated so `SearchState::apply` stays
deterministic during search, same "pre-shuffled deck" trick as Klondike/Spider),
`NODE_BUDGET = 8_000`. Heuristic is Yiyuan Lee's widely-reused GA-tuned one-piece weight
set, scored on the *resulting* board per placement (not a delta):

```
W_AGGREGATE_HEIGHT = -0.510066
W_LINES_CLEARED    = +0.760666
W_HOLES            = -0.35663
W_BUMPINESS        = -0.184483
```

## Animation (`lib.rs`)

`Game::apply` places pieces and clears lines instantly (pure/discrete — headless mode
is a plain tight loop, no virtual-dt stepping). `FallAnim::pose()` fakes a real
multi-beat play-through cosmetically: hold at spawn (`SPAWN_ROW = -2`, 2 rows above the
visible board) → instant-snap rotate while sliding to the target column and partially
descending (`ROTATE_FRAC`/`SLIDE_FRAC`) → accelerating hard-drop the rest of the way.
`board_cache` (`RenderCache`) covers the locked board only (no text, so no font-atlas
prewarm needed for it) — the falling piece and line-clear flash stay live per-frame
draws on top, same split as klondike/spider's card table.

## Gotchas

- `gen` is a reserved keyword since the 2024 edition (future generator-block syntax) —
  `let gen = ...` fails to compile. Named `piece_gen` throughout instead.
- `BOARD_Y = 128` (not a smaller value) is deliberate clearance so the falling piece's
  cosmetic spawn point (`SPAWN_ROW = -2`) doesn't draw through the "TETRIS" title text.
- Interior-only grid lines (`1..W`/`1..H`, not `0..=W`/`0..=H`) in `draw_board_static` —
  drawing a grid line directly on the board's own outer edge partially overwrites the
  border rect there, asymmetrically enough between edges to read as a visibly thinner
  border on one side.
- The side panel (next-piece box + stat lines) gets the same bordered
  container/full-height treatment as the board itself — without it, the panel's actual
  content only fills part of its height, leaving bare background below that reads as
  "more empty space" next to the board.
- The "NEXT" panel shows only `queue.front()` — 1 piece, the classic (NES-era) Tetris
  convention, not modern guideline Tetris's 3-6-deep queue. `game::LOOKAHEAD` (3) is
  unrelated and unchanged: it's retained depth for the solver's `BEAM_DEPTH = 2`
  lookahead, not display count — don't conflate the two if either changes again.

## Running

```bash
mise run run tetris                                    # native
mise run build-wasm tetris                              # WASM → dist/tetris/
HCG_SEED=1 target/release/tetris --no-ui --once --variant bag7 --debug
```
