# Match-3

Self-playing match-3 (Candy Crush-style swap-and-cascade), greedy 1-ply AI. Board 8x8,
6 colors, each color with its own silhouette (not just a color-coded fill). Directory/
package name has the dash; `xtask::title` renders it "Match 3".

## Source layout

| File | Contents |
|------|----------|
| `src/game.rs` | `Board`/`Tile`/`Special`/`Color`, `Game`, `resolve()` (swap → cascade), board gen |
| `src/solver.rs` | `choose_move` — greedy 1-ply eval over `Game::simulate` |
| `src/main.rs` | `GemShape`/`draw_gem`/`draw_tile`, `View`/`StepPhase` animation state machine, CLI |

## Why no `beam_solver`

Unlike Klondike/Spider/Tetris, there's no "known next piece" to search a second ply
into — what a swap reveals depends on where gravity happens to refill from, and an 8x8
board already offers ~100 legal swaps per move, so a second ply multiplies branching
factor rather than sharpening a small candidate set. `solver::choose_move` enumerates
`Game::legal_moves()`, resolves each via `Game::simulate` (pure, doesn't mutate `Game`),
and picks the highest-scoring `Resolution` by `solver::score_resolution`. See CLAUDE.md
(root)'s "Self-playing solver games" section for this as a named 3rd solver shape.

## Goal variants

`V`-cycles like Tetris's piece-gen modes (`VariantMode::Auto` rotates by
`generation % 4`, see `main.rs`):

| Variant | Goal | Paced by |
|---|---|---|
| `Score` | reach `SCORE_TARGET` | `SCORE_MOVE_LIMIT` moves |
| `Jelly` | clear all `Board::jelly` layers | `JELLY_MOVE_LIMIT` moves |
| `Ingredients` | get `INGREDIENTS_TARGET` `Tile::Ingredient` tiles to row `H-1` via gravity only | `INGREDIENTS_MOVE_LIMIT` moves |
| `Mystery` | clear every goal in `Game::mystery_goals` (1-3 colors, each own target — "hit all to win"; HUD names it "color hunt") | `MYSTERY_MOVE_LIMIT` moves |
| `Timed` | no move limit | real countdown, `Game::tick_time(dt)` called from `amain` directly (time-paced, not move-paced — the only variant not driven through `Game::apply`) |

**`Mystery`** (todo backlog item #4's "candy-order" goal — collect N of a specific
color, generalized to a small *combination* of simultaneous color targets) needed no new
`Tile`/board-gen: `Resolution::color_cleared: [u32; 6]` is computed unconditionally in
`resolve()` alongside `jelly_cleared`/`ingredients_collected` (indexed via `Color::index`),
so `Game::apply` just loops `self.mystery_goals` adding `res.color_cleared[goal.color
.index()]` to each — same mechanism as `Ingredients` reading `ingredients_collected`, just
per-goal instead of singular. `gen_mystery_goals` (called from `Game::new`) rolls 1-3
distinct colors (`MYSTERY_MAX_COLORS`) each with its *own* randomly-picked target from
`MYSTERY_TARGET_RANGES[count - 1]` — "various targets" per design, not one shared number.
Win condition (`update_phase`) is a plain `.all(|g| g.collected >= g.target)`. Not part of
`LEVELS` (yet) — `Game::new_level` sets `mystery_goals: Vec::new()` for every level, same
"carried but unused" shape as `ingredients_target` outside `Variant::Ingredients`. No
`Variant`-to-struct architecture change was needed for this (the todo doc's harder
"multi-goal-level" idea — combining goals *across different variants*, e.g. jelly AND
ingredients in one level — is a different, still-unbuilt ask; a `Mystery` episode's
multiple goals are all the same kind, just different colors/targets, which fits inside
one `Vec` field cleanly).

