use macroquad::prelude::*;

const N: usize = 4;
const WIN_W: f32 = 500.0;
const WIN_H: f32 = 610.0;
const GRID_X: f32 = 20.0;
const GRID_Y: f32 = 140.0;
const GRID_W: f32 = WIN_W - GRID_X * 2.0;
const GAP: f32 = 10.0;
const CELL: f32 = (GRID_W - GAP * 5.0) / 4.0;
const ANIM_SPEED: f32 = 4.0;  // reciprocal of slide duration in seconds
const AI_DELAY: f32 = 0.15;   // pause between moves
const POP_DUR: f32 = 0.28;    // spawn/merge pop duration
const WIN_PAUSE: f32 = 2.5;
const OVER_PAUSE: f32 = 2.5;

// --- Colors ---

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255., g as f32 / 255., b as f32 / 255., 1.)
}
fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::new(r as f32 / 255., g as f32 / 255., b as f32 / 255., a as f32 / 255.)
}

fn tile_bg(val: u32) -> Color {
    match val {
        0    => rgb(205, 193, 180),
        2    => rgb(238, 228, 218),
        4    => rgb(237, 224, 200),
        8    => rgb(242, 177, 121),
        16   => rgb(245, 149, 99),
        32   => rgb(246, 124, 95),
        64   => rgb(246, 94, 59),
        128  => rgb(237, 207, 114),
        256  => rgb(237, 204, 97),
        512  => rgb(237, 200, 80),
        1024 => rgb(237, 197, 63),
        2048 => rgb(237, 194, 46),
        _    => rgb(60, 58, 50),
    }
}

fn tile_fg(val: u32) -> Color {
    if val <= 4 { rgb(119, 110, 101) } else { rgb(249, 246, 242) }
}

// --- Board transforms ---

fn flip_h(mut b: [[u32; N]; N]) -> [[u32; N]; N] {
    for row in &mut b { row.reverse(); }
    b
}

fn transpose(b: [[u32; N]; N]) -> [[u32; N]; N] {
    let mut t = [[0u32; N]; N];
    for i in 0..N { for j in 0..N { t[j][i] = b[i][j]; } }
    t
}

fn prep(board: [[u32; N]; N], dir: u8) -> [[u32; N]; N] {
    match dir { 0 => board, 1 => flip_h(board), 2 => transpose(board), _ => flip_h(transpose(board)) }
}

fn unprep(board: [[u32; N]; N], dir: u8) -> [[u32; N]; N] {
    match dir { 0 => board, 1 => flip_h(board), 2 => transpose(board), _ => transpose(flip_h(board)) }
}

// Convert (row, col) from prep-space back to original board coordinates
fn inv_coord(r: f32, c: f32, dir: u8) -> (f32, f32) {
    let n = N as f32 - 1.0;
    match dir { 0 => (r, c), 1 => (r, n - c), 2 => (c, r), _ => (n - c, r) }
}

fn smoothstep(t: f32) -> f32 { let t = t.clamp(0., 1.); t * t * (3. - 2. * t) }

// --- Animation types ---

struct AnimTile {
    fr: f32, fc: f32, // from (row, col)
    tr: f32, tc: f32, // to (row, col)
    val: u32,
    merge: bool, // this tile is absorbed at destination
}

struct Pop {
    row: usize,
    col: usize,
    t: f32,    // 0→1 over POP_DUR
    spawn: bool, // spawn = scale-in; false = merge bounce
}

// --- Slide-left with animation data ---

fn slide_row_anim(row: [u32; N], grid_row: usize) -> (Vec<AnimTile>, [u32; N], u32) {
    let src: Vec<(usize, u32)> = (0..N).filter(|&c| row[c] != 0).map(|c| (c, row[c])).collect();
    let mut new_row = [0u32; N];
    let mut tiles = Vec::new();
    let mut pts = 0u32;
    let (mut i, mut dst) = (0, 0usize);
    let rf = grid_row as f32;
    while i < src.len() {
        if i + 1 < src.len() && src[i].1 == src[i + 1].1 {
            let v = src[i].1 * 2;
            new_row[dst] = v;
            pts += v;
            tiles.push(AnimTile { fr: rf, fc: src[i].0 as f32, tr: rf, tc: dst as f32, val: src[i].1, merge: false });
            tiles.push(AnimTile { fr: rf, fc: src[i+1].0 as f32, tr: rf, tc: dst as f32, val: src[i+1].1, merge: true });
            i += 2;
        } else {
            new_row[dst] = src[i].1;
            tiles.push(AnimTile { fr: rf, fc: src[i].0 as f32, tr: rf, tc: dst as f32, val: src[i].1, merge: false });
            i += 1;
        }
        dst += 1;
    }
    (tiles, new_row, pts)
}

