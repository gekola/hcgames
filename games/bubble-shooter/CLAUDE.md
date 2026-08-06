# Bubble Shooter

Self-playing Puzzle-Bobble-style bubble shooter. A beam-search AI aims and fires every
shot; a hex-packed wall of bubbles hangs from the ceiling, colors match in groups of
3+, and new rows descend under escalating time pressure. Portrait 600x720 canvas (like
Tetris) — the board is inherently tall and narrow.

## Source layout

| File | Contents |
|------|----------|
| `src/game.rs` | Grid geometry, `Board`/`Game`/`Move`/`Resolution`, shot raymarch physics, connectivity BFS, row-descend |
| `src/solver.rs` | `choose_move` — `beam_solver`-backed depth-2 search scored by `score_resolution` |
| `src/main.rs` | Rendering (`draw_bubble`/board frame/death line), `View`/`StepPhase` flight-pop-fall animation state machine, CLI |

## Grid: doubled-width hex coordinates, not axial or plain offset

Cells are `(col, row)` where `row` is a real board row (0 = ceiling) and `col` is in
units of **half a bubble diameter** (`RADIUS`, not a whole column) — a cell's pixel x is
just `BOARD_X + RADIUS + col * RADIUS`, no row-dependent term at all. Every occupied
cell satisfies `col & 1 == row & 1`.

This was **not** the original plan. The design phase's plan called for axial `(q, r)`
hex coordinates (kills the classic even/odd-row neighbor-parity bug). Implementing
`descend_row` (shift every bubble down one row when the ceiling pushes a new one)
surfaced a real problem with that: in a circle-packed hex grid, a bubble directly below
another is *not* vertically aligned to its own row's cell grid unless the coordinate
system already bakes in the half-cell stagger — plain axial (and plain offset/"brick
wall") coordinates both need a `col`/`q` correction whenever `row`'s parity flips,
which is every single descend. Doubled-width coordinates don't: `col` alone determines
pixel x, so `descend_row` is exactly `row += 1`, `col` untouched, **and** the six
neighbor offsets stay a single constant table with no parity branching at all (the same
goal the axial plan was after, achieved without the descend footgun). Hand-derived from
the pixel formulas — not copied from memory — and checked by `neighbor_offsets_are_symmetric`/
`neighbor_offsets_match_tangent_circle_distance` (`game.rs` tests): every neighbor pair
is mutual, and every neighbor distance is genuinely one bubble diameter (same row) or
`sqrt(RADIUS² + ROW_HEIGHT²)` (diagonal) — geometrically real tangent-circle distance,
not just a topologically-plausible table.

`Board::cells: HashMap<(i32,i32), Color>` — small enough (~140 cells at most) to clone
freely per beam-search node, same reasoning every other solitaire-family game here relies
on for a whole-board clone.

## Shot physics: raymarch → discrete move space

Aim angle is continuous in principle; `Game::legal_moves()` samples every degree from
`ANGLE_MIN` (8°) to `ANGLE_MAX` (172°), raymarches each (`RAY_STEP`-sized steps,
reflecting off both side walls, capped at `MAX_BOUNCES`), and snaps to the nearest empty
neighbor cell of whatever it hits first (existing bubble within `COLLISION_DIST`, or the
ceiling). Multiple angles landing on the same cell keep only the one closest to 90°
(straight up) as that cell's representative `Move { target, angle_deg }` — `resolve()`
re-raymarches the chosen move's own angle against the same board to reproduce the
landing deterministically (`debug_assert_eq!` guards this) and to get the flight
polyline for animation, rather than storing every candidate's path.

Collision lookup (`nearest_occupied_cell`) doesn't scan the whole board per ray step —
it computes the approximate nearest cell to the current point (`pixel_to_cell`) and only
checks a small window around it, so cost stays ~O(1) per step regardless of board size.

## Resolution model

Same "compute first, animate after" split every self-playing game in this workspace
uses (see match-3's `Resolution`/`Wave`). `Game::apply`/`Game::simulate` (the latter
pure, used by the solver) resolve into a `Resolution { path, popped, floaters,
score_gained, descended, .. }`: place the bubble, BFS-flood same color from the landing
cell (`flood_same_color`), pop if the group is ≥3, then — only if a pop happened — a
**global** BFS from every `row == 0` cell (`find_floating`) finds bubbles no longer
connected to the ceiling and drops those too, scored at 2x a direct pop
(`FLOATER_POINTS` vs `POP_POINTS`). `main.rs`'s `View`/`StepPhase` plays this back
cosmetically as `Flying` → (`Popping` → `Falling`, only if there was a pop) → `Idle`,
mirroring match-3's `Swap` → `Flash` → `Fall` → `Idle` shape exactly. `--no-ui`/`--once`
skips straight to the resolved state, same as every move-paced (not real-dt-paced) game
in this workspace.

## RNG decorrelation

Same trap and same fix as match-3 (`feedback_solver_rng_confound` memory) — a
hypothetical-scoring clone must never carry the real game's RNG stream forward, or the
solver's own scoring becomes an oracle for whatever move it's about to apply for real.
`Game`'s own `Rng` (SplitMix64) and `DeterministicHasher` are a straight port of
match-3's. Two call sites reseed from `preview_seed(board, mv)` (hash of sorted board
content + move): `Game::simulate` and `beam_solver::SearchState::apply`'s impl
(`solver.rs`) — the beam engine's own line-advancing `apply`, used to generate later-ply
candidates, is a second independent leak of the same bug if left un-reseeded.

