use std::collections::HashSet;
use macroquad::prelude::*;
use cards::card::Card;
use cards::render::{draw_card_back, draw_card_face, draw_empty_slot, draw_suit_symbol};

mod game;
mod solver;

use game::{Game, Move, Phase};
use solver::Solver;

const TICK: f32 = 0.20;
const ANIM_DURATION: f32 = 0.14;
const RESTART_DELAY: f64 = 2.8;

// ── Layout ────────────────────────────────────────────────────────────────────

struct Layout {
    cw: f32,
    ch: f32,
    gap: f32,
    ox: f32,
    top_y: f32,
    tab_y: f32,
    sh: f32,
}

impl Layout {
    fn from_screen() -> Self {
        let sw = screen_width();
        let sh = screen_height();
        let avail_w = sw - 20.0;
        let cw = ((avail_w - 6.0 * 12.0) / 7.0).min(105.0).max(36.0);
        let ch = cw * 1.40;
        let gap = ((avail_w - 7.0 * cw) / 6.0).max(4.0);
        let top_y = 42.0;
        let tab_y = top_y + ch + 16.0;
        Self { cw, ch, gap, ox: 10.0, top_y, tab_y, sh }
    }

    fn col_x(&self, i: usize) -> f32 {
        self.ox + i as f32 * (self.cw + self.gap)
    }

    fn waste_top_pos(&self, game: &Game) -> (f32, f32) {
        let wx = self.col_x(1);
        if game.draw_count == 3 {
            let n = game.waste.len().min(3);
            if n > 1 {
                let fan = (self.cw * 0.26).min(24.0);
                return (wx + (n - 1) as f32 * fan, self.top_y);
            }
        }
        (wx, self.top_y)
    }

    fn foundation_pos(&self, suit: usize) -> (f32, f32) {
        (self.col_x(suit + 3), self.top_y)
    }

    fn tableau_offsets(&self, game: &Game, pile: usize) -> (f32, f32) {
        let nd = game.n_down[pile];
        let n_up = game.tableau[pile].len().saturating_sub(nd);
        let avail_h = self.sh - self.tab_y - 8.0;
        let down_off = (self.ch * 0.155).max(10.0);
        let up_off_ideal = (self.ch * 0.265).max(14.0);
        let up_off = if n_up > 1 {
            let needed = nd as f32 * down_off + (n_up as f32 - 1.0) * up_off_ideal + self.ch;
            if needed > avail_h {
                ((avail_h - self.ch - nd as f32 * down_off) / (n_up as f32 - 1.0)).max(8.0)
            } else {
                up_off_ideal
            }
        } else {
            up_off_ideal
        };
        (down_off, up_off)
    }

    fn tableau_card_pos(&self, game: &Game, pile: usize, card_idx: usize) -> (f32, f32) {
        let tx = self.col_x(pile);
        let nd = game.n_down[pile];
        let (down_off, up_off) = self.tableau_offsets(game, pile);
        let mut cy = self.tab_y;
        for i in 0..card_idx {
            cy += if i < nd { down_off } else { up_off };
        }
        (tx, cy)
    }

    fn tableau_top_pos(&self, game: &Game, pile: usize) -> (f32, f32) {
        let len = game.tableau[pile].len();
        if len == 0 {
            (self.col_x(pile), self.tab_y)
        } else {
            self.tableau_card_pos(game, pile, len - 1)
        }
    }
}

// ── Animation ─────────────────────────────────────────────────────────────────