fn slide_left_anim(board: [[u32; N]; N]) -> (Vec<AnimTile>, [[u32; N]; N], u32) {
    let mut new_board = [[0u32; N]; N];
    let mut tiles = Vec::new();
    let mut pts = 0u32;
    for r in 0..N {
        let (rt, nr, rp) = slide_row_anim(board[r], r);
        new_board[r] = nr;
        tiles.extend(rt);
        pts += rp;
    }
    (tiles, new_board, pts)
}

// --- AI heuristic ---

fn rot90(b: [[u32; N]; N]) -> [[u32; N]; N] {
    let mut r = [[0u32; N]; N];
    for i in 0..N { for j in 0..N { r[j][N - 1 - i] = b[i][j]; } }
    r
}

// Snake weight matrix: top-left has highest weight, decreasing along a snake path.
// Score = dot(board, weights); try all 8 orientations, keep the best.
// This directly encodes "big tile in corner, snake pattern down" with a single term.
const SNAKE: [[f64; N]; N] = [
    [32768.0, 16384.0,  8192.0, 4096.0],
    [  256.0,   512.0,  1024.0, 2048.0],
    [  128.0,    64.0,    32.0,   16.0],
    [    1.0,     2.0,     4.0,    8.0],
];

fn heuristic(b: &[[u32; N]; N]) -> f64 {
    let dot = |b: &[[u32; N]; N]| -> f64 {
        (0..N).flat_map(|r| (0..N).map(move |c| b[r][c] as f64 * SNAKE[r][c])).sum::<f64>()
    };
    let b0 = *b;
    let b1 = rot90(b0); let b2 = rot90(b1); let b3 = rot90(b2);
    let bf = flip_h(b0);
    let bf1 = rot90(bf); let bf2 = rot90(bf1); let bf3 = rot90(bf2);
    [b0, b1, b2, b3, bf, bf1, bf2, bf3]
        .iter().map(dot).fold(f64::NEG_INFINITY, f64::max)
}

// Fast allocation-free slide for AI search
fn merge_row_s(row: [u32; N]) -> [u32; N] {
    let mut buf = [0u32; N];
    let mut k = 0usize;
    for &v in &row { if v != 0 { buf[k] = v; k += 1; } }
    let mut i = 0;
    while i + 1 < k {
        if buf[i] == buf[i + 1] {
            buf[i] *= 2;
            for j in i + 1..k - 1 { buf[j] = buf[j + 1]; }
            buf[k - 1] = 0;
            k -= 1;
        }
        i += 1;
    }
    buf
}

fn slide_s(board: [[u32; N]; N], dir: u8) -> Option<[[u32; N]; N]> {
    let p = prep(board, dir);
    let mut sp = p;
    for r in 0..N { sp[r] = merge_row_s(p[r]); }
    if sp == p { None } else { Some(unprep(sp, dir)) }
}

// Depth-5 expectimax; chance nodes place only 2 (90% case) to limit branching.
fn expectimax(board: [[u32; N]; N], depth: u8, player: bool) -> f64 {
    if depth == 0 { return heuristic(&board); }
    if player {
        let mut best = f64::NEG_INFINITY;
        let mut any = false;
        for dir in 0..4u8 {
            if let Some(nb) = slide_s(board, dir) {
                any = true;
                let s = expectimax(nb, depth - 1, false);
                if s > best { best = s; }
            }
        }
        if any { best } else { heuristic(&board) }
    } else {
        let empties: Vec<(usize, usize)> = (0..N)
            .flat_map(|r| (0..N).map(move |c| (r, c)))
            .filter(|&(r, c)| board[r][c] == 0)
            .collect();
        if empties.is_empty() { return heuristic(&board); }
        let n = empties.len() as f64;
        empties.iter().map(|&(r, c)| {
            let mut b2 = board; b2[r][c] = 2;
            expectimax(b2, depth - 1, true) / n
        }).sum()
    }
}

// --- Game ---