Solver: `solver::MYSTERY_WEIGHT` (300, same magnitude as `JELLY_WEIGHT`) per cell of *any
not-yet-completed* goal's color cleared (filtering out completed goals, so clears of an
already-finished color don't keep diluting the score once there's more than one goal
active). `Jelly`'s `JELLY_ENDGAME_*` dominance-threshold trick was tried here too and
**measurably didn't help**: an ablation (identical weight, endgame bonus zeroed) produced
byte-identical win/loss outcomes across 450 sweep seeds (back when this was still a single
fixed goal). Diagnosis: unlike jelly (any match landing on a jelly cell clears it, so
there's a real "pass up an available jelly-clear for a bigger `COMBO_BONUS`" failure
mode), an ordinary 3-match of a *target color specifically* already scores `3 ×
MYSTERY_WEIGHT = 900` in one shot — close enough to `COMBO_BONUS` (1200) that the
near-miss-losing-the-comparison pattern `Jelly` had just doesn't arise here. Dropped
rather than shipped as inert complexity; don't re-add without new evidence.

**Multi-goal balance surprised expectations going in — naive "split the single-goal
target across N colors" made 2-3 goal episodes trivially easy, not harder.** First pass
divided the single-goal target (31) down per goal count (e.g. ~15 for 2 colors, ~10 for
3) on the assumption that more simultaneous goals is more work. Measured instead: count=2
at 98.8% win rate, count=3 at 100% (n=150 each) — because ordinary, undirected play
already clears roughly equal amounts of *every* color over a game's cascades, so 2-3
colors out of the 6 on the board get satisfied largely as a side effect of playing at
all, regardless of solver intent; requiring *more* colors doesn't add difficulty the way
requiring *more of one* color does. Fix: per-goal targets for count=2/3 need to stay
close to the *single*-goal target's magnitude, not shrink proportionally — final
`MYSTERY_TARGET_RANGES` are `[(28,34), (24,30), (22,28)]` (count 1/2/3), all similar
order of magnitude despite covering different color counts. Tuned across three disjoint 450-seed ranges (1350 seeds total, `--no-ui --once --variant
mystery`, bucketed by goal count from the `mystery_goals=[...]` field in the `result=`
line, since the count itself is randomly rolled per episode): count=1 53.0% (224/423),
count=2 49.4% (238/482), count=3 54.8% (244/445) — all inside the 45-55% band. Don't
re-derive the naive proportional-split approach without re-checking this; it's a real,
measured trap, not a hunch.

Constants live at the top of `game.rs`. Win rate must be checked empirically, not
assumed from the heuristic weights — `--no-ui --once --variant X` across a range of
`HCG_SEED` (see root CLAUDE.md's "Native CLI flags"), targeting ~45-55% (winnable but
not a foregone conclusion, matching this repo's other self-playing games' feel). This
is how `SCORE_TARGET`/`INGREDIENTS_MOVE_LIMIT`/`solver::INGREDIENT_PROGRESS_WEIGHT` were
tuned — re-check any change to board gen, targets, or solver weights the same way.

**Exclude reshuffled episodes from win-rate A/B comparisons.** `Game::apply`'s
deadlock-safety-net reshuffle (see `Resolution model` below) mutates the board
mid-episode for reasons unrelated to solver skill — including it can muddy a before/after
comparison. `Game::reshuffles` counts how many times it fired this episode, and the
`--once` `result=` line prints it (`reshuffles=N`); filter with `grep reshuffles=0` (rare
in practice — ~2% of episodes in an `Ingredients` seed sweep) before trusting a close
win-rate delta. Disabling reshuffle outright for validation runs was considered and
rejected: it's there so the solver never faces a truly-zero-legal-moves board, so turning
it off risks an unhandled stuck state rather than just producing a "harder" episode —
excluding-after-the-fact is the safe version of the same idea.

**Reshuffle scope stays whole-board — closed, not revisited.** A narrower
"partial reshuffle limited to one stuck column" was floated as a possible fix for
`Ingredients` losses. Decided against: whole-board reshuffle-on-deadlock is the
genre-standard bailout (players of this kind of game already expect it), so narrowing it
would be a rules change with no established precedent to justify it, for a mechanism that
only fires in ~2% of episodes anyway. Don't re-propose without new evidence it's actually
costing win rate.