struct FlyingCard {
    card: Card,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

fn compute_flying_cards(game: &Game, m: Move, layout: &Layout) -> Vec<FlyingCard> {
    let mut after = game.clone();
    after.apply(m);

    match m {
        Move::DrawStock | Move::ResetStock => vec![],

        Move::WasteToFoundation => {
            let card = *game.waste.last().unwrap();
            let (x0, y0) = layout.waste_top_pos(game);
            let (x1, y1) = layout.foundation_pos(card.suit as usize);
            vec![FlyingCard { card, x0, y0, x1, y1 }]
        }
        Move::WasteToTableau(to) => {
            let card = *game.waste.last().unwrap();
            let (x0, y0) = layout.waste_top_pos(game);
            let (x1, y1) = layout.tableau_top_pos(&after, to);
            vec![FlyingCard { card, x0, y0, x1, y1 }]
        }
        Move::TableauToFoundation(from) => {
            let card = *game.tableau[from].last().unwrap();
            let (x0, y0) = layout.tableau_top_pos(game, from);
            let (x1, y1) = layout.foundation_pos(card.suit as usize);
            vec![FlyingCard { card, x0, y0, x1, y1 }]
        }
        Move::TableauToTableau { from, n, to } => {
            let t = &game.tableau[from];
            (0..n)
                .map(|i| {
                    let card_idx = t.len() - n + i;
                    let card = t[card_idx];
                    let (x0, y0) = layout.tableau_card_pos(game, from, card_idx);
                    let (x1, y1) =
                        layout.tableau_card_pos(&after, to, after.tableau[to].len() - n + i);
                    FlyingCard { card, x0, y0, x1, y1 }
                })
                .collect()
        }
        Move::FoundationToTableau { suit, to } => {
            let card = *game.foundations[suit].last().unwrap();
            let (x0, y0) = layout.foundation_pos(suit);
            let (x1, y1) = layout.tableau_top_pos(&after, to);
            vec![FlyingCard { card, x0, y0, x1, y1 }]
        }
    }
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0_f32).powi(3) / 2.0
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn conf() -> Conf {
    Conf {
        window_title: "Klondike".to_owned(),
        window_width: 900,
        window_height: 720,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    macroquad::rand::srand(macroquad::miniquad::date::now() as u64);

    let mut game = Game::new(0);
    let mut display_game = game.clone();
    let mut solver = Solver::new();
    let mut accum = 0.0f32;
    let mut anim_t: f32 = 1.0; // start settled so first move fires immediately
    let mut flying: Vec<FlyingCard> = Vec::new();
    let mut end_time: Option<f64> = None;

    loop {
        let now = macroquad::miniquad::date::now();
        let dt = get_frame_time().min(0.1);
        let layout = Layout::from_screen();

        // Advance animation; when it settles, sync display_game to actual game.
        anim_t = (anim_t + dt / ANIM_DURATION).min(1.0);
        if anim_t >= 1.0 {
            display_game = game.clone();
            flying.clear();
        }

        match game.phase {
            Phase::Playing => {
                accum += dt;
                if anim_t >= 1.0 && accum >= TICK {
                    accum -= TICK;
                    if let Some(m) = solver.choose_move(&game) {
                        flying = compute_flying_cards(&game, m, &layout);
                        game.apply(m);
                        if flying.is_empty() {
                            // No animation needed (draw/reset); snap display immediately.
                            display_game = game.clone();
                        } else {
                            anim_t = 0.0;
                            // display_game already holds the pre-move snapshot from above.
                        }
                    }
                }
            }
            Phase::Won | Phase::Stuck => {
                let t = *end_time.get_or_insert(now);
                if now - t > RESTART_DELAY {
                    game = Game::new(game.generation + 1);
                    display_game = game.clone();
                    solver = Solver::new();
                    end_time = None;
                    accum = 0.0;
                    anim_t = 1.0;
                    flying.clear();
                }
            }
        }

        let in_flight: HashSet<Card> = flying.iter().map(|fc| fc.card).collect();

        clear_background(Color::new(0.10, 0.28, 0.10, 1.0));
        draw_hud(&game);
        draw_game(&display_game, &layout, &in_flight);

        // Overlay flying cards at their interpolated position.
        let t = ease_in_out(anim_t);
        for fc in &flying {
            let x = fc.x0 + (fc.x1 - fc.x0) * t;
            let y = fc.y0 + (fc.y1 - fc.y0) * t;
            draw_card_face(x, y, layout.cw, layout.ch, fc.card);
        }

        next_frame().await;
    }
}

// ── HUD ───────────────────────────────────────────────────────────────────────

fn draw_hud(game: &Game) {
    let sw = screen_width();
    let (hud_bg, txt_col) = match game.phase {
        Phase::Won => (
            Color::new(0.05, 0.18, 0.08, 1.0),
            Color::new(0.28, 1.0, 0.52, 1.0),
        ),
        Phase::Stuck => (
            Color::new(0.18, 0.05, 0.05, 1.0),
            Color::new(1.0, 0.38, 0.38, 1.0),
        ),
        Phase::Playing => (
            Color::new(0.07, 0.07, 0.12, 1.0),
            Color::new(0.68, 0.68, 0.85, 1.0),
        ),
    };
    draw_rectangle(0.0, 0.0, sw, 34.0, hud_bg);

    let status = match game.phase {
        Phase::Playing => String::new(),
        Phase::Won => "  - WON! Restarting...".to_owned(),
        Phase::Stuck => "  - STUCK. Restarting...".to_owned(),
    };
    let msg = format!(
        "Klondike  Draw-{}   Moves: {}   Gen: {}{}",
        game.draw_count,
        game.moves,
        game.generation + 1,
        status,
    );
    draw_text(&msg, 10.0, 24.0, 20.0, txt_col);
}

// ── Game board ────────────────────────────────────────────────────────────────

fn draw_game(game: &Game, layout: &Layout, in_flight: &HashSet<Card>) {
    let Layout { cw, ch, top_y, tab_y, sh: _, .. } = *layout;

    // ── Stock ─────────────────────────────────────────────────────────────────
    let sx = layout.col_x(0);
    if game.stock.is_empty() {
        draw_empty_slot(sx, top_y, cw, ch);
    } else {
        draw_card_back(sx, top_y, cw, ch);
        let cnt = game.stock.len().to_string();
        let d = measure_text(&cnt, None, 16, 1.0);
        draw_text(
            &cnt,
            sx + cw * 0.5 - d.width * 0.5,
            top_y + ch + 14.0,
            16.0,
            Color::new(0.55, 0.65, 0.55, 0.75),
        );
    }

    // ── Waste ─────────────────────────────────────────────────────────────────
    let wx = layout.col_x(1);
    if game.waste.is_empty() {
        draw_empty_slot(wx, top_y, cw, ch);
    } else if game.draw_count == 3 {
        let start = game.waste.len().saturating_sub(3);
        let fan = (cw * 0.26).min(24.0);
        for (i, &card) in game.waste[start..].iter().enumerate() {
            if !in_flight.contains(&card) {
                draw_card_face(wx + i as f32 * fan, top_y, cw, ch, card);
            }
        }
    } else {
        let card = *game.waste.last().unwrap();
        if !in_flight.contains(&card) {
            draw_card_face(wx, top_y, cw, ch, card);
        }
    }

    // ── Foundations ───────────────────────────────────────────────────────────
    for s in 0..4usize {
        let fx = layout.col_x(s + 3);
        let f = &game.foundations[s];
        let gcol = if s == 1 || s == 2 {
            Color::new(0.55, 0.20, 0.20, 0.50)
        } else {
            Color::new(0.32, 0.36, 0.52, 0.50)
        };
        if f.is_empty() {
            draw_empty_slot(fx, top_y, cw, ch);
            draw_suit_symbol(fx + cw * 0.5, top_y + ch * 0.52, (ch * 0.32).max(12.0), s as u8, gcol);
        } else {
            let top = *f.last().unwrap();
            if !in_flight.contains(&top) {
                draw_card_face(fx, top_y, cw, ch, top);
            } else if f.len() >= 2 {
                draw_card_face(fx, top_y, cw, ch, f[f.len() - 2]);
            } else {
                draw_empty_slot(fx, top_y, cw, ch);
                draw_suit_symbol(fx + cw * 0.5, top_y + ch * 0.52, (ch * 0.32).max(12.0), s as u8, gcol);
            }
        }
    }

    // ── Tableau ───────────────────────────────────────────────────────────────
    for p in 0..7usize {
        let tx = layout.col_x(p);
        let t = &game.tableau[p];
        let nd = game.n_down[p];

        if t.is_empty() {
            draw_empty_slot(tx, tab_y, cw, ch);
            continue;
        }

        let (down_off, up_off) = layout.tableau_offsets(game, p);
        let mut cy = tab_y;
        for (i, &card) in t.iter().enumerate() {
            let is_last = i == t.len() - 1;
            if i < nd {
                draw_card_back(tx, cy, cw, ch);
                cy += down_off;
            } else {
                if !in_flight.contains(&card) {
                    draw_card_face(tx, cy, cw, ch, card);
                }
                if !is_last {
                    cy += up_off;
                }
            }
        }
    }
}
