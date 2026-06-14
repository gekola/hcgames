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

## Block generation (`blocks.rs`)

Each generation: up to 2000 attempts to place 4–8 random rectangles (2–4 wide, 2–3 tall).

Exclusion zone: no block within ±5 x / ±4 y of the centre (spawn point at `COLS/2, ROWS/2`).

A layout is accepted only if `is_valid_layout` passes:
1. **Connectivity** — flood fill must reach every passable cell.
2. **No articulation points** — Tarjan's iterative DFS; any cell whose removal disconnects the passable graph → reject. This subsumes dead-end detection.

Fallback: `[false; GRID]` (no blocks) if 2000 attempts all fail.

## RNG

```rust
rand::srand(macroquad::miniquad::date::now() as u64);  // in main(), before Game::new
```

`std::time::SystemTime::now()` panics on WASM — always use `miniquad::date::now()`.

## Running

```bash
mise run run snake          # native
mise run build-wasm snake   # WASM → dist/snake/
mise run serve              # http://localhost:8080
```
