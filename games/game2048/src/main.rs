use game2048::{N, can_move, choose_dir, prep, unprep};
use macroquad::prelude::*;

const WIN_W: f32 = 500.0;
const WIN_H: f32 = 610.0;
const GRID_X: f32 = 20.0;
const GRID_Y: f32 = 140.0;
const GRID_W: f32 = WIN_W - GRID_X * 2.0;
const GAP: f32 = 10.0;
const CELL: f32 = (GRID_W - GAP * 5.0) / 4.0;
const ANIM_SPEED: f32 = 4.0;
const AI_DELAY: f32 = 0.15;
const POP_DUR: f32 = 0.28;
const WIN_PAUSE: f32 = 2.5;
const OVER_PAUSE: f32 = 2.5;

// --- Colors ---

fn rgb(r: u8, g: u8, b: u8) -> Color {
    rgba(r, g, b, 255)
}
fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::new(
        r as f32 / 255.,
        g as f32 / 255.,
        b as f32 / 255.,
        a as f32 / 255.,
    )
}

fn tile_bg(val: u32) -> Color {
    match val {
        0 => rgb(205, 193, 180),
        2 => rgb(238, 228, 218),
        4 => rgb(237, 224, 200),
        8 => rgb(242, 177, 121),
        16 => rgb(245, 149, 99),
        32 => rgb(246, 124, 95),
        64 => rgb(246, 94, 59),
        128 => rgb(237, 207, 114),
        256 => rgb(237, 204, 97),
        512 => rgb(237, 200, 80),
        1024 => rgb(237, 197, 63),
        2048 => rgb(237, 194, 46),
        _ => rgb(60, 58, 50),
    }
}

fn tile_fg(val: u32) -> Color {
    if val <= 4 {
        rgb(119, 110, 101)
    } else {
        rgb(249, 246, 242)
    }
}

// --- Animation helpers ---

