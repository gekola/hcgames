# Sudoku

Self-playing Sudoku. No search/scoring — a fixed technique-escalation order plays it
the way a human with a pencil does, one deduction at a time, watchable via the
candidate pencil-marks and a technique-colored fill.

## Source layout

| File | Contents |
|------|----------|
| `src/game.rs` | `Game`, grid/candidate bitmask helpers, `generate_solved_grid`, `dig`/`carve`, `Difficulty` |
| `src/solver.rs` | `Solver::choose_move` — the technique escalation order |
| `src/main.rs` | `DigitMetrics`, `VariantMode`, render loop, CLI |

## Why no `beam_solver`

Sudoku's "AI" is a fixed technique-escalation order, not a search among competing
moves — nothing to score. `Solver::choose_move` just tries each `find_*` fn in order
and returns the first hit (see root CLAUDE.md's "Self-playing solver games" section for
this framing in general). Order, in `solver.rs`:

1. `find_naked_single` — a cell with exactly one candidate left.
2. `find_hidden_single` — a digit with exactly one legal cell left within some row/col/box.
3. `find_locked_candidate` — pointing (a box's remaining cells for a digit share a
   row/col, eliminate that digit from the rest of the row/col) and box-line reduction
   (the reverse: a row/col's remaining cells for a digit share a box). One elimination
   per call (`Move::Narrow`), not a whole batch — each deduction stays individually
   visible rather than being applied silently.
4. `find_guess` — last resort: the emptiest cell (fewest remaining candidates), digit
   read straight from `game.solution`.

**"Guess" isn't real backtracking.** `Game::generate_solved_grid` produces the full
solved grid *before* carving, so when no logical technique applies, the solver just
places the already-known-correct digit — always right by construction, since the
puzzle has a verified-unique solution. No undo/backtrack UI needed.

## Puzzle generation (`game.rs`)

`generate_solved_grid` (randomized recursive fill) → `carve(solution, difficulty)`:

- `dig` removes clues in shuffled order, keeping a removal only if `solve_count(_, limit:
  2) == 1` (still uniquely solvable). A single sweep plateaus in the low-to-mid 20s
  (removing cell A can unblock cell B an earlier sweep already passed over), so `dig`
  repeats sweeps until either `min_clues` is hit or a whole sweep removes nothing.
- `carve` runs `dig` `Difficulty::carve_attempts()` times (fresh shuffle order each
  time) and keeps the sparsest result. Only `Expert`/`Master` retry (4x for Master, 1x
  otherwise) — a single dig's plateau lands well above every target below `Hard`, so
  retries only matter once the target is *at* or past that plateau.

| Difficulty | `min_clues` | `carve_attempts` |
|---|---|---|
| Easy | 40 | 1 |
| Medium | 32 | 1 |
| Hard | 26 | 1 |
| Expert | 22 | 1 |
| Master | 17 (lowest possible for a unique grid) | 4 |

`V`-cycles Easy→Medium→Hard→Expert→Master→Auto (`VariantMode`, `Auto` rotates by
`generation % 5`) — same pattern as Klondike's Draw-1/Draw-3/Auto.

## Rendering

`DigitMetrics::compute()` measures all 9 digit glyphs at both sizes the board ever
draws (filled-cell 34px, candidate pencil-mark 15px) once at startup instead of calling
`measure_text` per cell per frame — with up to 81 cells × 9 candidates that was ~700
redundant layout calls/frame. Candidates drawn in `draw_candidates` read straight from
`game.candidates` — the same bitmask the solver reads to pick moves, so what's on
screen is always exactly what's driving the next decision, not a separate display-only
computation. Placed digits are color-coded by which technique filled them
(`technique_color`), with a legend in the HUD.

Root CLAUDE.md's RenderCache note already covers that this board's cache
(`board_cache`) is constructed once and never rebuilt, since it doesn't depend on
screen size — nothing further game-specific there.

## Running

```bash
mise run run sudoku                                # native
mise run build-wasm sudoku                          # WASM → dist/sudoku/
target/release/sudoku --no-ui --once --variant master --debug
```
