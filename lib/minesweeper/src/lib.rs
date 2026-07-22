pub mod board;
pub mod solver;

use macroquad::prelude::*;
use board::{Board, Cell, CellState, GridKind, Phase};
use solver::{Action, next_action, update_probs};

const TICK: f32 = 0.18;
const RESTART_DELAY: f64 = 3.5;

pub async fn run(kind: GridKind) {
    rand::srand(screenshot::seed());
    let mut board = Board::new(kind);
    let mut accum = 0.0f32;
    let mut shot = screenshot::Capture::from_env();
    let mut control = control::Control::new();
    let game_name = match kind {
        GridKind::Square => "minesweeper-square",
        GridKind::Hex => "minesweeper-hex",
    };

    loop {
        control.handle_keys();
        accum += control.scale(get_frame_time().min(0.1));

        while accum >= TICK {
            accum -= TICK;
            match board.phase {
                Phase::FirstClick => {
                    let center = (board.rows / 2 * board.cols + board.cols / 2) as usize;
                    board.place_mines(center);
                    board.phase = Phase::Playing;
                    board.reveal(center);
                    board.last_action = Some(center);
                    board.last_was_flag = false;
                    update_probs(&mut board);
                }
                Phase::Playing => {
                    let action = next_action(&board);
                    match action {
                        Action::Flag(idx) => {
                            board.flag(idx);
                            board.last_action = Some(idx);
                            board.last_was_flag = true;
                        }
                        Action::Open(idx) | Action::Guess(idx) => {
                            board.last_action = Some(idx);
                            board.last_was_flag = false;
                            board.reveal(idx);
                        }
                    }
                    if matches!(board.phase, Phase::Playing) {
                        update_probs(&mut board);
                    }
                }
                Phase::GameOver(t) | Phase::Won(t) => {
                    if macroquad::miniquad::date::now() - t > RESTART_DELAY {
                        let revealed =
                            board.cells.iter().filter(|c| c.state == CellState::Revealed).count();
                        control.episode_complete(game_name, revealed as i64);
                        board = Board::new(kind);
                        update_probs(&mut board);
                    }
                    break;
                }
            }
        }

        draw_board(&board, &control.label());
        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}

// ── Color helpers ─────────────────────────────────────────────────────────────

// greenish-gray (0%) → neutral gray (pivot) → orangish-gray (100%)
fn prob_color(p: f32, pivot: f32) -> Color {
    let p = p.clamp(0.0, 1.0);
    let pivot = pivot.clamp(0.01, 0.99);
    const GREEN:   (f32, f32, f32) = (0.37, 0.45, 0.39);
    const NEUTRAL: (f32, f32, f32) = (0.42, 0.42, 0.42);
    const ORANGE:  (f32, f32, f32) = (0.52, 0.44, 0.34);
    let (t, lo, hi) = if p <= pivot {
        (p / pivot, GREEN, NEUTRAL)
    } else {
        ((p - pivot) / (1.0 - pivot), NEUTRAL, ORANGE)
    };
    Color { r: lo.0 + t * (hi.0 - lo.0), g: lo.1 + t * (hi.1 - lo.1), b: lo.2 + t * (hi.2 - lo.2), a: 1.0 }
}

fn adj_color(n: u8) -> Color {
    match n {
        1 => Color::new(0.33, 0.53, 1.00, 1.0),
        2 => Color::new(0.20, 0.76, 0.30, 1.0),
        3 => Color::new(1.00, 0.27, 0.27, 1.0),
        4 => Color::new(0.13, 0.27, 0.60, 1.0),
        5 => Color::new(0.60, 0.13, 0.13, 1.0),
        6 => Color::new(0.13, 0.65, 0.65, 1.0),
        7 => Color::new(0.65, 0.20, 0.75, 1.0),
        8 => Color::new(0.70, 0.70, 0.70, 1.0),
        _ => WHITE,
    }
}

// global_prob = mines_remaining/hidden_remaining, None when equal (all remaining are mines)
fn cell_bg(cell: &Cell, idx: usize, hit: Option<usize>, global_prob: Option<f32>) -> Color {
    let pivot = global_prob.unwrap_or(0.5);
    match cell.state {
        CellState::Revealed => {
            if cell.is_mine {
                if hit == Some(idx) {
                    Color { r: 1.0, g: 0.08, b: 0.08, a: 1.0 }
                } else {
                    Color { r: 0.30, g: 0.30, b: 0.34, a: 1.0 }
                }
            } else {
                Color { r: 0.20, g: 0.22, b: 0.28, a: 1.0 }
            }
        }
        CellState::Hidden => prob_color(cell.mine_prob, pivot),
        CellState::Flagged => match global_prob {
            Some(p) => prob_color(p, pivot),
            None    => Color { r: 0.42, g: 0.42, b: 0.42, a: 1.0 },
        },
    }
}

// ── HUD ──────────────────────────────────────────────────────────────────────

fn draw_hud(board: &Board, sw: f32, speed_label: &str) {
    let hud_bg = match board.phase {
        Phase::GameOver(_) => Color { r: 0.28, g: 0.06, b: 0.06, a: 1.0 },
        Phase::Won(_) => Color { r: 0.06, g: 0.20, b: 0.10, a: 1.0 },
        _ => Color { r: 0.09, g: 0.09, b: 0.16, a: 1.0 },
    };
    let text_col = match board.phase {
        Phase::GameOver(_) => Color { r: 1.0, g: 0.35, b: 0.35, a: 1.0 },
        Phase::Won(_) => Color { r: 0.30, g: 1.0, b: 0.55, a: 1.0 },
        _ => Color { r: 0.68, g: 0.68, b: 0.85, a: 1.0 },
    };
    draw_rectangle(0.0, 0.0, sw, 34.0, hud_bg);
    let flagged = board.cells.iter().filter(|c| c.state == CellState::Flagged).count();
    let label = match board.kind {
        GridKind::Square => "Square",
        GridKind::Hex => "Hex",
    };
    let msg = match board.phase {
        Phase::FirstClick | Phase::Playing =>
            format!("{}  Mines: {}  Flagged: {}  Remaining: {}", label, board.mine_count, flagged, board.remaining_mines()),
        Phase::GameOver(_) => format!("{}  MINE HIT — restarting in {:.0}s…", label, RESTART_DELAY as u32),
        Phase::Won(_) => format!("{}  SOLVED! Restarting in {:.0}s…", label, RESTART_DELAY as u32),
    };
    draw_text(&msg, 10.0, 24.0, 20.0, text_col);

    let sd = measure_text(speed_label, None, 20, 1.0);
    draw_text(speed_label, sw - 8.0 - sd.width, 24.0, 20.0, text_col);
}

// ── Flag glyph (small triangle) ───────────────────────────────────────────────

fn draw_flag(cx: f32, cy: f32, size: f32) {
    let pole_h = size * 0.52;
    let pole_top = cy - pole_h * 0.52;
    let pole_bot = cy + pole_h * 0.48;
    let pole_x = cx - size * 0.04;
    let flag_w = size * 0.28;
    let flag_h = size * 0.22;
    let dark = Color { r: 0.10, g: 0.10, b: 0.12, a: 1.0 };
    let red  = Color { r: 0.95, g: 0.15, b: 0.15, a: 1.0 };
    // pole
    draw_line(pole_x, pole_top, pole_x, pole_bot, (size * 0.055).max(1.5), dark);
    // triangular flag pointing right
    draw_triangle(
        vec2(pole_x, pole_top),
        vec2(pole_x, pole_top + flag_h),
        vec2(pole_x + flag_w, pole_top + flag_h * 0.5),
        red,
    );
    // base
    draw_line(pole_x - size * 0.12, pole_bot, pole_x + size * 0.12, pole_bot, (size * 0.055).max(1.5), dark);
}

// ── Mine glyph ───────────────────────────────────────────────────────────────

fn draw_mine(cx: f32, cy: f32, size: f32) {
    let r = size * 0.26;
    let col = Color { r: 0.06, g: 0.06, b: 0.07, a: 1.0 };
    let thick = (size * 0.055).max(1.5);
    // 8 spikes
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::PI * 0.25;
        let (sa, ca) = a.sin_cos();
        draw_line(cx + ca * r * 0.75, cy + sa * r * 0.75, cx + ca * r * 1.5, cy + sa * r * 1.5, thick, col);
    }
    draw_circle(cx, cy, r, col);
    // shine
    draw_circle(cx - r * 0.28, cy - r * 0.28, r * 0.22, Color { r: 0.85, g: 0.85, b: 0.88, a: 0.55 });
}

