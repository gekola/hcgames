---
name: analyze-algo-performance
description: >
  Benchmark and diagnose a self-playing game's solver (win rate, stuck/failure
  patterns, per-move latency) in the hcg workspace. Use when asked to check/benchmark
  a game's AI/solver/algorithm, investigate a win rate, explain why a game gets stuck
  or loops, or compare a solver change before/after.
---

# Analyzing solver/algorithm performance

Assumes every self-playing game in this workspace (klondike, spider, and any future
one built the same way — see CLAUDE.md's "Self-playing solver games" section) supports
`--no-ui --debug --once --variant <...>` and reads `HCG_SEED` from the environment for
a reproducible deal. If the target game is missing any of these, add them first
following klondike's or spider's `main.rs` (`parse_cli_args`/`run_headless`) as the
reference implementation — don't try to benchmark through the windowed path.

## 0. Build `--release` first

Debug builds have measured ~100x slower per-move solver cost than release for these
games (beam search specifically: release ~0.35ms/move avg, debug visibly much slower).
Always `cargo build --release --bin <game>` before measuring anything — a debug-build
number is not informative and will make sweeps take drastically longer than necessary.

## 1. Discover the game's variants from source — don't assume

Variant sets differ per game and change over time (klondike currently has `1`/`3`/
`auto`/`yukon`; spider has `1`/`2`/`4`/`auto` — don't copy one game's list onto another,
and don't trust this file to be up to date either). Get the ground truth from the
game's own `parse_cli_args` in `main.rs`:

```bash
grep -A 20 '"--variant"' games/<game>/src/main.rs
```

That match block enumerates every accepted `--variant` string. Also check the variant
mode enum's cycling method (klondike/spider both call it `next()`) for which variants
the *automatic* rotation actually reaches vs which are only reachable by explicit
`--variant`/hotkey select — klondike's Yukon, for example, is in the `V` cycle but never
chosen by `Auto`'s own generation-based alternation. That distinction matters when
interpreting results: an explicit-only variant won't show up in a sweep that only ever
runs the game's default startup mode, so make sure the sweep passes `--variant`
explicitly for every discovered variant rather than relying on the default.

## 2. Win-rate / outcome sweep — one variant at a time

Don't fold every discovered variant into one opaque `for v in ...` loop. Run one
variant's sweep, look at its result (and its log — see "Benchmark logging convention"
below), *then* move to the next variant from step 1's list. A single all-variants loop
hides which variant is slow enough to need a smaller N, which one is actually worth
digging into with step 3, and makes it easy to silently under-log or skip one.

Per variant:

```bash
BIN=target/release/<game>
VARIANT=<one variant string from step 1>
N=300   # 100-300 is a reasonable default; drop it for a variant that's clearly slow
LOGDIR="tmp/bench/<game>"
mkdir -p "$LOGDIR"
RUNLOG="$LOGDIR/${VARIANT}-$(date +%Y%m%dT%H%M%S).log"

won=0; stuck=0; total=0
for s in $(seq 1 "$N"); do
  total=$((total+1))
  out=$(HCG_SEED=$s "$BIN" --variant "$VARIANT" --once --no-ui)
  echo "seed=$s $out" >> "$RUNLOG"
  if echo "$out" | grep -q result=won; then won=$((won+1)); else stuck=$((stuck+1)); fi
done

winrate=$(echo "scale=1; 100*$won/$total" | bc)
summary="$(date -Iseconds) game=<game> variant=$VARIANT seeds=1-$N build=release won=$won stuck=$stuck total=$total winrate=${winrate}%"
echo "$summary"
echo "$summary" >> "$LOGDIR/summary.log"
```

Each `--once` invocation pays process-spawn overhead (~10ms+, dominated by the
macroquad-linked binary's dynamic loading even though `--no-ui` skips the window) —
that's fine for a few hundred runs but don't read too much into total wall time as a
performance number; see step 4 for real per-move cost instead.

For quick single-process throughput (no spawn overhead, useful for a fast "did this
regress a lot" check) run without `--once` and count result lines over a fixed
duration: `timeout <secs> env HCG_SEED=<n> "$BIN" --variant <v> --no-ui | wc -l`.

## Benchmark logging convention

- **Raw per-seed output**: `tmp/bench/<game>/<variant>-<timestamp>.log`, one
  `seed=<n> result=...` line per run. Lets you grep a specific seed's outcome, or diff
  two runs' full seed-by-seed results, without re-running anything.
- **Running summary**: `tmp/bench/<game>/summary.log`, one line per sweep in the format
  shown in step 2 (`date game variant seeds build won stuck total winrate`).
  **Append-only — never overwrite it.** `tail`/`grep`/`diff` against it to see history
  across sweeps within the session (and across sessions, as long as `tmp/` survives).
- `tmp/` is gitignored (see the temp-files memory) — these logs are scratch, not
  committed, and not guaranteed to survive between sessions. They're the detailed
  backing data for a sweep, not a substitute for step 5's durable memory update.
- Before starting a new sweep, check whether `tmp/bench/<game>/summary.log` already
  has a recent entry for this variant/seed-range (or check memory for a
  `project_<game>_bench_baseline`-style entry) rather than re-deriving a baseline
  that's already been measured this session.

## 3. Diagnose a specific loss

Don't guess from a one-line `result=stuck` — re-run that exact seed with `--debug` and
read the *whole* trace (or at least a long tail), not just the last few lines. Known
failure signatures to look for, in roughly the order to check:

- **Stock-cycling dead loop** (Klondike-style games): `DrawStock`/`ResetStock` repeating
  with the progress metric (foundation count, completed runs, etc.) completely flat.
  Usually a genuine dead end from this solver's search depth, not a bug — confirm the
  score/progress metric truly never moves across the whole repeating stretch.
- **Large-block thrashing** (seen in Klondike Yukon): a long run of big
  `TableauToTableau`-style moves shuffling largely the same cards between a handful of
  piles with the uncovered/face-down count never changing. Distinct from an exact
  state-hash revisit (which the solver already hard-excludes) — these are *structurally
  different* board states each time, just not converging on anything, and won't be
  caught by revisit exclusion alone.
- **A STALL dump showing an obviously-good move that didn't get picked**: check whether
  a pointless-move filter or the revisit-hash exclusion ate it. Print the raw
  (pre-filter) `legal_moves()` list at the stall point if the game's `--debug` doesn't
  already do this (klondike's does — dumps raw candidates on stall).

Compare against any stored baseline before concluding something regressed — check
`tmp/bench/<game>/summary.log` for an earlier entry at the same seed range/variant this
session, and memory for a `project_<game>_bench_baseline`-style entry for anything
older, since a few-percentage-point swing is often normal variance from a different
seed set or sample size, not a real change.

## 4. Per-move latency

Coarse: single-process throughput from step 2's non-`--once` variant, divided by moves
per game (from `--debug` output) if you need ms/move rather than ms/game.

Precise: add a throwaway `#[cfg(test)]` block in `solver.rs` that wraps
`Solver::choose_move` calls in `std::time::Instant`, runs it across a handful of games
of the slowest/highest-branching-factor variant, and reports avg/max ms — run with
`cargo test --release -p <game> <test_name> -- --nocapture` (release matters here too).
**Remove the throwaway test before finishing** unless asked to keep it as permanent
tooling; it's diagnostic scaffolding, not part of the solver.

## 5. Report

State win rate per variant (with N and seed range), any failure pattern found and
whether it's an expected/known limitation vs a new regression, and per-move latency if
it was measured. If the finding is durable (a new known limitation, a measured
baseline, a fixed bug whose cause wasn't obvious), save it to memory and consider
whether CLAUDE.md needs updating — both per the "After a significant change" section
already in CLAUDE.md.