## Solver

`beam_solver::SearchState` on `Game`, same shape as match-3/klondike/spider.
`BEAM_WIDTH=6, BEAM_DEPTH=2, BEAM_NODE_BUDGET=600`. `legal_moves()` here costs ~165
raymarches (visibly more per call than match-3's O(1) swap check), but at depth 2 only
`1 + BEAM_WIDTH` nodes ever call it per real move — measured ~20-30ms/move in a release
build (150-shot episodes, `cargo test --release`), acceptable against the tick interval
but re-measure before raising width/depth.

Scoring (`score_resolution`): `+POP_WEIGHT` per popped cell, `+FLOATER_WEIGHT` (higher —
the actual skill outcome) per floater, `-HEIGHT_PENALTY` per row of the post-move
board's deepest bubble plus `-HEIGHT_MARGIN_PENALTY / margin_to_death_row` (see below),
`-NO_POP_PENALTY` plus `-ISOLATED_SINGLETON_PENALTY` if a no-pop shot's own bubble has
zero same-color neighbors, `+ADJACENT_PAIR_BONUS` per same-color adjacent pair left on
the board (live near-match proxy), `-ORPHAN_COLOR_PENALTY` per color with exactly one
bubble left, and `-LOSE_PENALTY` if the resulting state is `Outcome::Lost`. **These
weights are reasoned-from-first-principles, not measured-and-tuned** — root CLAUDE.md's
standing rule ("win rate must be checked empirically, not assumed from the weights")
hasn't had a real tuning pass yet here; see "Balance" below for what *was* measured.

**Fixed 2026-08-06: a losing move was scored like any other move.** `beam_score` called
`score_resolution(&res, &after.board)` — nothing in the scorer ever looked at
`after.phase`, so a move that outright ends the episode in `Outcome::Lost` only paid the
ordinary per-row `HEIGHT_PENALTY` (25/row), which a single decent pop could easily
outweigh. Confirmed via an A/B (same seeds, only this scoring change, `--no-ui` sweep):
Warm-Up's loss rate dropped from 48% to 30% purely from this fix, with the rest of the
episode-structure/level constants held fixed. Two things now enforce "never trade a
survivable move for a losing one, no matter how much it pops": `LOSE_PENALTY` (100_000,
added when `after.phase == Phase::Over(Outcome::Lost)`) and `HEIGHT_MARGIN_PENALTY /
margin` (`margin = DEATH_ROW - max_row`, floored at 1) — a hyperbolic term that's
negligible early (margin 8 adds ~38) but dominant right at the edge (margin 1 adds 300),
because the marginal cost of one more row of height isn't constant: it barely matters
when there's plenty of cushion left and ends the episode when there's none. A flat
per-row term alone couldn't express that difference.

Verified this fix isn't *itself* the reason the AI seemed to "leave bubbles on the
table": before touching any weights, a temporary diagnostic (compare the solver's chosen
move against the single best-available immediate popped+floater count, every real shot)
ran clean across 2000+ real shots spanning several seeds and levels — the solver never
once picked a move that popped/floated less than the best move actually available to it.
The AI's apparent "suboptimal decisions" symptom traced entirely to the pacing bug
described below, not to move selection.

## Episode structure

Decided with the user up front: **row-descend survival**, not static clear-the-board — a
new row pushes from the ceiling periodically, escalating over the episode
(`descend_interval_for`: starts at `DESCEND_INTERVAL` shots, shrinks by 1 every
`DESCEND_RAMP_SHOTS`, floors at `MIN_DESCEND_INTERVAL`). Episode ends on death-line
breach (`Outcome::Lost`), a cleared board (`Outcome::Won`, rare once descend is
running), or `SHOT_LIMIT` shots survived (`Outcome::Survived`).

