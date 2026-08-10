# Snake

AI snake game. No player input — the snake drives itself. Grid 40×30, window 900×720.

## Source layout

| File | Contents |
|------|----------|
| `src/main.rs` | Constants, `Pt`, `conf()`, render loop, RNG seed |
| `src/game.rs` | `Game` struct, `tick()`, AI (`choose_dir` → `bfs_to` → `time_flood` → `max_space_dir`) |
| `src/blocks.rs` | `generate_blocks()`, `is_valid_layout()`, `no_articulation_points()` |

## Key types and constants

```rust
const COLS: i32 = 40; const ROWS: i32 = 30;
const GRID: usize = 1200;          // COLS * ROWS
const BLOCK_SENTINEL: u16 = u16::MAX - 1;  // marks blocks in body_grid
const DIRS: [(i32,i32); 4]         // cardinal directions
struct Pt { x, y }                 // grid coordinate; .idx() → flat index
struct Game { body: VecDeque<Pt>, dir, food, score, generation, blocks: [bool; GRID] }
```

`body[0]` = head. `body_grid()` returns `[u16; GRID]`:
- `u16::MAX` = empty
- `BLOCK_SENTINEL` = static block
- `j as u16` = body segment index `j` (used for time-aware passability)

## AI strategy (`game.rs`)

`choose_dir` runs each tick:

1. **BFS to food** (`bfs_to`) — time-aware: body segment `j` vacates at step `n−j`.
2. **Safety check** — `time_flood` from food must see `> n` free cells after arriving; `bfs_to(food → tail)` must also succeed (prevents row-filling traps that partition the grid).
3. **Tail chase** — if food path is unsafe, BFS to tail (space opens as snake follows itself).
4. **Max space** — fallback: pick direction with most reachable cells via `time_flood`.

**Desperation is two-stage, and the first stage must stay tail-guarded.** After `2n` ticks
without food the snake stops requiring the food route to be *comfortable* (step 2's flood
test) but still requires `bfs_to(next → tail)` to succeed. Dropping that guard — taking the
greedy food direction with no safety check at all — was the sole cause of essentially every
death: the fatal tick always had *zero* legal moves (the head was already walled in), and the
move that sealed it was an unguarded dash. Guarding it took mean score 269 → 374 over 100
seeds (`--no-ui --once`, `HCG_SEED=1..100`).

But the guard alone livelocks: with no escape at all, 59/100 seeds froze (score stuck at 429
for 1M+ ticks) tail-chasing in a cycle they could never safely leave. Hence the second stage
at `6n`, where the unguarded dash is allowed again — dying eventually beats never progressing.
The `6n` figure is not sensitive (3n/4n/8n/12n all land within noise of each other); the
*existence* of the escape is what matters. Two related off-by-ones were measured and rejected
as non-causes: `max_space_dir`'s `best = self.dir` default (reachable, but only ever with
`legal=[]`, i.e. the snake was already dead) and making the flood/space tests eat-aware
(`n+1` cells needed, tail doesn't vacate when the candidate cell is the food) — worth 0pp.

`DESPERATE_TICKS_MULT`/`STARVING_TICKS_MULT` (both `pub` in `game.rs`) are these two
thresholds, `2` and `6`. `main.rs` reads them to drive the head's hunger tint (blue →
white at `2n` ticks hungry, white → purple at `6n`; body segments stay plain blue) so
the on-screen color tracks the AI's actual risk stage instead of an independently-tuned
scale that could drift from it.

## Block generation (`blocks.rs`)

Each generation: up to 2000 attempts to place 4–8 random rectangles (2–4 wide, 2–3 tall).

Exclusion zone: no block within ±5 x / ±4 y of the centre (spawn point at `COLS/2, ROWS/2`).

A layout is accepted only if `is_valid_layout` passes:
1. **Connectivity** — flood fill must reach every passable cell.
2. **No articulation points** — Tarjan's iterative DFS; any cell whose removal disconnects the passable graph → reject. This subsumes dead-end detection.

Fallback: `[false; GRID]` (no blocks) if 2000 attempts all fail.

## RNG

```rust
let mut control = control::Control::new();
rand::srand(control.seed());  // before Game::new
```

`Control::seed` is `HCG_SEED` (native override) → day-hash (when `?daily=1` /
`control.daily_mode()`, see root CLAUDE.md's pilot notes) → wall-clock, in that order.
`std::time::SystemTime::now()` panics on WASM, which is why even the wall-clock fallback
goes through `macroquad::miniquad::date::now()` rather than `SystemTime`. `Control` has
to be constructed before this call so its `daily_mode()` read (from the page's `?daily=1`
query param) is available in time to pick the right seed path.

## Running

```bash
mise run run snake          # native
mise run build-wasm snake   # WASM → dist/snake/
mise run serve              # http://localhost:8080
```