struct Game {
    board: [[u32; N]; N],
    next_board: [[u32; N]; N],
    score: u32,
    best: u32,
    pending: u32,
    over: bool,
    won: bool,
    show_win: bool,
    anim_tiles: Vec<AnimTile>,
    anim_t: f32,
    pops: Vec<Pop>,
    ai_wait: f32,
    overlay_timer: f32,
    last_dir: Option<u8>,
}

impl Game {
    fn new(best: u32) -> Self {
        let mut g = Self {
            board: [[0; N]; N], next_board: [[0; N]; N],
            score: 0, best, pending: 0,
            over: false, won: false, show_win: false,
            anim_tiles: Vec::new(), anim_t: 1.0,
            pops: Vec::new(),
            ai_wait: 0.6, overlay_timer: 0.0,
            last_dir: None,
        };
        g.spawn(); g.spawn();
        g
    }

    fn spawn(&mut self) {
        let e: Vec<(usize, usize)> = (0..N)
            .flat_map(|r| (0..N).map(move |c| (r, c)))
            .filter(|&(r, c)| self.board[r][c] == 0)
            .collect();
        if e.is_empty() { return; }
        let (r, c) = e[rand::gen_range(0, e.len())];
        self.board[r][c] = if rand::gen_range(0u32, 10) == 0 { 4 } else { 2 };
        self.pops.push(Pop { row: r, col: c, t: 0.0, spawn: true });
    }

    fn can_move(&self) -> bool {
        for r in 0..N { for c in 0..N {
            if self.board[r][c] == 0 { return true; }
            if r + 1 < N && self.board[r + 1][c] == self.board[r][c] { return true; }
            if c + 1 < N && self.board[r][c + 1] == self.board[r][c] { return true; }
        }}
        false
    }

    fn choose_dir(&self) -> Option<u8> {
        let empty = self.board.iter().flat_map(|r| r.iter()).filter(|&&v| v == 0).count();
        // depth=4 for most of the game so we always see 2 player moves ahead;
        // depth=5 only in tight endgame where branching is naturally low.
        let depth: u8 = match empty {
            0..=3 => 5,
            4..=9 => 4,
            _     => 3,
        };
        let mut best = f64::NEG_INFINITY;
        let mut result = None;
        for dir in 0..4u8 {
            if let Some(nb) = slide_s(self.board, dir) {
                let s = expectimax(nb, depth, false);
                if s > best { best = s; result = Some(dir); }
            }
        }
        result
    }

    fn start_move(&mut self, dir: u8) -> bool {
        let p = prep(self.board, dir);
        let (mut tiles, new_p, pts) = slide_left_anim(p);
        if new_p == p { return false; }
        for t in &mut tiles {
            let (fr, fc) = inv_coord(t.fr, t.fc, dir);
            let (tr, tc) = inv_coord(t.tr, t.tc, dir);
            t.fr = fr; t.fc = fc; t.tr = tr; t.tc = tc;
        }
        self.next_board = unprep(new_p, dir);
        self.pending = pts;
        self.anim_tiles = tiles;
        self.anim_t = 0.0;
        self.last_dir = Some(dir);
        true
    }

    fn commit(&mut self) {
        // Track merge destinations for bounce pop
        let mut merge_dests = [[false; N]; N];
        for t in &self.anim_tiles {
            if t.merge { merge_dests[t.tr as usize][t.tc as usize] = true; }
        }
        self.score += self.pending;
        self.best = self.best.max(self.score);
        self.board = self.next_board;
        self.anim_t = 1.0;
        self.anim_tiles.clear();
        for r in 0..N { for c in 0..N {
            if merge_dests[r][c] { self.pops.push(Pop { row: r, col: c, t: 0.0, spawn: false }); }
        }}
        self.spawn();
        if !self.won && self.board.iter().any(|row| row.iter().any(|&v| v >= 2048)) {
            self.won = true;
            self.show_win = true;
            self.overlay_timer = WIN_PAUSE;
        }
        if !self.can_move() {
            self.over = true;
            self.overlay_timer = OVER_PAUSE;
        }
    }

