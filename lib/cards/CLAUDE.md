# lib/cards

Shared 52-card deck model and rendering, used by `games/klondike` and `games/spider`.

## Source layout

| File | Contents |
|------|----------|
| `src/card.rs` | `Card { rank: u8, suit: u8 }` (rank 0=Ace..12=King, suit 0=♣ 1=♦ 2=♥ 3=♠), `shuffled_deck()` |
| `src/render.rs` | `draw_card_face`/`draw_card_back`/`draw_empty_slot`, suit-symbol drawing |

`Card::is_red()`/`color_bit()` — suits 1/2 (♦/♥) are red; used for alternating-color
tableau legality checks in both games.

## Suit-symbol rendering (`render.rs`)

Each suit (`draw_diamond`/`draw_heart`/`draw_spade`/`draw_club`) is hand-drawn via
triangle fans and beziers, not a font glyph. Lessons from iterative refinement, still
load-bearing for the current implementation:

- **Fan concave shapes from the wide end, not the point.** Fanning a concave bezier
  outline (spade/club base) from the apex leaves the bottom edge with ~zero-width
  coverage except at the two corners, forcing a bottom fill triangle that overpaints
  the concavity and makes the shape read as a plain triangle. Fan from `(cx, bbot)`
  (the wide end) instead — the bottom edge becomes each triangle's natural base, no
  fill triangle needed. `draw_spade`/`draw_club` do this.
- **Use a bezier for the heart's bottom curve, not the raw parametric heart formula**
  (`x = 16sin³t`). Parametric heart curvature is under 1px at symbol size (`x` grows as
  `u³` near the tip while `y` grows as `u²`, so the sides go nearly straight) — a bezier
  with its control point on the center axis gives genuinely visible concavity instead.
- Don't add a straight bottom-fill triangle after correct bottom-fanned bezier fans —
  it cancels the concavity the fans were drawn to show.

`draw_suit_symbol_flipped` mirrors a symbol for the card's upside-down corner index.

## Running

No standalone binary — exercised through `games/klondike` and `games/spider`
(`mise run run klondike` / `mise run run spider`).
