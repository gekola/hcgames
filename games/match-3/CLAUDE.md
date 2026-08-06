# Match-3

Self-playing match-3 (Candy Crush-style swap-and-cascade), beam-search AI (depth 2). Board 8x8,
6 colors, each color with its own silhouette (not just a color-coded fill). Directory/
package name has the dash; `xtask::title` renders it "Match 3".

## Source layout

| File | Contents |
|------|----------|
| `src/game.rs` | `Board`/`Tile`/`Special`/`Color`, `Game`, `resolve()` (swap → cascade), board gen |
| `src/solver.rs` | `choose_move` — `beam_solver`-backed depth-2 search scored by `score_resolution`'s tuned eval over `Game::simulate` |
| `src/main.rs` | `GemShape`/`draw_gem`/`draw_tile`, `View`/`StepPhase` animation state machine, CLI |

## Solver: beam search, not greedy — promoted from an opt-in experiment

Unlike Klondike/Spider/Tetris, there's no "known next piece" to search a second ply into
— what a swap reveals depends on where gravity happens to refill from, and an 8x8 board
already offers ~100 legal swaps per move, so on paper a second ply multiplies branching
factor rather than sharpening a small candidate set (root CLAUDE.md's "Self-playing
solver games" section names this as a 3rd solver shape that *defaults* to a cheap 1-ply
eval). match-3 was originally built exactly that way: a plain greedy `choose_move`
enumerating `Game::legal_moves()`, resolving each via `Game::simulate`, and picking the
highest-scoring `Resolution` by `solver::score_resolution`.

**A direct measurement disproved the theoretical objection.** `lib/beam_solver` (width 6,
depth 2, ~800-node budget) was wired in as an opt-in `--solver beam|hybrid` experiment and
A/B'd against greedy at n=500 (`HCG_SEED=1..500`, `--no-ui --once`, release), RNG-confound
fixed (see the preview-oracle note below). Beam won across every variant: Score
56.9%→66.4%, Jelly 59.1%→73.1%, Mystery 49.9%→62.7%, Ingredients 44.9%→51.3%, Timed mean
score 10061→10886 — +6-14pp, at ~1.5ms/move (nowhere near a per-tick budget problem, even
on mobile). See `.notes/match3_solver_beam_todo.md` for the full table. As of this change,
beam is the *default and only* solver: both greedy and the throttled `hybrid` were deleted
(hybrid was worst-of-both — its every-3rd-move throttle cost more than the already-cheap
full beam saved). `solver::choose_move` is now the sole entry point, a `beam_solver` search
scored by the same tuned `score_resolution`.

**Building the beam solver surfaced a real, subtle bug worth knowing about before touching
`Game::simulate`/`resolve`/anything RNG-related in this file: a hypothetical-scoring
clone must never let its RNG carry forward the real board's actual future stream.**
`Board` carries its own `Rng` (SplitMix64) rather than drawing from the ambient global
`macroquad::rand` — necessary because two solvers that call `simulate` a different
number of times per real move (beam vs. greedy) would otherwise perturb the *real*
game's future refills by different amounts at the same `HCG_SEED`, breaking any A/B
between them. The naive version of this fix — reseed a board's own `Rng` once per board
from the global RNG, then let clones simply *carry forward* whatever state the source
board's `rng` was already in — is not just insufficiently isolated, it's actively worse:
since nothing else touches the real board between a `Game::simulate` scoring call and a
same-move real `Game::apply`, carrying the real state forward makes the "preview" a
**perfect oracle** for whatever move actually gets applied next, not an unbiased
estimate of a hypothetical. Measured directly: every variant's win rate jumped to
85-100% (from a healthy 45-65% band) the first time this was tried, which is what caught
it — re-run the seed sweep after any change here and treat near-100% as a bug signal,
not a win. The actual fix, `game::preview_seed(board, mv)`: reseed every
hypothetical-resolve clone's `rng` from a **deterministic hash of the pre-move board
content + the move** (a hand-rolled `DeterministicHasher`, not
`std::collections::hash_map::DefaultHasher` — see "`HCG_SEED` reproducibility gotcha"
below for why `std`'s default hasher specifically can't be used for anything that
affects which move gets chosen) instead of carrying the real stream forward. This keeps
previews fully decorrelated from the real future (no oracle) while staying deterministic
given `(board, move)` and independent of caller/call-count (the original fairness
requirement). Two call sites need this, not one: `Game::simulate` itself, and
`beam_solver::SearchState::apply`'s impl for `Game` in `solver.rs` — the beam engine's
own line-advancing `apply` (used to generate later-ply candidates, not just to score a
move) is a second, separate leak of the exact same bug, reached via a different call
path. Only real gameplay (`Game::apply` called on `self.board` directly, never a clone)
still continues a board's actual running stream, uninterrupted by however much scoring
happens around it.

## Goal variants

`V`-cycles like Tetris's piece-gen modes (`VariantMode::Auto` rotates by
`generation % 6`, see `main.rs`):

| Variant | Goal | Paced by |
|---|---|---|
| `Score` | reach `SCORE_TARGET` | `SCORE_MOVE_LIMIT` moves |
| `Jelly` | clear all `Board::jelly` layers | `JELLY_MOVE_LIMIT` moves |
| `Ingredients` | get `INGREDIENTS_TARGET` `Tile::Ingredient` tiles to row `H-1` via gravity only | `INGREDIENTS_MOVE_LIMIT` moves |
| `Mystery` | clear every goal in `Game::mystery_goals` (1-3 colors, each own target — "hit all to win"; HUD names it "color hunt") | `MYSTERY_MOVE_LIMIT` moves |
| `Licorice` | clear every `Tile::Licorice` cell on the board (HUD names it "blocker clear") | `LICORICE_MOVE_LIMIT` moves |
| `Timed` | no move limit | real countdown, `Game::tick_time(dt)` called from `amain` directly (time-paced, not move-paced — the only variant not driven through `Game::apply`) |