    // Returns true if the caller should replace self with Game::new(best)
    fn update(&mut self, dt: f32) -> bool {
        for p in &mut self.pops { p.t += dt / POP_DUR; }
        self.pops.retain(|p| p.t < 1.0);

        if self.show_win {
            self.overlay_timer -= dt;
            if self.overlay_timer <= 0.0 { self.show_win = false; self.ai_wait = 0.3; }
            return false;
        }
        if self.over {
            self.overlay_timer -= dt;
            if self.overlay_timer <= 0.0 { return true; }
            return false;
        }
        if self.anim_t < 1.0 {
            self.anim_t = (self.anim_t + dt * ANIM_SPEED).min(1.0);
            if self.anim_t >= 1.0 { self.commit(); self.ai_wait = AI_DELAY; }
        } else if self.ai_wait > 0.0 {
            self.ai_wait -= dt;
        } else if let Some(dir) = self.choose_dir() {
            self.start_move(dir);
        } else {
            self.over = true;
            self.overlay_timer = OVER_PAUSE;
        }
        false
    }
}

// --- Draw helpers ---

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    draw_rectangle(x + r, y, w - 2.0 * r, h, color);
    draw_rectangle(x, y + r, w, h - 2.0 * r, color);
    draw_circle(x + r, y + r, r, color);
    draw_circle(x + w - r, y + r, r, color);
    draw_circle(x + r, y + h - r, r, color);
    draw_circle(x + w - r, y + h - r, r, color);
}

fn tile_cell(cx: f32, cy: f32, val: u32, scale: f32, alpha: f32) {
    let bg = tile_bg(val);
    let bg = Color { a: bg.a * alpha, ..bg };
    let s = CELL * scale;
    let ox = cx + (CELL - s) / 2.0;
    let oy = cy + (CELL - s) / 2.0;
    rrect(ox, oy, s, s, (4.0 * scale).max(1.0), bg);
    if val > 0 {
        let fg = tile_fg(val);
        let fg = Color { a: fg.a * alpha, ..fg };
        let text = val.to_string();
        let base_fs: u16 = match text.len() { 1 | 2 => 52, 3 => 42, _ => 34 };
        let fs = (base_fs as f32 * scale).max(1.0) as u16;
        let d = measure_text(&text, None, fs, 1.0);
        draw_text(&text, ox + (s - d.width) / 2.0, oy + (s + d.height) / 2.0, fs as f32, fg);
    }
}

fn grid_xy(row: f32, col: f32) -> (f32, f32) {
    (GRID_X + GAP + col * (CELL + GAP), GRID_Y + GAP + row * (CELL + GAP))
}

fn score_box(x: f32, y: f32, w: f32, h: f32, label: &str, val: u32) {
    rrect(x, y, w, h, 4.0, rgb(187, 173, 160));
    let ld = measure_text(label, None, 13, 1.0);
    draw_text(label, x + (w - ld.width) / 2.0, y + 18.0, 13.0, rgb(238, 228, 218));
    let s = val.to_string();
    let fs: u16 = if val >= 10000 { 18 } else if val >= 1000 { 22 } else { 26 };
    let vd = measure_text(&s, None, fs, 1.0);
    draw_text(&s, x + (w - vd.width) / 2.0, y + h - 10.0, fs as f32, WHITE);
}

fn draw_equilateral(cx: f32, cy: f32, r: f32, dir: u8, color: Color) {
    let s = r * 0.866; // sqrt(3)/2 — half the base width
    let (v1, v2, v3) = match dir {
        0 => (vec2(cx - r, cy),  vec2(cx + r*0.5, cy - s), vec2(cx + r*0.5, cy + s)), // left
        1 => (vec2(cx + r, cy),  vec2(cx - r*0.5, cy - s), vec2(cx - r*0.5, cy + s)), // right
        2 => (vec2(cx, cy - r),  vec2(cx - s, cy + r*0.5), vec2(cx + s, cy + r*0.5)), // up
        _ => (vec2(cx, cy + r),  vec2(cx - s, cy - r*0.5), vec2(cx + s, cy - r*0.5)), // down
    };
    draw_triangle(v1, v2, v3, color);
}

