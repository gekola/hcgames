# Klondike

Self-playing Klondike (American Patience). `game.rs`/`solver.rs`/`main.rs` split per
root CLAUDE.md's "Self-playing solver games" section — first game in that family; card
primitives (`Card`, suit rendering, card face/back drawing) were extracted to
`lib/cards` here for reuse by Spider (and any future FreeCell/etc.). Suit-symbol
rendering itself (bezier-based club/spade stems, heart/spade parametric curve, diamond)
lives entirely in `lib/cards/src/render.rs`, not here — see that file directly, there's
no per-game suit-rendering code to document.

## Variants

`Variant::Klondike` (standard tableau deal, stock/waste) or `Variant::Yukon` (full
52-card deck dealt straight into the tableau, no stock/waste). `V` cycles
Draw-1 → Draw-3 → Auto → Yukon → Draw-1 (`Auto` only alternates Draw-1/Draw-3 by
generation, same as before Yukon existed — Yukon is explicit-select only, folded into
the existing cycle rather than a dedicated hotkey).

Yukon's signature rule (move any face-up card plus everything stacked on it, regardless
of internal order) needed zero changes to `Game::legal_moves()`'s tableau-to-tableau
search — it already only checks that the *bottom* card of the moved group fits the
target pile, never that the group itself is an ordered run.

`--variant <1|3|auto|yukon>` pins the starting variant (see root CLAUDE.md's CLI flags
table for the flag itself).

## Solver (`solver.rs`)

Beam search via `lib/beam_solver`: `BEAM_WIDTH = 8`, `BEAM_DEPTH = 5`, `NODE_BUDGET =
usize::MAX` (effectively disabled — Klondike hasn't needed the pathological-branching
safety net Spider has; Yukon's branching factor is much higher than Klondike's other
variants but measured fine in practice: release build avg 0.35ms/move, max 0.94ms
across 6000+ Yukon moves).

`is_pointless` combines three checks:
- `is_pointless_tableau_move` — vetoes a `TableauToTableau` move unless it uncovers a
  face-down card, exposes a foundation-ready card, or empties a pile with a King
  available to fill it. **Skipped entirely for Yukon** (`game.variant != Variant::Yukon`
  gate) — Yukon has no stock to fall back on while waiting for a useful move, so it
  routinely *requires* lateral shuffles this function would otherwise veto; the beam
  search's own revisit exclusion is what keeps Yukon from thrashing instead.
- `is_pointless_king_swap` — a King-led run with nothing below it (no face-downs) moved
  onto an empty column just relabels which column is empty. Applies to **both**
  variants unconditionally, unlike the above — pulled into its own function specifically
  because it's the one rule inside `is_pointless_tableau_move` that's still meaningful
  for Yukon even though the rest of that function is skipped there. If touching either
  function, remember this split: don't assume a whole veto function is variant-gated
  just because its caller gates most of it.
- `is_pointless_foundation_return` — pulling a card off the foundation only earns its
  keep if it immediately enables uncovering a face-down card or exposing a
  foundation-ready one (checked via a one-move preview).

Win rate (release, 300 headless games/variant): Draw-1 ~52%, Draw-3 ~15%, Yukon ~40%.
Re-check with a fresh seed sweep (`--release --no-ui --once --variant <v>` across a
`HCG_SEED` range) after any change to `is_pointless`/scoring — see root CLAUDE.md's
"Native CLI flags" section for the general pattern.

## Stuck detection (`Game::check_phase`)

- Draw-1: gives up after 3 full stock laps with no productive move, or 2000 total moves.
- Draw-3: 6 laps or 8000 moves (more laps allowed since cards are harder to find 3 at a
  time).
- Yukon: no stock to cycle, so `no_progress` never advances — cap on move count alone
  (4000).

All three also flip to `Phase::Stuck` immediately if `legal_moves()` is empty, rather
than waiting on the counters.

## Key types (`game.rs`)

```rust
Game::new(variant: Variant, generation: u32, draw_count: u8) -> Self
Game::legal_moves(&self) -> Vec<Move>
Game::apply(&mut self, m: Move)
Game::state_hash(&self) -> u64
Game::n_down: [usize; 7]   // face-down card count per tableau pile
```

## Running

```bash
mise run run klondike
mise run build-wasm klondike
target/release/klondike --no-ui --once --debug --variant yukon   # (build --release first)
```