**`Mystery`** (todo backlog item #4's "candy-order" goal — collect N of a specific
color, generalized to a small *combination* of simultaneous color targets) needed no new
`Tile`/board-gen: `Resolution::color_cleared: [u32; 6]` is computed unconditionally in
`resolve()` alongside `jelly_cleared`/`ingredients_collected` (indexed via `Color::index`),
so `Game::apply` just loops `self.mystery_goals` adding `res.color_cleared[goal.color
.index()]` to each — same mechanism as `Ingredients` reading `ingredients_collected`, just
per-goal instead of singular. `gen_mystery_goals` (called from `Game::new`) rolls 1-3
distinct colors (`MYSTERY_MAX_COLORS`, clamped to however many colors are actually
available — see below) each with its *own* randomly-picked target from
`MYSTERY_TARGET_RANGES[count - 1]` — "various targets" per design, not one shared number.
Win condition (`update_phase`) is a plain `.all(|g| g.collected >= g.target)`. Wired into
`LEVELS` too (`Color Search`/`Rainbow Hunt`/`Color Storm`, one per color-count tier) —
`Game::new_level` rolls `mystery_goals` the same way `Game::new` does (a `let
mystery_goals = gen_mystery_goals(&board.active_colors)` before the `Self { .. }` literal,
since goals are rolled fresh per episode rather than stored on `LevelParams` — a `Mystery`
level only needed the `color_count`/`move_limit` fields every other variant already has,
no dedicated goal-list field). No `Variant`-to-struct architecture change was needed for this (the todo doc's harder
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

**Re-tried under the beam solver + multi-goal `Mystery` — still doesn't help, now with a
measurable *regression*.** The old ablation above was under the greedy solver and a single
fixed goal, so beam's different eval landscape + the multi-goal shape counted as genuinely
new conditions worth a re-test (per this file's "don't re-add without new evidence" rule
cutting both ways). Implemented as a `MYSTERY_ENDGAME_*` bonus aimed at the single
*bottleneck* goal (least-complete still-open goal, `min` remaining need) rather than
blanket across every open goal, same `remaining≤4`/`bonus 5000` shape as `JELLY_ENDGAME_*`.
Ablation on a fresh disjoint range (`HCG_SEED=301..600`, n=300, `--variant mystery`,
bonus-zeroed baseline vs. active), bucketed by goal count: overall 61.3%→**59.0%**, and
every bucket regressed (g1 65.0%→62.5%, g2 61.7%→59.1%, g3 58.1%→56.2%). Reverted (consts
+ logic deleted). Same root cause as the greedy finding still holds under beam: a 3-match
of the target color already scores ~900, close enough to `COMBO_BONUS` (1200) that there's
no near-miss-loses-to-combo pattern to fix; forcing dominance just perturbs move choice
slightly the wrong way. A second, independent "didn't help" data point — don't re-attempt
without new evidence beyond "beam is new."

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

**`gen_mystery_goals` draws from `board.active_colors`, not always `Color::ALL`** — added
once `LevelParams::color_count` (see "Board color count scales with level" below) made it
possible for a board to not generate every color at all; picking a goal color the board
never spawns would be a silently-unwinnable episode, not just a hard one. `count` is also
clamped to `active_colors.len()` so a hypothetical very-narrow palette can't roll more
distinct goal colors than actually exist. This was landed *before* `Mystery` was wired
into `LEVELS`, specifically so that wiring couldn't introduce an unwinnable level by
construction — confirmed bit-identical to the pre-fix free-cycling win rates at the time
(color_count 6 there == `Color::ALL`, no behavior change), and validated for real once
`Mystery` levels existed via the usual floor-check soak test. Computed via a `let
mystery_goals = ...` *before* the `Self { .. }` literal in both `Game::new` and
`Game::new_level` (needs `&board.active_colors`, and `board` itself moves into that
literal) — don't move either call inline into its struct literal without re-threading
the borrow.

**`Mystery` levels needed no new `LevelParams` field** — `Color Search`
(color_count 4, move_limit 20), `Rainbow Hunt` (5, 22), and `Color Storm` (6, 24) reuse
`variant`/`move_limit`/`color_count` exactly like any other level; `score_target`/
`jelly_cell_count`/`ingredients_target`/`time_limit` are simply dead for them, same
"only meaningful for the corresponding variant" pattern the doc comment on `LevelParams`
already established. Floor-checked in the same soak test as every other level: 100%,
97%, 73% respectively (n=60 each) — consistent with their tier siblings (Warm-Up/Sticky
Start also 100% at color_count 4; Deep Jelly/Full Batch 63%/53% at color_count 6), so no
separate tuning pass was needed beyond confirming the floor holds.

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

**`JELLY_ENDGAME_BONUS` alone still leaks jelly-endgame near-misses under the depth-2
beam — the *path-additive* bonus lets the beam defer the winning clear onto a phantom
refill. Fixed with a root-ply-only companion (`solver::JELLY_ENDGAME_ROOT_BONUS`).** L11
"Deep Jelly" (`LEVELS[10]`, 22 jelly / 24 moves / 6 colors) was losing ~28% (n=150),
100% near-miss, almost all ending `jelly_remaining=1`. Instrumented every losing episode's
endgame turns (`jr≤4`, disjoint seeds 151-300): **33 of 38 losses had ≥1 turn where a
legal move would clear jelly immediately but the beam picked one that didn't — and the
passed-up alternative frequently cleared *more* jelly AND scored *more* raw points**,
which a 1-ply eval structurally cannot do, so the culprit had to be the depth-2 search.
Root cause: `beam_solver` scores whole lines by *cumulative sum* and `JELLY_ENDGAME_BONUS`
is path-additive, so a plan that clears the last jelly at ply 2 earns the same 5000/cell
at ply 2 *plus* banks ply-0's incidental `score_gained` on top — out-scoring the "win now"
line by that incidental margin. But ply 2 rides a `preview_seed`-decorrelated phantom
refill (see the RNG note above) that won't materialize, so the deferred win evaporates.
Fix: an extra `JELLY_ENDGAME_ROOT_BONUS` (4000/cell, `jr≤JELLY_ENDGAME_REMAINING`,
`Variant::Jelly` only) applied in `score_root` but **not** `score_step` — only the root
move is ever actually applied, so crediting real immediate jelly progress over a
hypothetical later clear is exactly correct. `choose_move` now passes a distinct
`beam_score_root` (= `beam_score` + this bonus) for ply 0; `beam_score` still scores every
later ply. Measured across five disjoint 150-seed ranges (151-900): 533→616/750 overall
(71.1%→82.1%, +11pp), four ranges +19/+21/+19/+25, one flat (-1) — robustly past the
"~5pp is noise" bar, not one lucky range. Floor test unregressed (Jelly levels 2/6 stay
100%/97%; every level ≥ floor; Score/Timed/Ingredients/Mystery untouched, gated).

**L14 "Against the Clock" (`LEVELS[13]`, `Timed`, target 8200 / 55s = 55 moves) near-miss
losses are board-luck/pacing, *not* an eval miss — no fix, mirrors the `Score`-variant
"cascade-starved variance" finding.** Diagnosis (seeds 151-300): replayed every episode a
second time with a *pure highest-`score_gained` 1-ply* policy and compared final scores.
Beam wins 123/150 (82%); pure-score greedy wins only 94/150 — **beam already beats raw
score-chasing in aggregate**, so it isn't leaving points on the table. Greedy scores
higher *only* on the subset of beam's own 27 losses (would-win 19 of them, +560 median
score), but that policy loses far more elsewhere — you can't capture the subset benefit
without the aggregate cost unless the eval becomes score-gap/time-aware (a much bigger
change, not a weight tweak). Losers use all 55 moves (winners finish in ~44), median
shortfall only 510 pts (~94% of target) — a smooth near-miss distribution, the signature
of variance, not a solver blind spot. L14 also isn't below floor (82% on 151-300, 63% on
1-60), so no balance lever needed either. Left as-is; don't reweight `Timed` blind — moving
toward greedy would regress the aggregate.

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