fn window_conf() -> Conf {
    Conf {
        window_title: "2048 AI".to_string(),
        window_width: WIN_W as i32,
        window_height: WIN_H as i32,
        window_resizable: false,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    rand::srand(macroquad::miniquad::date::now() as u64);
    let mut game = Game::new(0);

    loop {
        let dt = get_frame_time();
        if game.update(dt) {
            let best = game.best;
            game = Game::new(best);
        }

        // --- Draw ---
        clear_background(rgb(250, 248, 239));

        // Title + direction triangle
        let td = measure_text("2048", None, 72, 1.0);
        draw_text("2048", 15.0, 78.0, 72.0, rgb(119, 110, 101));
        if let Some(dir) = game.last_dir {
            // Vertically centred with the digit glyphs (baseline 78, cap-height ~49px)
            draw_equilateral(15.0 + td.width + 28.0, 54.0, 20.0, dir, rgb(143, 122, 102));
        }

        // Score boxes
        score_box(300.0, 8.0, 90.0, 60.0, "SCORE", game.score);
        score_box(400.0, 8.0, 90.0, 60.0, "BEST", game.best);

        // Grid background
        rrect(GRID_X, GRID_Y, GRID_W, GRID_W, 6.0, rgb(187, 173, 160));

        // Tiles
        let animating = game.anim_t < 1.0;
        if animating {
            // Mark source positions so we don't double-draw
            let mut from_grid = [[false; N]; N];
            for t in &game.anim_tiles {
                from_grid[t.fr.round() as usize][t.fc.round() as usize] = true;
            }
            // Static tiles (didn't move)
            for r in 0..N { for c in 0..N {
                let (tx, ty) = grid_xy(r as f32, c as f32);
                if from_grid[r][c] {
                    tile_cell(tx, ty, 0, 1.0, 1.0); // draw empty slot
                } else {
                    tile_cell(tx, ty, game.board[r][c], 1.0, 1.0);
                }
            }}
            // Moving tiles at interpolated positions
            let t = smoothstep(game.anim_t);
            for tile in &game.anim_tiles {
                let r = tile.fr + (tile.tr - tile.fr) * t;
                let c = tile.fc + (tile.tc - tile.fc) * t;
                let (tx, ty) = grid_xy(r, c);
                let alpha = if tile.merge { (1.0 - t * t * t).max(0.0) } else { 1.0 };
                tile_cell(tx, ty, tile.val, 1.0, alpha);
            }
        } else {
            // Final board with pop animations
            for r in 0..N { for c in 0..N {
                let (tx, ty) = grid_xy(r as f32, c as f32);
                let val = game.board[r][c];
                let scale = game.pops.iter()
                    .find(|p| p.row == r && p.col == c)
                    .map(|p| {
                        let t = p.t.clamp(0., 1.);
                        if p.spawn {
                            // Scale in from 0
                            (t * std::f32::consts::PI / 2.0).sin()
                        } else {
                            // Merge bounce: 1 → 1.2 → 1
                            1.0 + 0.2 * (t * std::f32::consts::PI).sin()
                        }
                    })
                    .unwrap_or(1.0);
                tile_cell(tx, ty, val, scale, 1.0);
            }}
        }

        // Win overlay (auto-dismisses)
        if game.show_win {
            draw_rectangle(GRID_X, GRID_Y, GRID_W, GRID_W, rgba(255, 243, 108, 185));
            let txt = "You win!";
            let d = measure_text(txt, None, 64, 1.0);
            let cx = GRID_X + GRID_W / 2.0;
            let cy = GRID_Y + GRID_W / 2.0;
            draw_text(txt, cx - d.width / 2.0, cy - 30.0, 64.0, rgb(119, 110, 101));
            let sub = format!("Continuing in {:.0}...", game.overlay_timer.max(0.0));
            let sd = measure_text(&sub, None, 20, 1.0);
            draw_text(&sub, cx - sd.width / 2.0, cy + 15.0, 20.0, rgb(119, 110, 101));
        }

        // Game over overlay (auto-restarts)
        if game.over && !game.show_win {
            draw_rectangle(GRID_X, GRID_Y, GRID_W, GRID_W, rgba(199, 187, 177, 190));
            let txt = "Game over!";
            let d = measure_text(txt, None, 60, 1.0);
            let cx = GRID_X + GRID_W / 2.0;
            let cy = GRID_Y + GRID_W / 2.0;
            draw_text(txt, cx - d.width / 2.0, cy - 30.0, 60.0, rgb(119, 110, 101));
            let sc = format!("Score: {}", game.score);
            let scd = measure_text(&sc, None, 26, 1.0);
            draw_text(&sc, cx - scd.width / 2.0, cy + 10.0, 26.0, rgb(119, 110, 101));
            let sub = format!("Restarting in {:.0}...", game.overlay_timer.max(0.0));
            let sd = measure_text(&sub, None, 18, 1.0);
            draw_text(&sub, cx - sd.width / 2.0, cy + 40.0, 18.0, rgb(119, 110, 101));
        }

        next_frame().await;
    }
}