// Convert (row, col) from prep-space back to original board coordinates.
fn inv_coord(r: f32, c: f32, dir: u8) -> (f32, f32) {
    let n = N as f32 - 1.0;
    match dir {
        0 => (r, c),
        1 => (r, n - c),
        2 => (c, r),
        _ => (n - c, r),
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

struct AnimTile {
    fr: f32,
    fc: f32,
    tr: f32,
    tc: f32,
    val: u32,
    merge: bool,
}

struct Pop {
    row: usize,
    col: usize,
    t: f32,
    spawn: bool,
}

fn slide_row_anim(row: [u32; N], grid_row: usize) -> (Vec<AnimTile>, [u32; N], u32) {
    let src: Vec<(usize, u32)> = (0..N)
        .filter(|&c| row[c] != 0)
        .map(|c| (c, row[c]))
        .collect();
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
            tiles.push(AnimTile {
                fr: rf,
                fc: src[i].0 as f32,
                tr: rf,
                tc: dst as f32,
                val: src[i].1,
                merge: false,
            });
            tiles.push(AnimTile {
                fr: rf,
                fc: src[i + 1].0 as f32,
                tr: rf,
                tc: dst as f32,
                val: src[i + 1].1,
                merge: true,
            });
            i += 2;
        } else {
            new_row[dst] = src[i].1;
            tiles.push(AnimTile {
                fr: rf,
                fc: src[i].0 as f32,
                tr: rf,
                tc: dst as f32,
                val: src[i].1,
                merge: false,
            });
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

// --- Game ---

#[derive(PartialEq)]
enum Phase {
    Playing,
    WinPause,
    GameOver,
}

struct Game {
    board: [[u32; N]; N],
    next_board: [[u32; N]; N],
    score: u32,
    best: u32,
    pending: u32,
    won: bool,
    phase: Phase,
    overlay_timer: f32,
    anim_tiles: Vec<AnimTile>,
    anim_t: f32,
    pops: Vec<Pop>,
    ai_wait: f32,
    last_dir: Option<u8>,
}

impl Game {
    fn new(best: u32) -> Self {
        let mut g = Self {
            board: [[0; N]; N],
            next_board: [[0; N]; N],
            score: 0,
            best,
            pending: 0,
            won: false,
            phase: Phase::Playing,
            overlay_timer: 0.0,
            anim_tiles: Vec::new(),
            anim_t: 1.0,
            pops: Vec::new(),
            ai_wait: 0.6,
            last_dir: None,
        };
        g.spawn();
        g.spawn();
        g
    }

    fn spawn(&mut self) {
        let e: Vec<(usize, usize)> = (0..N)
            .flat_map(|r| (0..N).map(move |c| (r, c)))
            .filter(|&(r, c)| self.board[r][c] == 0)
            .collect();
        if e.is_empty() {
            return;
        }
        let (r, c) = e[rand::gen_range(0, e.len())];
        self.board[r][c] = if rand::gen_range(0u32, 10) == 0 { 4 } else { 2 };
        self.pops.push(Pop {
            row: r,
            col: c,
            t: 0.0,
            spawn: true,
        });
    }

    fn start_move(&mut self, dir: u8) -> bool {
        let p = prep(self.board, dir);
        let (mut tiles, new_p, pts) = slide_left_anim(p);
        if new_p == p {
            return false;
        }
        for t in &mut tiles {
            let (fr, fc) = inv_coord(t.fr, t.fc, dir);
            let (tr, tc) = inv_coord(t.tr, t.tc, dir);
            t.fr = fr;
            t.fc = fc;
            t.tr = tr;
            t.tc = tc;
        }
        self.next_board = unprep(new_p, dir);
        self.pending = pts;
        self.anim_tiles = tiles;
        self.anim_t = 0.0;
        self.last_dir = Some(dir);
        true
    }

    fn commit(&mut self) {
        let mut merge_dests = [[false; N]; N];
        for t in &self.anim_tiles {
            if t.merge {
                merge_dests[t.tr as usize][t.tc as usize] = true;
            }
        }
        self.score += self.pending;
        self.best = self.best.max(self.score);
        self.board = self.next_board;
        self.anim_t = 1.0;
        self.anim_tiles.clear();
        for (r, row) in merge_dests.iter().enumerate() {
            for (c, &dest) in row.iter().enumerate() {
                if dest {
                    self.pops.push(Pop {
                        row: r,
                        col: c,
                        t: 0.0,
                        spawn: false,
                    });
                }
            }
        }
        self.spawn();
        if !self.won && self.board.iter().any(|row| row.iter().any(|&v| v >= 2048)) {
            self.won = true;
            self.phase = Phase::WinPause;
            self.overlay_timer = WIN_PAUSE;
        }
        if !can_move(&self.board) {
            self.phase = Phase::GameOver;
            self.overlay_timer = OVER_PAUSE;
        }
    }

    fn update(&mut self, dt: f32) -> bool {
        for p in &mut self.pops {
            p.t += dt / POP_DUR;
        }
        self.pops.retain(|p| p.t < 1.0);

        if self.phase == Phase::WinPause {
            self.overlay_timer -= dt;
            if self.overlay_timer <= 0.0 {
                self.phase = Phase::Playing;
                self.ai_wait = 0.3;
            }
            return false;
        }
        if self.phase == Phase::GameOver {
            self.overlay_timer -= dt;
            if self.overlay_timer <= 0.0 {
                return true;
            }
            return false;
        }
        if self.anim_t < 1.0 {
            self.anim_t = (self.anim_t + dt * ANIM_SPEED).min(1.0);
            if self.anim_t >= 1.0 {
                self.commit();
                self.ai_wait = AI_DELAY;
            }
        } else if self.ai_wait > 0.0 {
            self.ai_wait -= dt;
        } else if let Some(dir) = choose_dir(&self.board) {
            self.start_move(dir);
        } else {
            self.phase = Phase::GameOver;
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
    let bg = Color {
        a: bg.a * alpha,
        ..bg
    };
    let s = CELL * scale;
    let ox = cx + (CELL - s) / 2.0;
    let oy = cy + (CELL - s) / 2.0;
    rrect(ox, oy, s, s, (4.0 * scale).max(1.0), bg);
    if val > 0 {
        let fg = tile_fg(val);
        let fg = Color {
            a: fg.a * alpha,
            ..fg
        };
        let text = val.to_string();
        let base_fs: u16 = match text.len() {
            1 | 2 => 52,
            3 => 42,
            _ => 34,
        };
        let fs = (base_fs as f32 * scale).max(1.0) as u16;
        let d = measure_text(&text, None, fs, 1.0);
        draw_text(
            &text,
            ox + (s - d.width) / 2.0,
            oy + (s + d.height) / 2.0,
            fs as f32,
            fg,
        );
    }
}

fn grid_xy(row: f32, col: f32) -> (f32, f32) {
    (
        GRID_X + GAP + col * (CELL + GAP),
        GRID_Y + GAP + row * (CELL + GAP),
    )
}

fn score_box(x: f32, y: f32, w: f32, h: f32, label: &str, val: u32) {
    rrect(x, y, w, h, 4.0, rgb(187, 173, 160));
    let ld = measure_text(label, None, 13, 1.0);
    draw_text(
        label,
        x + (w - ld.width) / 2.0,
        y + 18.0,
        13.0,
        rgb(238, 228, 218),
    );
    let s = val.to_string();
    let fs: u16 = if val >= 10000 {
        18
    } else if val >= 1000 {
        22
    } else {
        26
    };
    let vd = measure_text(&s, None, fs, 1.0);
    draw_text(&s, x + (w - vd.width) / 2.0, y + h - 10.0, fs as f32, WHITE);
}

fn draw_equilateral(cx: f32, cy: f32, r: f32, dir: u8, color: Color) {
    let s = r * 0.866;
    let (v1, v2, v3) = match dir {
        0 => (
            vec2(cx - r, cy),
            vec2(cx + r * 0.5, cy - s),
            vec2(cx + r * 0.5, cy + s),
        ),
        1 => (
            vec2(cx + r, cy),
            vec2(cx - r * 0.5, cy - s),
            vec2(cx - r * 0.5, cy + s),
        ),
        2 => (
            vec2(cx, cy - r),
            vec2(cx - s, cy + r * 0.5),
            vec2(cx + s, cy + r * 0.5),
        ),
        _ => (
            vec2(cx, cy + r),
            vec2(cx - s, cy - r * 0.5),
            vec2(cx + s, cy - r * 0.5),
        ),
    };
    draw_triangle(v1, v2, v3, color);
}

fn window_conf() -> Conf {
    Conf {
        window_title: "2048 AI".to_string(),
        window_width: WIN_W as i32,
        window_height: WIN_H as i32,
        window_resizable: false,
        high_dpi: true,
        ..Default::default()
    }
}

// ── CLI args (native only — meaningless in a browser tab) ───────────────────────

struct CliArgs {
    /// `--debug`: print each move (once its slide animation starts) plus episode
    /// results to stderr.
    debug: bool,
    /// `--once`: play one episode to game-over, print a result line, then exit
    /// instead of looping into a new episode forever.
    once: bool,
    /// `--no-ui`: run with no window, no GL context, and no miniquad involvement at
    /// all (see `run_headless`).
    #[cfg(not(target_arch = "wasm32"))]
    no_ui: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_cli_args() -> CliArgs {
    let mut debug = false;
    let mut once = false;
    let mut no_ui = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--debug" => debug = true,
            "--once" => once = true,
            "--no-ui" => no_ui = true,
            other => {
                eprintln!("unknown argument '{other}' (expected --debug, --once, --no-ui)");
                std::process::exit(2);
            }
        }
    }

    CliArgs { debug, once, no_ui }
}

#[cfg(target_arch = "wasm32")]
fn parse_cli_args() -> CliArgs {
    CliArgs {
        debug: false,
        once: false,
    }
}

fn max_tile(board: &[[u32; N]; N]) -> u32 {
    board
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .max()
        .unwrap_or(0)
}

/// `Game::update` is driven by a real `dt` (slide-animation speed, AI think-delay,
/// win/game-over pause timers), not a discrete move list — so headless mode drives a
/// fixed virtual step forward each iteration instead of reading `get_frame_time()`,
/// running flat-out instead of pacing to real time.
#[cfg(not(target_arch = "wasm32"))]
const HEADLESS_DT: f32 = 0.05;

/// Runs the game with no window, no GL context, and no miniquad involvement at all —
/// `Game`'s board logic (`game2048::{prep, unprep, can_move, choose_dir}`) has no
/// rendering dependency (`macroquad::rand` is a pure `no_std` PRNG, safe to call
/// standalone), and miniquad has no headless backend to opt into, so the only way to
/// guarantee zero window creation is to never call
/// `miniquad::start`/`Window::from_config` in the first place.
#[cfg(not(target_arch = "wasm32"))]
fn run_headless(cli: CliArgs) -> ! {
    rand::srand(screenshot::seed());
    let mut game = Game::new(0);
    let mut prev_anim_t = 1.0f32;

    loop {
        if game.update(HEADLESS_DT) {
            if cli.debug {
                eprintln!(
                    "game_over score={} best={} max_tile={}",
                    game.score,
                    game.best,
                    max_tile(&game.board)
                );
            }
            if cli.once {
                println!(
                    "result=game_over score={} best={} max_tile={}",
                    game.score,
                    game.best,
                    max_tile(&game.board)
                );
                std::process::exit(0);
            }
            let best = game.best;
            game = Game::new(best);
            prev_anim_t = 1.0;
            continue;
        }

        if cli.debug && game.anim_t == 0.0 && prev_anim_t != 0.0 {
            eprintln!(
                "move dir={:?} score={} max_tile={}",
                game.last_dir,
                game.score,
                max_tile(&game.board)
            );
        }
        prev_anim_t = game.anim_t;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let cli = parse_cli_args();
    if cli.no_ui {
        run_headless(cli);
    } else {
        macroquad::Window::from_config(window_conf(), amain(cli));
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    macroquad::Window::from_config(window_conf(), amain(parse_cli_args()));
}

async fn amain(cli: CliArgs) {
    rand::srand(screenshot::seed());
    let mut game = Game::new(0);
    let title_w = measure_text("2048", None, 72, 1.0).width;
    let mut shot = screenshot::Capture::from_env();
    let mut control = control::Control::new();
    let mut prev_anim_t = 1.0f32;

    loop {
        control.handle_keys();
        let dt = control.scale(get_frame_time());
        let over = game.update(dt);

        if cli.debug && game.anim_t == 0.0 && prev_anim_t != 0.0 {
            eprintln!(
                "move dir={:?} score={} max_tile={}",
                game.last_dir,
                game.score,
                max_tile(&game.board)
            );
        }
        prev_anim_t = game.anim_t;

        if over {
            if cli.debug {
                eprintln!(
                    "game_over score={} best={} max_tile={}",
                    game.score,
                    game.best,
                    max_tile(&game.board)
                );
            }
            control.episode_complete("game2048", game.score as i64);
            if cli.once {
                println!(
                    "result=game_over score={} best={} max_tile={}",
                    game.score,
                    game.best,
                    max_tile(&game.board)
                );
                std::process::exit(0);
            }
            let best = game.best;
            game = Game::new(best);
            prev_anim_t = 1.0;
        }

        clear_background(rgb(250, 248, 239));

        draw_text("2048", 15.0, 78.0, 72.0, rgb(119, 110, 101));
        if let Some(dir) = game.last_dir {
            draw_equilateral(15.0 + title_w + 28.0, 54.0, 20.0, dir, rgb(143, 122, 102));
        }

        score_box(260.0, 8.0, 90.0, 60.0, "SCORE", game.score);
        score_box(360.0, 8.0, 90.0, 60.0, "BEST", game.best);

        let speed_label = control.label();
        let sd = measure_text(&speed_label, None, 13, 1.0);
        draw_text(
            &speed_label,
            WIN_W - 8.0 - sd.width,
            26.0,
            13.0,
            rgb(150, 140, 130),
        );

        rrect(GRID_X, GRID_Y, GRID_W, GRID_W, 6.0, rgb(187, 173, 160));

        let animating = game.anim_t < 1.0;
        if animating {
            let mut from_grid = [[false; N]; N];
            for t in &game.anim_tiles {
                from_grid[t.fr.round() as usize][t.fc.round() as usize] = true;
            }
            for (r, row) in from_grid.iter().enumerate() {
                for (c, &from) in row.iter().enumerate() {
                    let (tx, ty) = grid_xy(r as f32, c as f32);
                    if from {
                        tile_cell(tx, ty, 0, 1.0, 1.0);
                    } else {
                        tile_cell(tx, ty, game.board[r][c], 1.0, 1.0);
                    }
                }
            }
            let t = smoothstep(game.anim_t);
            for tile in &game.anim_tiles {
                let r = tile.fr + (tile.tr - tile.fr) * t;
                let c = tile.fc + (tile.tc - tile.fc) * t;
                let (tx, ty) = grid_xy(r, c);
                let alpha = if tile.merge {
                    (1.0 - t * t * t).max(0.0)
                } else {
                    1.0
                };
                tile_cell(tx, ty, tile.val, 1.0, alpha);
            }
        } else {
            for r in 0..N {
                for c in 0..N {
                    let (tx, ty) = grid_xy(r as f32, c as f32);
                    let val = game.board[r][c];
                    let scale = game
                        .pops
                        .iter()
                        .find(|p| p.row == r && p.col == c)
                        .map(|p| {
                            let t = p.t.clamp(0., 1.);
                            if p.spawn {
                                (t * std::f32::consts::PI / 2.0).sin()
                            } else {
                                1.0 + 0.2 * (t * std::f32::consts::PI).sin()
                            }
                        })
                        .unwrap_or(1.0);
                    tile_cell(tx, ty, val, scale, 1.0);
                }
            }
        }

        if game.phase == Phase::WinPause {
            draw_rectangle(GRID_X, GRID_Y, GRID_W, GRID_W, rgba(255, 243, 108, 185));
            let txt = "You win!";
            let d = measure_text(txt, None, 64, 1.0);
            let cx = GRID_X + GRID_W / 2.0;
            let cy = GRID_Y + GRID_W / 2.0;
            draw_text(txt, cx - d.width / 2.0, cy - 30.0, 64.0, rgb(119, 110, 101));
            let sub = format!("Continuing in {:.0}...", game.overlay_timer.max(0.0));
            let sd = measure_text(&sub, None, 20, 1.0);
            draw_text(
                &sub,
                cx - sd.width / 2.0,
                cy + 15.0,
                20.0,
                rgb(119, 110, 101),
            );
        }

        if game.phase == Phase::GameOver {
            draw_rectangle(GRID_X, GRID_Y, GRID_W, GRID_W, rgba(199, 187, 177, 190));
            let txt = "Game over!";
            let d = measure_text(txt, None, 60, 1.0);
            let cx = GRID_X + GRID_W / 2.0;
            let cy = GRID_Y + GRID_W / 2.0;
            draw_text(txt, cx - d.width / 2.0, cy - 30.0, 60.0, rgb(119, 110, 101));
            let sc = format!("Score: {}", game.score);
            let scd = measure_text(&sc, None, 26, 1.0);
            draw_text(
                &sc,
                cx - scd.width / 2.0,
                cy + 10.0,
                26.0,
                rgb(119, 110, 101),
            );
            let sub = format!("Restarting in {:.0}...", game.overlay_timer.max(0.0));
            let sd = measure_text(&sub, None, 18, 1.0);
            draw_text(
                &sub,
                cx - sd.width / 2.0,
                cy + 40.0,
                18.0,
                rgb(119, 110, 101),
            );
        }

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}