**`HCG_SEED` reproducibility gotcha**: `find_matches_with_spawns`' tie-break for *which*
of several simultaneous L/T-shape matches spawns the `Wrapped` tile used to read directly
off a `HashSet<Pos>`'s iteration order (`.next()`/`.find()`) — `std`'s default hasher is
randomly seeded per process, so two separate runs of the *same* `HCG_SEED` could pick a
different tie-break and diverge for the rest of the episode, silently breaking the
"`HCG_SEED` reproduces a run" contract root CLAUDE.md documents for every game. Fixed by
sorting the candidates before picking (`wrapped_sorted` in `find_matches_with_spawns`) —
arbitrary among ties either way, just needs to be deterministic. Confirmed by running the
same seed+binary 5x before (non-deterministic result) and after (identical every time).
If you ever add another `HashSet`-backed tie-break in this file, sort first for the same
reason — a `HashSet<Pos>` used only for membership/union (not iterated for a "pick one"
decision) doesn't have this problem.

**Goal-aware solver tuning — eval-weight reweighting mostly failed; a dominance-threshold
fix worked for `Jelly`; a different-mechanism "setup" heuristic worked for
`Ingredients`**: seed-swept both variants' losses (n=150, post-determinism-fix) and found
essentially every `Jelly`/`Ingredients` loss is a near-miss (`Jelly`: 1-5 cells left, most
at exactly 1; `Ingredients`: usually 2/3 collected) rather than a blowout — exactly the
shape the todo backlog predicted would benefit from weighting the last few goal cells
higher.

First pass tried three *gentle* reweightings — scaling `JELLY_WEIGHT` up below a
low-remaining threshold, an "endgame mode" inside the last ~5 moves that zeroes
`score_gained`/`COMBO_BONUS` in favor of goal weights, and a "reward clearing anything in
the same column (at/above the stuck cell's row)" proxy for cycling fresh refills through
it. All landed flat-to-slightly-worse across n=150 sweeps.

The actual root cause, found by instrumenting real losing episodes rather than
theorizing further: **26 of 60 `Jelly` losses (n=150) had the bot pass up an
already-legal jelly-clearing move in favor of a bigger `COMBO_BONUS`/cascade elsewhere**
— the moves existed, they just consistently lost the comparison, because a *gentle*
multiplier (2x `JELLY_WEIGHT` = 600) still loses to `COMBO_BONUS` (1200). The fix isn't
"weight jelly higher," it's "make it dominate": below `solver::JELLY_ENDGAME_REMAINING`
(4) remaining cells, `solver::JELLY_ENDGAME_BONUS` (5000) per cell cleared outright beats
every other term in the eval. Measured on two disjoint seed ranges to rule out
overfitting: 90→97/150 (tuning range), 85→88/150 (held-out range) — both positive,
holdout gain smaller than tuning gain as expected, not a fluke. `Score`/`Ingredients`
win rates confirmed unchanged (this bonus is gated to `Variant::Jelly` only).

**`Ingredients` has no *reweighting* fix, but does have a *different-mechanism* fix that
works.** Re-tried the same low-remaining/dominance idea there and it didn't move the
needle (61-62/150 vs baseline 61) — reweighting genuinely can't help here: an ingredient
only advances via gravity carrying it down, so on a turn where nothing currently clears
cells *underneath* one, there is no candidate move to boost. "No legal move can advance
the goal this turn" is *actually true* for `Ingredients` in a way it usually isn't for
`Jelly` (jelly can be cleared by any match landing on that cell — a broader condition
than "clears space under a specific falling tile").

That asymmetry pointed at the real fix: instead of rewarding *this* move for progress, reward
it for *setting up* next move's progress. `solver::ingredient_setup_score` checks, on the
board after a candidate move, whether the cell directly below each still-uncollected
`Ingredient` is "primed" (already has a same-color neighbor — one swap away from a match)
— verified against `compact_and_refill`'s actual gravity first (clearing *below* an
ingredient pulls it down a row; clearing *above* does nothing to its position, so "below"
is the only cell that matters). Speculative going in (refill is random, priming doesn't
guarantee a match lands there), but measured as a genuine win: **168→187/450 across three
disjoint 150-seed ranges, every single range individually improved** (+4, +3, +12 — not
one lucky range propping up a flat-elsewhere average, which is what made this trustworthy
enough to ship despite the speculative premise).

