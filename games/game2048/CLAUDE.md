# 2048

Self-playing 2048. AI: depth-limited expectimax over a fixed "snake" positional
heuristic, modeling both 2-tile (90%) and 4-tile (10%) spawns at chance nodes.

## Source layout

| File | Contents |
|------|----------|
| `src/lib.rs` | Pure game/AI logic: board transforms, `heuristic`, `slide`/`merge_row`, `expectimax`, `choose_dir` |
| `src/play.rs` | Rendering, animation (slide/merge/pop-bounce — see root CLAUDE.md's `RenderCache` section, which uses this game's animation as its "genuinely animates every frame" example), CLI |
| `src/main.rs` | Thin standalone binary — `game2048::play::start()` and nothing else |
| `src/bin/bench.rs` | Headless benchmark binary — runs N games with a seeded xorshift RNG, reports score percentiles + max-tile distribution |

## AI

`heuristic` scores a board via dot-product against a fixed `SNAKE` weight matrix
(corner-to-corner snake path, ratio 2.0 between adjacent cells: 32768→16384→...→1),
maximized over all 8 rotations/reflections (so the AI doesn't care which physical corner
the snake starts in). `expectimax`: player nodes pick the best of the 4 `slide`
directions; chance nodes model both a 2 (90%) and a 4 (10%) spawn at every empty cell,
averaged. `choose_dir` (`play.rs`/`bench.rs`'s entry point) uses **adaptive depth** —
depth 5 with ≤3 empty cells, depth 4 with 4-9, depth 3 otherwise (fewer empties = fewer
chance-node branches, so deeper search stays cheap when the board is nearly full, which
is also when search quality matters most).

## Current best config (verified against `lib.rs` as of this doc)

4-tile expectimax + adaptive depth, `SNAKE` r=2.0, no smoothness/free-tile/merge-bonus
terms (see "Ruled out" below) — this is exactly what's in `lib.rs` right now.

**200 games, seed=42** (`cargo run --release --bin bench -p game2048 -- 200 42`):

| Metric | Value |
|---|---|
| mean score | 65,762 |
| p50 / p75 | 60,772 / 78,932 |
| max | 156,700 |
| 4096+ rate | 71% (112×4096 + 30×8192) |
| speed | 248 moves/s |

Re-run this exact command (fixed seed) before/after any heuristic or search change —
don't compare against a different seed or game count, per
[[feedback_bench_methodology]] (compile variants to separate binaries, run in parallel,
fixed seed).

## Ruled out (don't re-try blind)

All measured against the pre-fourtile baseline (mean 57,352, same 200-game/seed=42
bench) unless noted:

- **Smoothness penalty** (absolute *or* α-relative-scaled, both forms) — always worse
  (mean dropped 39-50%). Smoothness and the snake gradient are anti-correlated: a steep
  gradient (exactly what snake rewards) *produces* rough rows, so any smoothness penalty
  fights the objective regardless of how it's scaled.
- **Free-tile bonus** (reward for keeping empty cells) — catastrophic at every weight
  tried (β 0.1→1.0, mean dropped 39-59%). Same failure shape as smoothness: the bonus
  term overwhelms the snake gradient's ability to distinguish good/bad positions.
- **Merge-potential bonus** — even more catastrophic (γ 0.1→1.0, mean dropped 81-88%).
  Directly rewards breaking the snake ordering to create merges, which is exactly what
  the snake heuristic exists to prevent.
- **Lower SNAKE r-value** (1.75/1.5/1.25 vs the r=2.0 baseline) — r=1.5 ties within
  noise, 1.75/1.25 both worse. Lower r weakens the row-0 gradient that drives aggression
  toward row-1 merges.
- **Fixed depth increase** (uniform 4 or 5 instead of the then-current fixed depth) —
  no score improvement, only slower (1.5-2.8×). A pure-snake heuristic doesn't get
  better guidance from extra depth alone — this is *why* adaptive depth (spend the depth
  budget only when the board is nearly full) was tried next and kept, see above.

**Not yet tried**: combining fourtile+adaptive-depth (already the shipped baseline) with
one of the ruled-out bonus terms *reintroduced at a much smaller scale* now that the
chance-node model is more accurate — none of the ruled-out attempts were re-tried after
fourtile landed. A terminal-state heuristic return (currently just
`heuristic(&board)` on game-over, same as any other leaf) could instead return `-∞` to
more strongly discourage losing lines, untested either way.

## Running

```bash
mise run run game2048                                 # native
mise run build-wasm game2048                            # WASM → dist/game2048/
cargo run --release --bin bench -p game2048 -- 200 42   # headless benchmark
```