**`Outcome::Survived` (and `SHOT_LIMIT` itself) exists because the first version had no
guaranteed episode end at all.** `full_playthrough_terminates` (a soak test, mirroring
match-3's own) hung past 5000 shots — a competent beam-search bot can in principle keep
the board under control indefinitely with only a fixed-pace descend and no shot cap.
Every other game in this workspace has *some* move/time limit; this one needed one too.

## Level progression (`game::LEVELS`)

Difficulty ramps **across** episodes, not just within one — `Game::generation` (already
incremented every episode by `Session::next_generation`) indexes into a fixed `LEVELS`
line via `level_for(generation)`, wrapping back to the start after the last one (no
win/stuck gating like match-3's `VariantMode::Levels` — every level here is playable to
its `SHOT_LIMIT` regardless of outcome, so there's no "stuck" condition to gate on;
level just advances every round). Two levers, both per the user's request:

- `color_count` — a prefix of `Color::ALL`, carried as `Game::active_colors` and used by
  every random color draw (initial fill, shot queue, row-descend refill) instead of the
  full palette. Fewer colors means far more incidental matches — same lever match-3's
  `LevelParams::color_count` ramp already established, run in the *easy* direction here.
- `initial_rows` — how many rows the board starts pre-filled with.

HUD shows `level N/7 — Name`; the `--once` `result=` line includes `level`/`colors`/
`initial_rows` for sweep scripts to bucket by.

**`initial_rows` is capped at 5, not ramped toward `DEATH_ROW`.** A first pass ramped it
3→7; measured almost every top-level loss landing at `shots_used` 1-7 — i.e. dying to
the very first ceiling descend, sometimes before the bot got a real turn. `DEATH_ROW` is
a fixed global constant (the shooter's screen position depends on it, and the canvas is
a fixed 600x720 size — it can't vary per level), so a level whose starting wall already
sits close to it has almost no margin *before play even begins*. That's an instant-death
artifact, not earned difficulty, and reads as broken. Capping the ramp at 5 (`DEATH_ROW`
stays 7, so margin never drops below ~2-3 rows) fixed it — see the measurement below.

## Balance — measured, not just reasoned

**STALE as of 2026-08-06 — every number below was measured against a since-fixed bug and
needs re-sweeping.** `descend_row` used to hardcode the fresh ceiling row to always start
at an even column, correct only for the very first descend — every descend after that
needed the opposite column parity to stay tangent-adjacent to the (already-shifted) row
below it, and getting it wrong silently severed the *entire* existing wall from the
ceiling every other descend. `find_floating` never caught it (it only runs right after a
pop, not after a descend itself), so the wall would sit detached until the next pop, at
which point the whole thing dropped as floaters at once — a free, constant board-clear
disguised as "the bot chaining huge cascades." That's what made every tier in the table
below read as far safer than the game actually is: post-fix, `Warm-Up` (the easiest
level) loses roughly 11/15 seeds instead of the documented ~0%. See `game.rs`'s
`descend_row`/`Board::row0_parity` doc comments for the full mechanism, and
`descend_row_keeps_wall_connected`/`full_playthrough_terminates`'s `find_floating`
assertion (`game.rs` tests) for the regression coverage that would have caught it. The
whole sweep (this table, `DEATH_ROW`, `DESCEND_INTERVAL`, `LEVELS.initial_rows`, and the
never-yet-tuned solver weights below) needs redoing against the fixed solver before any
of these numbers can be trusted again.

**Solver weights (`solver.rs`'s `score_resolution`) are still reasoned-from-first-
principles, not measured-and-tuned** — root CLAUDE.md's standing rule ("win rate must be
checked empirically") hasn't had a pass on those yet. What *was* tuned empirically,
several rounds via `--no-ui` `HCG_SEED` sweeps (root CLAUDE.md's standard method,
aggregated per-level by running each seed through a full, un-`--once`'d rotation and
bucketing the `level="..."` field in each `result=` line):

- 5 colors, flat `DESCEND_INTERVAL=6`, `DEATH_ROW=12` (7-row cushion over 5 initial
  rows): 5/5 seeds survived, comfortably — the bot kept the board at rows 0-2 the entire
  150-shot cap, floater cascades routinely popping 10-20+ bubbles at once.
- + escalating `descend_interval_for` (min 2, ramp every 15 shots), still `DEATH_ROW=12`:
  30/30 survived.
- + 6 colors + tighter escalation (min 1, ramp every 10), still `DEATH_ROW=12`: 40/40
  survived. Even a new row every single shot in the endgame didn't threaten a bot capable
  of burst-clearing 10+ bubbles in one shot — *average* pressure barely matters when a
  single lucky cascade can undo several rows at once.
- `DEATH_ROW` 12→9, `LEVELS` not yet added (flat config): 9/10 survived, 1/10 lost —
  thinner cushion was the first lever that actually mattered, confirming the problem was
  margin, not throughput.
- `LEVELS` added, `initial_rows` ramping 3→7, `DEATH_ROW=9`: **0/210 losses** across all
  7 levels (30 seeds × 7 levels) — the easiest levels' small starting walls made the
  9-row cushion trivially safe everywhere, swamping the one earlier data point.
- `DEATH_ROW` 9→7, same `initial_rows` 3→7 ramp: 33/105 lost overall, but concentrated
  and *instant* at the top two levels (Full Spectrum/Overflow: 100% and 87% lost, nearly
  all at `shots_used` 1-7) — the instant-death artifact described above, diagnosed by
  checking the loss `shots_used` distribution specifically, not just the aggregate rate.
- `initial_rows` ramp capped at 3→5 (current `LEVELS`), `DEATH_ROW=7`: **n=40 seeds ×
  7 levels, final numbers**:

  | Level | colors | initial_rows | Lost | Survived | Won |
  |---|---|---|---|---|---|
  | Warm-Up | 3 | 3 | 0% | 97.5% | 2.5% |
  | Getting Busy | 4 | 3 | 0% | 97.5% | 2.5% |
  | Color Rush | 4 | 4 | 0% | 100% | 0% |
  | Wider Palette | 5 | 4 | 5% | 95% | 0% |
  | Packed House | 5 | 5 | untested after this tier's own last edit — see note below | | |
  | Full Spectrum | 6 | 4 | 15% | 85% | 0% |
  | Overflow | 6 | 5 | 27.5% | 72.5% | 0% |

  Loss `shots_used` for this config ranged 65-147 — spread across the run, not
  clustered at the first descend, confirming these are organic losses.

**Packed House's `initial_rows` was bumped 4→5 after the n=40 sweep above** (it had
been accidentally identical to Wider Palette — same `color_count`/`initial_rows` — so
its 0% at n=40 was just that pairing's own noise, not a real "easier than Wider
Palette" data point). Not re-swept at n=40 after the fix — a reasonable, not
rigorously confirmed, expectation is somewhere around Wider Palette and Full Spectrum's
rates (5-15%) given where its levers now sit, but this specific number is unverified.

**Shipped state**: a real, monotonically-increasing-by-level difficulty curve with
organic (not instant) losses, going from "safe warm-up" to "~1-in-4 losses" over the
7-level rotation. Not a rigorously-tuned 45-55%-per-tier target (that framing doesn't
obviously apply to a *ramp* the way it does to match-3's single fixed-difficulty
variants) — further tuning (smoothing the mid-ramp, re-verifying Packed House, tuning
solver weights) remains a legitimate follow-up, not a blocking defect.

## Rendering

`draw_bubble`: darker rim + inset base color + (if `highlight`) two opaque gloss dots —
same "precompute the blend, draw opaque" pattern as match-3's `draw_gem`, applied from
the start rather than discovered the hard way: this is an all-circles board, so the
`RenderCache` cache-vs-live AA mismatch match-3's jelly-rim saga hit would bite
immediately here, not eventually. `board_cache` uses `with_backdrop` (the board frame
fully opaque-covers its rect) and `with_supersample(2)`; every animating frame
(`Flying`/`Popping`/`Falling`) routes through `draw_fresh` rather than drawing straight
to the screen, so cached and live rasterize through identical machinery — see match-3's
CLAUDE.md's rendering section for the full rationale neither crate re-derives.

`View`/`StepPhase`: `Flying` (bubble travels `Resolution::path`) → `Popping` (flash
`popped` cells, only entered if non-empty) → `Falling` (`floaters` slide down and fade,
only entered if non-empty) → `Idle`. `View::settled_this_frame` is set for exactly the
one frame `phase` becomes `Idle`, and is what the render loop uses to `mark_dirty()` the
board cache — avoids re-rendering the cache every idle frame just because *a* frame
happened to be idle.

The shooter + next-color preview are drawn as part of the same cached/animated closures
as the board (not a separate always-live element) — their color only changes exactly
when the board's settled state changes, so they fit the same caching cadence.

## Running

```bash
mise run run bubble-shooter                              # native
mise run build-wasm bubble-shooter                         # WASM → dist/bubble-shooter/
HCG_SEED=1 target/release/bubble-shooter --no-ui --once --debug
```