**Shipped: a narrow, `Ingredients`-only 2-ply lookahead** (`solver::best_next_move_ingredient_gain`)
— the "most promising unexplored direction" flagged by a prior session's research handoff.
Not a general 2-ply (still correctly ruled out — see "Why no `beam_solver`" above): only
the top `INGREDIENT_LOOKAHEAD_TOPK` (12) first-ply candidates get a second-ply probe, and
that probe only considers second-ply moves whose column falls within 1 of a still-uncollected
`Ingredient`'s column — moves elsewhere on the board can't affect that ingredient's fall
path next turn, so they're excluded rather than scored. Unlike `ingredient_setup_score`
(which rewards "primed," a proxy), this rewards a *simulated, deterministic* next-move
`ingredients_collected` gain — genuinely 2 plies of real lookahead, just pruned to a small
candidate set on both plies instead of the full ~100×100. Measured on 4 disjoint 150-seed
ranges against the setup-heuristic-only baseline, every range positive: 65→68, 59→75,
63→64, 48→58 (net 235→265/600, +5pp). Per-move solver cost overhead: ~0.5ms average in a
native release build (150-episode batch: 0.445s→1.799s baseline vs. lookahead) — trivial
against the fixed tick interval a solver runs at, including on WASM/mobile, since this
isn't a per-frame cost.

