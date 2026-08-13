# Spider

Self-playing Spider solitaire, `game.rs`/`solver.rs`/`lib.rs` split (see root
CLAUDE.md's "Self-playing solver games"). 10 tableau columns, 104-card deck, 3 variants
(1/2/4 distinct suits — 8/4/2 copies of each rank respectively), cycled via `V` or
pinned with `--variant <1|2|4|auto>` (default `auto`, rotates by `generation % 3`).

## Source layout

| File | Contents |
|------|----------|
| `src/game.rs` | `Game`, `Move`, `Phase`, `legal_moves`/`apply`, `suited_run_len`, `state_hash` |
| `src/solver.rs` | `Solver` (wraps `beam_solver::BeamSearch<Game>`), `is_pointless`, `score`/`score_core`, `diagnose_stuck` |
| `src/lib.rs` | CLI (`--variant`), `run_headless`, render loop, `VariantMode` V-cycle |
| `src/main.rs` | Thin standalone binary — `spider::start()` and nothing else |

## Solver

`beam_solver::BeamSearch<Game>` (see `lib/beam_solver`'s own doc for the shared engine)
at `BEAM_WIDTH=32`, `BEAM_DEPTH=8`, `NODE_BUDGET=1500` (all in `solver.rs`, each with a
comment on why that exact number). `Move::Deal` is the search's carry-forward-terminal
predicate (matches `beam_solver::BeamSearch::new`'s 4th arg).

Wider search (tried up to width=64/depth=12) measurably raises win rate further but was
**not shipped** — per-decision latency became unpredictable at higher settings (a
565ms outlier against a ~27ms baseline was observed, though follow-up investigation
concluded most of that specific spike was dev-sandbox jitter, not the algorithm itself —
no isolated confirmation either way). `NODE_BUDGET` is what actually bounds worst-case
cost now; raise it before raising `width`/`depth` if revisiting this. **No in-browser
WASM latency has ever been measured for this game** — all tuning here used native
release timings, and WASM runs measurably slower; get a real browser measurement before
trusting any further native-only tuning.

`is_pointless` (in `solver.rs`) has three rules, each with a full doc comment on the
function explaining the current condition, the failed attempts that preceded it, and
why — read that doc comment, not this file, for the exact logic; it's substantial and
kept next to the code on purpose. Summary: (1) relocating an already-fully-exposed pile
between empty columns is a no-op; (2) peeling a partial slice off a same-suit run is
pointless unless the destination run ends up strictly longer than the source's own run
length was; (3) only the lowest-indexed empty column is ever generated as a landing
spot (every empty column is interchangeable). `score`/`score_core` price everything
else `is_pointless` doesn't filter, rather than filtering more aggressively —
over-filtering previously starved the solver of legitimate moves.

**Known open gap** (documented in `is_pointless`'s own doc comment, rule 2 preamble):
a partial peel into an *empty* column is always pointless, even when it would
genuinely reduce the empty-column count and unlock a `Deal` — this can produce a false
`Stuck` with `stock` still non-empty. Two fix attempts were tried and reverted after
A/B regressing win rate badly (see the doc comment for exact numbers); no narrower
condition has been found yet. Don't re-attempt without measuring — this has already
regressed twice.

**Win rates** (30-seed A/B at the shipped config, `--no-ui --once`): 1-suit ~21/30,
2-suit ~16/30, 4-suit ~3/30. 4-suit is a genuine difficulty ceiling (2 copies of each
rank/suit vs. 8/4 for 1-/2-suit — same-suit consolidation is much rarer), matching real
Spider difficulty, not a bug — but has improved substantially from an early ~0/30.
**Any change to `is_pointless`, `score`/`score_core`, or the beam config must be
re-verified with a 30-seed × 3-variant A/B before considering it done** — several
changes that looked obviously correct by inspection measured as regressions.

## Debugging a `Stuck` report

`--debug` makes `Solver::choose_move` call `diagnose_stuck` right when no move is
found — dumps the full board, every column's `n_up`/`suited_run_len`, and the *actual*
`is_pointless` verdict (not a hand-traced guess) for every raw legal move:

```bash
spider --no-ui --once --debug --variant <1|2|4>
```

Prefer this over reasoning about the rules on paper — hand-tracing `is_pointless`'s
behavior has been wrong more than once in this game's history, including a case where
"reverting to the exact prior behavior" produced a materially different result for
reasons never fully explained. If a process looks alive-but-stuck (steady CPU, no
progress) rather than crashed, `gdb -p <pid> -batch -ex "thread apply all bt"`
distinguishes a genuine infinite loop from slow convergence from waiting on vsync
(glX/libgallium frames in the backtrace are normal idle, not a bug) — this caught a
real `usize` underflow in flying-card position math once (release builds don't panic on
overflow, so the near-`u64::MAX` result looked exactly like a hang).

## Running

```bash
mise run run spider                                  # native
mise run build-wasm spider                            # WASM → dist/spider/
spider --no-ui --once --debug --variant 1             # headless, one game, verbose
```