**Superseded and removed once beam search shipped as the default** (see "Solver: beam
search, not greedy" above). This whole `best_next_move_ingredient_gain`/
`INGREDIENT_LOOKAHEAD_*` hand-rolled top-12-candidates lookahead is gone — beam's genuine
depth-2 search across the *full* candidate set outperforms it *without* the hack being
ported into beam's eval at all: beam still beat greedy on `Ingredients` (44.9%→51.3% at
n=500), the one variant this lookahead was built for, which is the evidence it wasn't
needed. Kept the writeup above because the tuning story (setup-heuristic vs. reweighting
vs. narrow lookahead, and the board-gen lever that was rejected as noise) is still a real
lesson worth not re-deriving.

A *board-gen* lever was also tried and rejected: spawning ingredients 2 rows lower
(`gen_range` in `gen_board`'s `Ingredients` arm, rows 2-3 instead of 0-1) to shorten the
required descent. Looked promising on one seed range (+12) but was *worse* on the other
two (-2, -4) — net barely positive across all 450 and, critically, inconsistent
range-to-range, unlike the setup-heuristic's fix which improved everywhere. Rejected as
noise, not signal; don't ship a change on the strength of one favorable range without
checking at least two more — `Ingredients`' baseline win rate alone swings ~7 points
range to range (34%-41% across three 150-seed samples with *no* code change), so a
single-range "+8pts" can easily be nothing.

**A *deeper beam* (depth 3) for `Ingredients` only was tried under the beam solver and
rejected — the gravity-gated stall is structural, not search-depth-reachable.** Rationale
going in: unlike a *weight* change (which can't touch a turn where no legal move advances
the goal), a deeper search *could* score a 3-move column-clearing plan above a shallower
alternative even when step 1 does nothing for the ingredient this turn; `BEAM_NODE_BUDGET`
bounds cost so depth 3 stays cheap (measured 4.1ms/move, trivial vs. tick). Measured on
disjoint ranges vs. same-range depth-2 baseline, every measurement flat-to-worse:
free-cycle `Ingredients` `HCG_SEED=301..600` 48.3%→51.0% (+2.7pp, *under* the ~5pp noise
floor — and 140 of 300 episodes flipped outcome for a net +8, the churn signature of
noise, not a systematic gain: deeper search just picks different moves → different
`preview_seed`-reseeded refill futures → near-coin-flip different outcomes); L7 Two-by-Two
`151..=300` 73.3%→**57.3%** (-16pp); L12 Full Batch 57.3%→53.3% (-4pp). Reverted to a
single global `BEAM_DEPTH` (2). Confirms this section's standing `Ingredients` diagnosis:
"no legal move can advance the goal this turn" is *actually true* on the stalled turns, so
more plies have nothing better to find — the loss is gravity/board-gen-gated, not skill.
Don't re-attempt a depth bump here without evidence that changes.

**Two remaining `.notes/match3_solver_beam_todo.md` beam levers (#3 step-scoring, #5
`is_pointless`) investigated and both ruled out — nothing shipped.** Fresh disjoint range
`HCG_SEED=1000..1200` (n=200 free-cycle each + L7/L11/L12/L14/L15), baseline confirmed
healthy first (Jelly 83.5%, L11 79.5%, Score 58.5%, Mystery 64.0%, Ingredients 48.5%).
- **`is_pointless` (#5): no meaningful category exists here.** Every legal swap clears
  cells — `is_legal_swap` only admits a `Normal` two-plain swap when it forms a match, and
  `Combo`/`SoloTrigger` swaps always fire a bonus effect — so there's no legal-but-useless
  move to filter (unlike Klondike/Spider's empty-effect stock draws). A "prune the
  worst-scoring fraction" filter is also provably output-neutral: `beam_solver` already
  sorts root candidates by `score_root` and truncates to `width`, so a score-floor filter
  can't change which `width` survive; and match-3's ~100-move branching × width 6 × depth
  2 stays under `BEAM_NODE_BUDGET` (800), so it never exhausts budget where a step-ply
  filter could matter. Left `|_, _| false`.
- **General step-ply goal-progress discount (#3b): flat-to-negative.** Multiplied only the
  goal terms (jelly/ingredient/mystery clears + progress + setup, *not* raw
  `score_gained`/combo/bonus-spawn) by a factor <1 in `score_step` while leaving
  `score_root` full — the general version of the `JELLY_ENDGAME_ROOT_BONUS` lesson (later
  plies ride a `preview_seed` phantom refill). d=0.75: Jelly 83.5→80.0, Mystery 64.0→64.5,
  Ingredients 48.5→45.5, L11 79.5→80.5, L12 48.5→45.5. d=0.5: Jelly 82.5, Mystery 61.0,
  Ingredients 44.5, L11 78.0, L12 44.5. Score/L14/L15 identical by construction (no goal
  terms). No metric beat baseline past noise; Ingredients/L12 regressed. The L11 jelly
  case the root bonus already handles didn't want generalizing — reverted.
- **Redundant `resolve()` per node (#3a): real but not worth fixing.** `SearchState::apply`
  (solver.rs) resolves + discards the `Resolution`, then `beam_score`/`beam_score_root`
  re-`simulate` the same `(board, move)` (identical `preview_seed`) to score it — every
  node resolves twice. But the closures only get `&Game`, so removing the second resolve
  needs a `Resolution` stored on `Game`, which every beam clone would then deep-clone
  (waves `Vec`s) — likely adding clone cost, not net-saving. Perf is already trivial
  (2.65ms/move release across the 9-config sweep, vs. tens-of-ms tick), so not worth the
  complexity. Left as-is.

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
`ingredients_target`/`time_limit`/`color_count` instead of the module's fixed per-variant
constants — `Game::new_level` is the level-aware sibling of `Game::new`, and `gen_board`
takes `jelly_cell_count`/`ingredients_target`/`active_colors` as parameters rather than
reading the consts directly, for exactly this reason.

Deliberately **not** one of `Auto`'s `generation % 6` rotation targets — per root
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

**L12 "Full Batch" and L15 "Grand Finale" were both above-floor but outlier-low next to
their color_count=6 siblings (50%/35% vs. Deep Jelly 85%/Color Storm 85%/Against the
Clock 63%) — re-swept post beam-promotion and fixed via level params, not the solver.**
A dedicated instrumented sweep (n≈450 each, disjoint from the floor test's own range)
found the two losses are different shapes, matching this section's existing
per-variant diagnoses exactly:
- **L12 (`Ingredients`, target 3/move_limit 26): 152/222 losses (68%) were short by
  exactly 1 ingredient** — the same "gravity-gated stall" this file already documents
  as *not* fixable by reweighting or deeper search (see "`Ingredients` has no
  *reweighting* fix" above). Since the block is structural per-turn, not a solver
  blind spot, the fix is a level-param one: **`move_limit` 26→28**, giving a stalled
  board more chances for gravity to eventually deliver the last ingredient. Not a
  solver change.
- **L15 (`Score`, target 3400/move_limit 18): only 20/279 losses (7%) were near-miss**
  (median shortfall 790/3400, ~23% short) — a broad scoring deficit, the same
  "target set too high for the move budget" shape that already justified lowering the
  free-cycling module constant (`SCORE_TARGET` 3400→3100, see "Follow-up, same
  broad-deficit diagnosis" above). That fix didn't touch `LEVELS` (each level's own
  `score_target` is independent by design), so L15 was left sitting at the same 3400
  value the free-cycling variant was already measured too high at, on a *tighter*
  budget (18 moves vs. the free variant's 20). Fix: **`score_target` 3400→3000**.

Both candidates were swept over two disjoint 150-seed ranges before shipping (this
file's usual bar): L12 move_limit=28 → 54.7%/58.7%; L15 target=3000 → 53.3%/48.7% —
both landed in the healthy ~45-60% band on both ranges, not one lucky range. Re-running
the real floor test confirmed it end to end: L12 50%→57%, L15 35%→53% (n=60), every
other level unchanged, still all ≥ floor.

**Board color count scales with level** (todo backlog item #2): `LevelParams::color_count`
(4-6, only meaningful for `Levels` — every free-cycling variant always uses the full
`Color::ALL`) slices `&Color::ALL[..color_count]` and carries it as `Board::active_colors`,
which every color-rolling call site reads instead of the old parameterless
`Color::random()` (now deleted): `gen_plain_tiles` (initial fill), `compact_and_refill`
(gravity's fresh-tile spawns), and `reshuffle` (both the initial "guarantee a legal move"
pass and the deadlock safety net). Carrying the palette *on the board* rather than
threading it through every one of those calls separately means it survives every clone
this game already makes for free — `simulate`'s scratch boards, each wave's
`board_before`/`board_after` — without a fourth parameter creeping into functions that
don't otherwise need to know about levels at all. `MIN_LEVEL_COLORS` (3) is a hard floor,
not a difficulty knob: `gen_plain_tiles`'s per-cell retry loop excludes at most 2 distinct
colors (one to avoid extending a horizontal run, one for vertical), so 2 active colors
could exclude both and spin forever — `Game::new_level` `debug_assert!`s every level's
`color_count` stays above it. `LEVELS` currently ramps 4 (levels 1-4) → 5 (5-8) → 6 (9-12).

Re-running the floor-check soak test after adding `color_count` surfaced a real, sizeable
effect worth knowing about before "fixing" it as a regression later: **levels 1-8 (4-5
colors) jumped to near-100% win rate**, while **levels 9-12 (still the unchanged, always-6
colors) stayed exactly where they were before this feature** (63%/53%/62%/30% — identical
seed-for-seed, since a `color_count: 6` level's `&Color::ALL[..6]` is the same full slice
`gen_board` always used). Fewer colors means far more incidental matches at the *same*
`score_target`/`move_limit`/etc. those early levels were originally tuned against (tuned
back when every level used 6 colors) — this reads as correct, not broken: the LEVELS
floor rule (see above) only asserts a *minimum* of 25%, deliberately with no matching
upper bound, because unlike a standalone free-cycling variant (which should always feel
like a real toss-up, hence *that* convention's 45-55% band) a hand-authored level *line*
is supposed to ramp from trivial to hard — an early "Warm-Up" the bot always wins is the
intended shape, not a bug. Left as-is; re-tuning every level's *other* numbers (score
targets, move limits, etc.) downward to compensate and manufacture a tighter early-level
band was considered and deliberately not done — no floor violation to fix, and doing so
would be re-tuning 8 levels' worth of already-shipped, already-floor-checked numbers on
taste alone rather than in response to a measured problem.

## Blocker tiles (`Tile::Licorice`)

First of `.notes/match3_todo.md` item #3's blocker candidates to ship — flagged there as
the simplest (closest to the already-solved `Tile::Ingredient` immunity pattern) and
picked as the starting point the rest of that list (Frozen, Deep Freeze, Locked/Caged,
Spreading Jelly) can build on. Colorless, like `ColorBomb`/`Ingredient` (`Tile::color()`
returns `None`), which is what already keeps it out of ordinary color runs — no separate
guard needed, same mechanism `Ingredient` already relies on. `classify_swap` refuses any
swap touching it (`SwapKind::Illegal`, same check as `Ingredient`) — it's a wall, not a
piece that can be picked up.

**The first version shipped had the clearing rule backwards — checked against real
match-3 games (Candy Crush's "Licorice Swirl" family) rather than assumed, and fixed.**
The todo doc's original guess ("only removed by a bonus effect... never an ordinary
match") turned out to be inverted: real Licorice Swirl's primary, iconic mechanic is
being cleared by an *adjacent ordinary match* — "each match weakens it" — with bonus
effects as a secondary path. The original implementation only had the secondary path,
which made it near-permanent in practice (bonus tiles are relatively rare), starving
`solver::LICORICE_WEIGHT` of almost anything to actually reward. Fixed in
`find_matches_with_spawns`: after computing the final `matched` set for *any* match
(the initial swap-driven one and every cascade continuation alike — real Licorice
weakens on cascades too, not just the triggering match), every orthogonal neighbor of a
matched cell that holds `Tile::Licorice` is folded into the same set, so it clears
alongside the match that triggered it. The bonus-effect path is unchanged and still
correct on its own terms: `RowClear`/`ColClear`/`Wrapped`'s `effect_cells` don't filter
by tile identity, so a Licorice cell caught in one of those areas clears same as any
other cell; `ColorBomb` still doesn't reach it (color-filtered, and Licorice has none) —
falls out of the existing filter, no bespoke exception needed. (Real Licorice Swirl also
"resists" a striped/line effect specifically, stopping its propagation past — not
replicated here as unneeded complexity for a first pass.) `Resolution::licorice_cleared`
(computed unconditionally in `resolve()`, same pattern as `jelly_cleared`/`color_cleared`)
is what the solver reads.

**Gravity-blocking ("shelf") behavior was kept despite not being directly attested for
the real Swirl object specifically** — search results distinguish it from a separate
"Licorice Fence" object, documented as the one that explicitly blocks falling. But any
obstacle that fully occupies a cell and never moves has to act as a local shelf for
whatever's above it in the same column as a matter of physical consequence, regardless of
which of King's two licorice-family props that behavior is nominally attached to, so the
mechanism below was kept as originally built. `compact_and_refill` doesn't compact a
column top-to-bottom in one pass — it walks each column looking for runs of rows *not*
containing a surviving (uncleared) Licorice cell, and calls the pre-existing
single-column logic (factored out as `compact_and_refill_segment`) once per run, scoped
to `start..end` instead of always `0..H`. A Licorice cell that itself got cleared this
wave isn't a separator — the segments on either side simply merge for that pass, same as
any other obstacle's removal would unify a column. `Tile::Ingredient`'s bottom-row
collection check is guarded on `end == H` for the same reason: a segment sitting above a
surviving shelf structurally can't reach the board's true bottom row while the blocker
stands.

**A segment below a shelf can't spawn its fresh tiles off-board the way the top segment
does — that would visibly clip through the shelf, since `FallEntry::from_row` is an
absolute board row the renderer lerps from, not a segment-relative one.** The original
single-segment formula (`from_row = start + i - deficit - 1`) goes negative or otherwise
above `start` for every fresh tile by construction; harmless when `start == 0` (spawns
off the true top of the board, unchanged behavior for every level with no Licorice in a
column), but for `start > 0` that range always lands at or above `start - 1` — i.e.
*inside* the segment above the shelf, or off-board past it. A first pass tried this naively
and it animated as tiles falling from off the top of the whole board, straight through
the shelf and the segment above it. Fixed by special-casing `start > 0`: every fresh tile
in a below-shelf segment falls in from exactly `start - 1` (the shelf's own row) instead —
no stagger between simultaneous spawns in that segment, but a fall that reads as
"emerging from beneath the shelf," not "through" it. Zero behavior change for `start == 0`
(every column in every pre-Licorice level), confirmed by the full test suite and the floor
test's unchanged win rates for every level not featuring Licorice.

**`Variant::Licorice` — clear every `Tile::Licorice` cell — added after the first ship,
replacing the original `Score`-dressed "Licorice Lane."** An obstacle-clearing level
should make clearing the obstacle the actual point, not an incidental drag on an
unrelated score target; a real `Variant` (mirroring `Jelly`'s "clear all X" shape) also
makes it a proper free-cycling `V`-cycle entry like every other goal, not a `LEVELS`-only
special case. `Game::licorice_remaining` (computed from `board.tiles` at generation time,
decremented by `Resolution::licorice_cleared` in `apply`) is the win-condition counter,
same relationship `jelly_remaining` has to `Variant::Jelly`. `solver::LICORICE_GOAL_WEIGHT`
(350, on top of the always-on `LICORICE_WEIGHT` of 200 — so ~550/cell total when Licorice
*is* the goal) is the `INGREDIENT_WEIGHT`-equivalent term that makes the eval actually
chase it rather than merely tolerate it.

`LevelParams::licorice_cell_count` itself stays independent of `variant` (threaded through
`gen_board` alongside `jelly_cell_count`/`ingredients_target`, placed after any
variant-specific placement so it only ever overwrites a plain tile) — a level *could* in
principle decorate a `Score`/`Jelly`/etc. board with some Licorice without making clearing
it the win condition, same architecture as before, just no longer how "Licorice Lane"
itself is built. `MAX_LEVEL_LICORICE` (soft cap on `licorice_cell_count`, not a rigorously
derived floor like `MIN_LEVEL_COLORS`) started at `W*H/4` (16) as an untested guess and was
raised to `W*H*3/8` (24) once real tuning data showed that density plays fine — see below,
the adjacent-match clearing rule needed *more* cells than guessed to feel like a real
obstacle, not fewer.

**Tuning: the adjacent-match clearing rule is dramatically more generous than the
bonus-effect-only version, and needed correspondingly denser boards / tighter move
budgets than the first-ship numbers.** Measured via the standard seed-sweep method
(`--no-ui --once --variant licorice` for the free-cycling variant; `--release` seed
sweeps of `LEVELS[15]` for "Licorice Lane"), iterating both levers together rather than
one at a time given how far off the starting guess was:

- Free-cycling `Variant::Licorice`: started at 12 cells / 24 moves (near the *old*,
  too-low `MAX_LEVEL_LICORICE` guess) — 90.7% win rate (n=150), far too easy. 20/20 → 62.7%. Then
  overcorrected to 28/15 → 30% (two big lever moves at once, past the band on the low
  side). Settled at **`LICORICE_CELL_COUNT` = 24, `LICORICE_MOVE_LIMIT` = 17**: 55.3%
  (seeds 1-150), 41.3% (seeds 151-300) — averages to a healthy ~48%, consistent with this
  file's other documented range-to-range swings.
- "Licorice Lane" (`LEVELS[15]`, still appended after "Grand Finale" so no other level's
  index moved): the original `Score`-variant numbers are moot now that it's
  `Variant::Licorice`. Started from 14 cells / 22 moves (83% win rate, too easy), tried
  22/18 (62%), settled at **`licorice_cell_count` = 22, `move_limit` = 17**: 52% (seeds
  1-60, the floor test's own range), 51.3% (a disjoint 61-210 check) — both in-band.
  Floor-retested end to end afterward: every other level unchanged, all still ≥ floor.

**A related, more targeted diagnostic on `Variant::Ingredients` — raised while reviewing
this feature — reinforced this file's existing "no reweighting fix" conclusion from a
new angle, rather than contradicting it.** A report that the solver "doesn't seem to
prioritize removing Ingredients" prompted an instrumented sweep distinct from the earlier
documented one: for every turn across 40 episodes, compare the *chosen* move's own
ingredient-progress (rows fallen + collections, via `Game::simulate`) against the *best*
progress achievable by any legally available move that turn. On turns where a
positive-progress move *was* available, the solver picked a zero-progress move anyway
44.6% of the time — and in over a third of those, the chosen move even had lower raw
`score_gained` than the best-progress alternative would have. This is a different failure
mode than the one already ruled out above ("no legal move can advance the goal this
turn is *actually true* on the stalled turns" — that's about turns where *no* progress
move exists at all, a disjoint case from what this diagnostic measured). Tested whether
it's fixable by the obvious lever anyway: `INGREDIENT_PROGRESS_WEIGHT` 180→320 (+78%)
moved the skip rate only 44.6%→43.9% and the win rate only 46.7%→48.0% (n=150 both) —
both comfortably inside this file's own noise floor. Reverted. The skip rate being nearly
invariant to a 78%-larger weight is itself informative: whatever's driving these skips
isn't "the weight is a bit too small," so a further reweighting attempt without a new
angle would very likely reproduce this same null result. Diagnostic code was throwaway
(not committed) — the numbers above are the artifact worth keeping, per this file's usual
practice for one-off tuning sweeps.

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

`GemShape::for_color`: Red=circle, Orange=triangle, Yellow=star (a hexagram — macroquad
has no star primitive), Green=pentagon, Blue=hexagon, Purple=diamond. `draw_gem` layers:
darker backing shape, base-color shape inset, then
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
a square outline. **They hit the exact same gray-under-`RenderCache` bug as the gloss
streak above, reported later** (translucent white faded by `alpha`, drawn regardless of
`highlight`) — fixed identically: `lighten(color_rgb(color), 0.55)`, opaque, gated on
`highlight` rather than scaled by `alpha`. Safe because every call site that passes
`highlight: true` already draws at `alpha: 1.0` (a settled, non-animating tile — see the
call sites in `draw_board_static`/the `else` branches of `draw_flash_live`/
`draw_fall_live`), so nothing fades that needs to. Verified by sampling a live stripe
pixel in a headless-browser screenshot against its gem's base color — matched
`lighten(base, 0.55)` exactly, not the muddy near-gray the bug produced.

**Yellow's hexagram used to be two overlapping `draw_poly` triangles (±90° rotated) —
a different translucency artifact from the two above, not a `RenderCache` one.** Opaque,
the overlap is invisible (paint over paint). At `alpha < 1` (any flash/fade), the shared
hexagonal core gets alpha-blended *twice*, which reads as a visibly brighter patch at the
star's center than the rest of the gem — reported as "the yellow gem looks transparent
with a brighter overlap." Fixed for real, not with an opaque-precompute workaround (this
one hits every alpha<1 draw, cached or not, so precomputing one blended color doesn't fit
— the affected region isn't the whole shape): `draw_hexagram` fills the same 12-vertex
star outline as one non-overlapping triangle fan from the center instead. The 6 outer
tips sit exactly where the two triangles' own vertices already were (30°+60°k); the 6
inner concave points are at 60°k, radius `r / sqrt(3)` — the standard hexagram
inradius/circumradius ratio. A single fill has nothing left to double-blend, at any alpha.
Verified in a headless browser: idle hexagram has no seams between the 12 triangles.

**`ColorBomb`'s backing circle (20,20,26) was nearly indistinguishable from the board's
own cell background (24,22,30)** — with no gem shape underneath and only 6 small sparkle
dots on top, the "orb" itself was essentially invisible, so the tile read as a loose
scatter of dots rather than a solid bomb ("crumbling"). Two-toned it the same way every
colored gem already is (rim `(48,45,62)`, inset fill `(66,62,84)` — both clearly lighter
than the board background) instead of one near-invisible fill. Verified in a headless
browser: a live `ColorBomb` (needs a 5-in-a-row match, caught after ~30s of autoplay)
reads as a cohesive dark orb with a visible rim, not scattered dots.

**`board_cache` now clears to an opaque backdrop instead of transparent
(`RenderCache::with_backdrop`), generalizing the gray-under-`RenderCache` fix instead of
hand-patching a fourth translucent draw** — jelly's underlay (`draw_jelly_underlay`,
`Color::new(0.55, 0.85, 0.95, 0.28)`, drawn straight after `draw_board_frame` inside the
same cached `draw_board_static` closure) hit the same bug class as the gloss streak and
bonus stripes above, reported independently as "the slime background" flickering. Rather
than adding a third `lighten()`-and-gate-on-a-bool workaround, `RenderCache` itself
gained a `with_backdrop(color)` builder (see `lib/render_cache`'s doc comment and the
`project_render_cache` memory) — `board_cache` passes `rgb(60, 60, 75)`, matching
`draw_board_frame`'s own border fill (the first thing its closure draws, covering this
exact rect), so every translucent draw layered on top within that one closure composites
correctly with no further per-draw-call fixes. The gloss-streak/bonus-stripe fixes were
left as they were rather than reverted back to translucent — still correct, just
redundant-but-harmless now. Verified: a live jelly-tinted cell's corner pixel matched the
hand-computed `translucent-over-(24,22,30)` blend exactly (`(56,76,89)`) after the fix,
consistent across many sampled frames; a clean isolated before/after (cached vs. live)
pair specifically for jelly proved hard to pin down via automated play (variant cycling
in a headless browser is timing-flaky), so this is one measurement short of the
gloss-streak/stripe fixes' rigor — the fix itself doesn't depend on that mechanism being
exactly nailed down (see `RenderCache::with_backdrop`'s doc comment for why it's safe
regardless).

**Jelly's underlay redesigned for recognizability, not just correctness — the fix above
made it render its *intended* color, but that intended design (a flat, pale, low-alpha
tint mostly hidden behind the gem's own silhouette) was still hard to actually notice.**
Two changes: a much smaller inset (`1.5px` vs. the old `3px`, so more of the cell's own
footprint stays exposed around whatever gem sits on it) and a darker-rim/lighter-fill
treatment — the same backing/inset layering `draw_gem` gives every gem, instead of one
flat `draw_rectangle`. Picked a teal/green hue (rim `(0.12, 0.47, 0.42)`, fill `(0.24,
0.78, 0.65)`) no gem color uses, so it reads as its own thing rather than a gem-shading
artifact. This is meant to set the visual precedent for future per-cell overlays too
(todo backlog #3's blocker tiles — Frozen, Locked, etc.): a colored rim + lighter fill,
picking a hue distinct from both the 6 gem colors and any other overlay already in play,
rather than a flat tint that competes with whatever gem happens to be sitting on top of
it.

A third layer — a gloss highlight circle matching `draw_gem`'s own — was tried and
dropped: since a gem's silhouette covers most of the cell, jelly's own highlight ended
up sitting right at the gem's edge, half clashing with the gem's own highlight, half
spilling onto the rim. Reported as looking broken rather than additive; rim/fill alone
already reads clearly as jelly.

**`with_backdrop` fixed the *drastic* cache-vs-live gray mismatch, but left a small
(~5-7/255 per channel), fully reproducible residual gap specifically for jelly's
near-opaque (`0.88`-`0.92` alpha) fill/rim — root cause not pinned down (some deeper
render-target color precision/blend quirk past what `with_backdrop` addresses), found by
screenshotting the *same* jelly cell mid-animation (live) vs. settled (cached) with the
now-fixed `S` hotkey (see "WASM caveats" in root CLAUDE.md) and diffing — every jelly
cell read a uniform, reproducible `(30, 111-112, 100)` when cached vs. `(29, 104, 95)`
live, not noise.** Fixed the same way the gloss streak/bonus stripes were before
`with_backdrop` existed: `blend(fg, board_bg, alpha)` precomputes the color in Rust and
draws it at `alpha: 1.0` instead of actually drawing translucent — an opaque draw has no
composite left at draw time to be inconsistent about, cached or not. Confirmed by
re-running the same live-vs-cached screenshot diff: every sampled frame (live and
cached alike) now reads the identical `(30, 112, 100)`.

**Jelly's rim still read as "shifting by a pixel" between cache and live after the color
fix above — turned out to be a real hard-edge-vs-antialiased-edge mismatch, not a
position bug.** Same live-vs-cached screenshot-diff methodology, this time comparing the
full pixel *sequence* across the rim/fill boundary rather than a single sample point:
cached rendering transitioned bg→rim in exactly one pixel every time, live rendering
spread the same transition across two pixels of blended color
(`(24,22,30)→(27,67,65)→(30,112,100)`) — a soft, antialiased edge. Root cause:
`board_cache`'s render target uses `sample_count: 0` (deliberately — see
`RenderCache::new`'s doc comment, real MSAA there trips a WASM crash on some
browsers/GPUs), so cached content rasterizes with zero antialiasing, while live draws
land on the default framebuffer, whose WebGL context (`mq_js_bundle.js`, not ours to
edit) has antialiasing on by default. Fixed with `RenderCache::with_supersample(2)`:
renders into a 2x-sized texture and lets a `Linear`-filtered `blit` shrink it back down,
approximating antialiasing without touching `sample_count` at all — sidesteps the crash
risk entirely rather than trying to work around it. Confirmed by re-running the same
diff: live and cached pixel sequences are now byte-identical across every sampled cell.

**Triangle/Pentagon gems sit visibly high in their cell — a pre-existing bug, unrelated
to jelly, that jelly's new visible rim just made obvious** (reported as "grid
misalignment," but it's per-shape, not per-cell — every gem is drawn at the same `(cx,
cy)`). Root cause: both are drawn "point up" via `draw_poly` with an odd vertex count,
so the top point extends further from the true polygon center than the flatter bottom
edge does — the shape's *bounding box* isn't symmetric around `(cx, cy)` the way
`Diamond`/`Hexagon`/`Circle`/`Star` (all even-vertex-count or otherwise symmetric) are.
Measured directly: cropping a rendered gem and finding its bounding box put `Pentagon`
~2.75px and `Triangle` (by the same geometry, larger since it's a sharper point) further
off than that, matching the derived correction almost exactly. Fixed with
`GemShape::vertical_bias`, a downward correction (derived from each polygon's actual
vertex geometry, not eyeballed) applied once to `cy` in `draw_gem` before any of the
backing/inset/highlight draws, so everything shifts together consistently.

**RenderCache usage differs from every other game here**: match-3's board animates
almost continuously (swap/flash/fall chase each other with no real idle gap *during* a
move), so caching only pays off in the `Idle` beat between moves and the `GameOver`
overlay — `amain` mirrors `game2048`'s exact split (`board_cache.mark_dirty()` every
frame while animating, `board_cache.draw()` only when settled) rather than Tetris's
"cache the locked board, draw the live piece on top" split.

**`Tile::Licorice` draws as a dark rounded block filling the whole cell (rim/fill layers,
same pattern every other tile uses), not a gem silhouette inset into it** — it's a wall
occupying the cell rather than a piece sitting on it, so it needed its own full-cell
treatment rather than `draw_gem`'s shape-in-a-cell one. Two rounds of tuning from real
feedback: the original near-black plum hue (rim `(14,10,18)`, fill `(28,20,34)`) with a
subtle same-hue diagonal double stripe was "doesn't look good, not noticeable" — too
close to the board's own cell background `(24,22,30)` to register at a glance. A first
fix (wine-red block + a flat hazard-tape amber X) solved contrast but lost the licorice
identity — "should still look a little more like licorice". Settled shape: a true
licorice-black backing (rim `(16,11,13)`, fill `(32,23,25)`) with the X arms drawn as
twisted red-licorice rope rather than a flat bar — each arm layers a body stroke
(`168,30,38`), a thinner lighter core sheen (`222,92,92`), and four perpendicular ridge
ticks (`108,16,22`) along its length evoking a rope twist, the same
body/core-highlight two-tone language every other tile in this file uses, just traced
along a line instead of filling a shape. Reads as candy up close, still an unmistakable
high-contrast X at a glance. Drawn at plain `alpha`-scaled opacity like every other tile
(`a(...)`, no `blend()`-precompute treatment) since `board_cache`'s `with_backdrop` already fixes
translucency-under-`RenderCache` generally — the extra `blend()` precompute elsewhere in
this section was a narrow fix for a *residual* gap specific to jelly's near-opaque fill,
not something every future tile needs by default.

## Testing

`game.rs`'s `#[cfg(test)]` module includes `full_playthrough_terminates_for_every_variant`
— a soak test (10 episodes/variant via the real `crate::solver::choose_move`) asserting
termination and that every applied move came from `legal_moves()`. Since `choose_move` is
now the `beam_solver`-backed entry point, each episode constructs a fresh
`solver::new_beam_search()` and passes `&mut beam` in — same per-episode `Beam` lifetime
`Session` follows (a stale `visited` set must never carry across episodes). Worth this
shape of test for any new self-playing game, not just this one.

`licorice_is_unswappable_and_splits_column_gravity`, `licorice_never_matches_or_joins_a_run`,
and `licorice_is_cleared_by_an_adjacent_ordinary_match` cover `Tile::Licorice`'s mechanics
directly (illegal swap, run-breaking, shelf-splitting gravity via a direct
`compact_and_refill` call, and the adjacent-match clearing rule) rather than only through a
full playthrough — the segmented-gravity change in particular is exactly the kind of thing
a soak test could pass while still animating falls through a shelf, since
`full_playthrough_terminates_for_every_variant` only asserts termination and
move-legality, not fall-entry correctness.

## Running

```bash
mise run run match-3                                   # native
mise run build-wasm match-3                             # WASM → dist/match-3/
HCG_SEED=1 target/release/match-3 --no-ui --once --variant score --debug
```