A *board-gen* lever was also tried and rejected: spawning ingredients 2 rows lower
(`gen_range` in `gen_board`'s `Ingredients` arm, rows 2-3 instead of 0-1) to shorten the
required descent. Looked promising on one seed range (+12) but was *worse* on the other
two (-2, -4) — net barely positive across all 450 and, critically, inconsistent
range-to-range, unlike the setup-heuristic's fix which improved everywhere. Rejected as
noise, not signal; don't ship a change on the strength of one favorable range without
checking at least two more — `Ingredients`' baseline win rate alone swings ~7 points
range to range (34%-41% across three 150-seed samples with *no* code change), so a
single-range "+8pts" can easily be nothing.

**`Score`'s "chain-of-bonuses planning" gap (todo backlog bullet 3) has no real
headroom**: instrumented actual play and found combos fire in **7 of ~1200 moves across
60 episodes (0.6%)** — two specials ending up adjacent-and-swappable is rare on an 8x8
board, and on the rare occasion it happens the combo is already an enumerated legal move
whose `score_gained` plus `COMBO_BONUS` trivially wins the 1-ply comparison already; the
todo's "used Wrapped alone when an adjacent RowClear would combo for more" scenario only
bites when the two specials are *not* yet adjacent — a true "swap A first to set up A+B
next turn" 2-ply plan, which a 1-ply eval structurally cannot see regardless of how the
existing bonus weights are tuned. Separately, `Score` losses are broad scoring deficits
(mean 2552 vs target 3400; only 10/88 losses within 200 points of the target), not
near-misses a bonus-choice tweak touching <1% of moves could plausibly close. No fix
attempted — the diagnosis alone was conclusive enough not to need one.

**Follow-up, same broad-deficit diagnosis pushed one step further: `Score`'s weakness
was balance mis-calibration, not solver skill — fixed by lowering `SCORE_TARGET`.**
Traced real losing episodes (`--debug`) rather than reasoning from the eval alone: the
final-score distribution at move-limit-out is smooth and unimodal, centered *below*
3400 (median 3120 at the old target, n=150) — the signature of "target set too high for
the move budget," not "solver falls off a cliff on a subset of boards." Loser episodes
are cascade-starved by random-refill luck (losers average ~3.5 big/≥200pt cascades and
~7 bare-3s per episode vs. winners' ~6 big/~3 bare-3s) rather than by the solver passing
up a bigger available move — `score_gained` is what the eval already optimizes most
directly, so there was little skill headroom left to find. Since `score_resolution`
never reads `SCORE_TARGET` (only `update_phase`'s win check does), the chosen move
sequence — and therefore every episode's final score — is bit-identical regardless of
the target's value, so the win-rate-at-each-target-value comparison below is exact, not
resampled: **`SCORE_TARGET` 3400→3100** (`game.rs`), measured on the same three disjoint
150-seed ranges used throughout this section, every range improved: 62→77, 60→76,
66→79 — overall `Score` **188/450 (41.8%) → 232/450 (51.6%)**, landing in the repo's
stated 45-55% band for the first time. `Score` `LEVELS` entries are unaffected (each
level's own `LevelParams::score_target` is independent of the module constant, by
design).

A companion *skill* idea was tried and rejected: a sub-quantum "churn" tiebreak
(`fall_distance / 12, capped at 9` — provably incapable of overriding any real
score/spawn/combo difference, since `score_for` only ever changes `score_gained` in
multiples of 10) rewarding moves that displace more cells, on the theory that more
churn means more fresh random refill means more future cascade chances. Looked mildly
positive on the first three 150-seed ranges (+4, +2, +1) but **-15 on a fourth,
holdout range** — net *negative* across all 600 seeds once that range is included.
Same rejection shape as the `Ingredients` board-gen lever above: don't trust a small
positive signal on too few ranges. Rejected; not shipped.

## Level progression (`VariantMode::Levels`)

A fifth `V`-cycle entry alongside the four free-cycling variants above (`--variant
levels`) — steps through a hand-authored line, `game::LEVELS: &[LevelParams]`, instead
of one fixed `Variant`. Each `LevelParams` reuses the same four `Variant` win-conditions
but supplies its own `score_target`/`move_limit`/`jelly_cell_count`/
`ingredients_target`/`time_limit` instead of the module's fixed per-variant constants —
`Game::new_level` is the level-aware sibling of `Game::new`, and `gen_board` takes
`jelly_cell_count`/`ingredients_target` as parameters rather than reading the consts
directly, for exactly this reason.

Deliberately **not** one of `Auto`'s `generation % 4` rotation targets — per root
CLAUDE.md's "In-game controls" note, `Auto` should never land on a mode only reachable
by explicit select. Only reachable via the `V` cycle or `--variant levels`.

**Session-level state, not derivable from `generation` alone**: `main.rs`'s `Session`
carries `level_index`/`level_attempts` fields alongside `mode`/`game`, since "which
level" and "how many times has the bot failed it" depend on win/loss history, not just
an episode counter. `Session::next_generation` is where this is judged: a win or
`game::LEVEL_STUCK_LIMIT` (5) consecutive losses advances to the next level (wrapping
past the end of the line back to the start); otherwise the same level replays with a
fresh board. There's no player to eventually git gud against a fixed-heuristic bot, so
a level that can't be won at all would otherwise stall the line forever without the
stuck-limit escape hatch.

**`Timed` needed a real fix, not just new numbers**: the free-cycling `Timed` variant
has no win condition at all (`update_phase`'s `Timed` arm was a hardcoded `false` —
only `TimeUp` ever ends it), so a naive "Timed level" would've been structurally
unwinnable. `update_phase` now checks `self.score_target > 0 && self.score >=
self.score_target` for `Timed`; `Game::new`'s free-cycling `Timed` games set
`score_target: 0` (never satisfied, so behavior is unchanged from before), while
`Timed` levels give it an actual positive target to race the clock against.

**Balance discipline**: `game::tests::level_line_win_rates_stay_above_floor` (`#[ignore]`
— slow) seed-sweeps every level via the real solver and asserts each stays above a 25%
floor, per this file's and the root CLAUDE.md's "every level needs a seed-sweep check
before it ships" rule — a level under that floor is a balance bug, not "hard mode."
Run it after touching `LEVELS` or the solver's weights:
```
cargo test --release -p match-3 level_line_win_rates -- --ignored --nocapture
```

## Bonus tiles and combos

Match-4 spawns a line-clear tile **aligned** with the match's own direction — a
horizontal match spawns `Special::RowClear` (clears its row), a vertical match spawns
`ColClear`. This is intentionally *not* the real Candy Crush convention (which is
perpendicular) — a deliberate user choice made when reviewing this game; don't "fix" it
back to perpendicular without asking. Match-5-straight spawns a colorless
`Tile::ColorBomb`. An L/T-shaped 5-cell match (both a horizontal and vertical run
sharing a cell) spawns `Special::Wrapped` (clears a 3x3 block).

**Combos**: swapping two bonus tiles together (not just incidentally matching them)
triggers an amplified effect via `combo_cells` — thick 3-row+3-col cross for
line+line/line+wrapped, 5x5 block for wrapped+wrapped, full-board wipe for bomb+bomb,
"every tile of a color, each also getting the paired effect" for bomb+line/bomb+wrapped.
Ordinary chain-triggering (a match sweeps through a *pre-existing* bonus tile) reuses
the same per-kind `effect_cells` without the combo amplification — a BFS fixed-point in
`chain_specials`.

**Gotchas if touching `resolve()`**:
- Bonus effect areas are computed geometrically/by-color and don't know an
  `Tile::Ingredient` sitting in their area is supposed to be immune —
  `cleared.retain(|pos| !is_ingredient(pos))` must run right after `chain_specials`, or
  a bonus effect silently destroys collection-goal tiles instead of leaving them for
  gravity. Any future "immune tile" type needs the same guard.
- `ColorBomb`'s `effect_cells` is color-filtered and `ColorBomb` itself is colorless, so
  it never clears (consumes) *itself* by that path alone — both `SwapKind::SoloTrigger`
  and `combo_cells`' bomb branches explicitly insert the bomb's own post-swap cell.
- The swap happens (`board.tiles[a]=tile_b; board.tiles[b]=tile_a`) *before*
  `classify_swap`/the resolution match — `SwapKind::SoloTrigger`'s effect must anchor on
  where the special tile *lands* (the other position from where it started), not where
  it began. Easy to get backwards; `combo_cells` already takes post-swap `at`/`bt` as
  parameters for exactly this reason.

## Resolution model

`Game::apply`/`Game::simulate` (the latter pure, used by both real play and the
solver's candidate scoring) resolve a swap into `Resolution { waves: Vec<Wave>, .. }` —
each `Wave` is one clear+gravity+refill round, cascades are just more waves. Gravity
(`compact_and_refill`) records per-tile `FallEntry { from_row, to_row }` so the renderer
can tween exact fall distances, including tiles queued above the board (`from_row`
negative) for freshly-spawned refills. `main.rs`'s `View`/`StepPhase` plays this back as
Swap → (Flash → Fall) per wave → Idle, matching the already-decided outcome cosmetically
— same "compute first, animate after" pattern as Tetris's `FallAnim`.

## Rendering

`GemShape::for_color`: Red=circle, Orange=triangle, Yellow=star (two overlapping
triangles — macroquad has no star primitive), Green=pentagon, Blue=hexagon,
Purple=diamond. `draw_gem` layers: darker backing shape, base-color shape inset, then
(if `highlight`) a soft diagonal gloss streak (3 small circles of decreasing size/alpha
toward the upper-left) — kept conservatively inside even a triangle/star's tight
inradius (~0.3 × size) so it can't spill onto the dark cell background outside the
shape. The `highlight: bool` param (threaded through `draw_tile`→`draw_gem`) hides the
streak outright during any blink/fade/crossfade rather than fading it — see git history
of `draw_gem` for why (a translucent light layer fading over a dark background reads as
gray, not a dimming highlight).

**The streak's circles are opaque, colored by `lighten(base, al)` computed in Rust, not
translucent `Color::new(1,1,1,al)`** — this region is cached by `RenderCache`
(`board_cache` in `amain`, see next section), and translucent content rendered inside a
`RenderCache`'d target composites visibly grayer than identical draw calls made live.
match-3 is the first cached region in this workspace with genuinely translucent content
inside it; see the `lib/render_cache` project memory for the general version of this if
extending this pattern elsewhere.

Bonus-tile overlays (row/col-clear stripes, the wrapped ring) are shape-agnostic —
stripes read as a band across any silhouette; the wrapped ring is a circular halo, not
a square outline.

**RenderCache usage differs from every other game here**: match-3's board animates
almost continuously (swap/flash/fall chase each other with no real idle gap *during* a
move), so caching only pays off in the `Idle` beat between moves and the `GameOver`
overlay — `amain` mirrors `game2048`'s exact split (`board_cache.mark_dirty()` every
frame while animating, `board_cache.draw()` only when settled) rather than Tetris's
"cache the locked board, draw the live piece on top" split.

## Testing

`game.rs`'s `#[cfg(test)]` module includes `full_playthrough_terminates_for_every_variant`
— a soak test (10 episodes/variant via the real `crate::solver::choose_move`) asserting
termination and that every applied move came from `legal_moves()`. Worth this shape of
test for any new self-playing game, not just this one.

## Running

```bash
mise run run match-3                                   # native
mise run build-wasm match-3                             # WASM → dist/match-3/
HCG_SEED=1 target/release/match-3 --no-ui --once --variant score --debug
```