// ── Square grid ───────────────────────────────────────────────────────────────

fn draw_square(board: &Board, ox: f32, oy: f32, avail_w: f32, avail_h: f32, global_prob: Option<f32>) {
    let cell = (avail_w / board.cols as f32).min(avail_h / board.rows as f32).floor().max(4.0);
    let gw = board.cols as f32 * cell;
    let gh = board.rows as f32 * cell;
    let ox = ox + ((avail_w - gw) * 0.5).floor();
    let oy = oy + ((avail_h - gh) * 0.5).floor();

    for idx in 0..board.cells.len() {
        let c = (idx as i32 % board.cols) as f32;
        let r = (idx as i32 / board.cols) as f32;
        let px = ox + c * cell;
        let py = oy + r * cell;
        let cell_ref = &board.cells[idx];
        let pad = 1.5_f32.max(cell * 0.03);

        draw_rectangle(px + pad, py + pad, cell - 2.0 * pad, cell - 2.0 * pad, cell_bg(cell_ref, idx, board.hit_mine, global_prob));

        let cx = px + cell * 0.5;
        let cy = py + cell * 0.5;

        if cell_ref.state == CellState::Flagged {
            draw_flag(cx, cy, cell);
        }

        if cell_ref.state == CellState::Revealed && cell_ref.is_mine {
            draw_mine(cx, cy, cell);
        }

        if cell_ref.state == CellState::Revealed && !cell_ref.is_mine && cell_ref.adj_mines > 0 {
            let fs = (cell * 0.55).max(10.0);
            let txt = cell_ref.adj_mines.to_string();
            let d = measure_text(&txt, None, fs as u16, 1.0);
            draw_text(&txt, cx - d.width * 0.5, cy + d.height * 0.5, fs, adj_color(cell_ref.adj_mines));
        }

        if board.last_action == Some(idx) {
            draw_rectangle_lines(px + pad, py + pad, cell - 2.0 * pad, cell - 2.0 * pad, 2.5, YELLOW);
        }
    }
}

// ── Hex grid ─────────────────────────────────────────────────────────────────
// flat-top hexagons, odd columns shifted down by row_spacing/2

fn draw_hex_grid(board: &Board, ox: f32, oy: f32, avail_w: f32, avail_h: f32, global_prob: Option<f32>) {
    let cols = board.cols as f32;
    let rows = board.rows as f32;
    // hex_r = circumscribed radius; col_spacing = 1.5*r; row_spacing = sqrt(3)*r
    let r_from_w = avail_w / (1.5 * cols + 0.5);
    let r_from_h = avail_h / (1.732 * rows + 0.866);
    let hex_r = r_from_w.min(r_from_h).max(4.0);
    let col_sp = hex_r * 1.5;
    let row_sp = hex_r * 1.732;

    let total_w = col_sp * (board.cols as f32 - 1.0) + 2.0 * hex_r;
    let total_h = row_sp * (board.rows as f32 - 1.0) + hex_r + row_sp * 0.5;
    let ox = ox + ((avail_w - total_w) * 0.5).max(0.0).floor();
    let oy = oy + ((avail_h - total_h) * 0.5).max(0.0).floor();

    for idx in 0..board.cells.len() {
        let col = (idx as i32 % board.cols) as f32;
        let row = (idx as i32 / board.cols) as f32;
        let cx = ox + col * col_sp + hex_r;
        let cy = oy + row * row_sp + (if (idx as i32 % board.cols) % 2 == 1 { row_sp * 0.5 } else { 0.0 }) + hex_r;

        let cell_ref = &board.cells[idx];
        let r_inner = hex_r - 1.5;

        draw_poly(cx, cy, 6, r_inner, 0.0, cell_bg(cell_ref, idx, board.hit_mine, global_prob));

        if cell_ref.state == CellState::Flagged {
            draw_flag(cx, cy, hex_r * 2.0);
        }

        if cell_ref.state == CellState::Revealed && cell_ref.is_mine {
            draw_mine(cx, cy, hex_r * 2.0);
        }

        if cell_ref.state == CellState::Revealed && !cell_ref.is_mine && cell_ref.adj_mines > 0 {
            let fs = (hex_r * 0.85).max(10.0);
            let txt = cell_ref.adj_mines.to_string();
            let d = measure_text(&txt, None, fs as u16, 1.0);
            draw_text(&txt, cx - d.width * 0.5, cy + d.height * 0.5, fs, adj_color(cell_ref.adj_mines));
        }

        if board.last_action == Some(idx) {
            draw_poly_lines(cx, cy, 6, r_inner, 0.0, 2.5, YELLOW);
        }
    }
}

// ── Top-level draw ────────────────────────────────────────────────────────────

fn draw_board(board: &Board, speed_label: &str) {
    let sw = screen_width();
    let sh = screen_height();

    clear_background(Color { r: 0.07, g: 0.07, b: 0.12, a: 1.0 });
    draw_hud(board, sw, speed_label);

    let total_hidden = board.cells.iter().filter(|c| c.state == CellState::Hidden).count() as i32;
    let remaining = board.remaining_mines();
    // None when equal (all remaining hidden are mines) → flagged tiles use flat gray
    let global_prob: Option<f32> = if total_hidden > 0 && total_hidden != remaining {
        Some((remaining as f32 / total_hidden as f32).clamp(0.0, 1.0))
    } else {
        None
    };

    let pad = 10.0_f32;
    let avail_w = sw - 2.0 * pad;
    let avail_h = sh - 34.0 - pad;

    match board.kind {
        GridKind::Square => draw_square(board, pad, 34.0, avail_w, avail_h, global_prob),
        GridKind::Hex => draw_hex_grid(board, pad, 34.0, avail_w, avail_h, global_prob),
    }
}
