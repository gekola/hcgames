use macroquad::prelude::*;

mod blocks;
mod game;

use game::Game;

pub const COLS: i32 = 40;
pub const ROWS: i32 = 30;
pub const TICK: f32 = 0.08;
pub const GRID: usize = (COLS * ROWS) as usize;
pub const BLOCK_SENTINEL: u16 = u16::MAX - 1;
pub const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pt {
    pub x: i32,
    pub y: i32,
}

impl Pt {
    pub fn shifted(self, dx: i32, dy: i32) -> Self {
        Self { x: self.x + dx, y: self.y + dy }
    }
    pub fn in_bounds(self) -> bool {
        self.x >= 0 && self.x < COLS && self.y >= 0 && self.y < ROWS
    }
    pub fn idx(self) -> usize {
        self.y as usize * COLS as usize + self.x as usize
    }
}

fn conf() -> Conf {
    Conf {
        window_title: "Snake".to_owned(),
        window_width: 900,
        window_height: 720,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    rand::srand(screenshot::seed());
    let mut game = Game::new(1);
    let mut accum = 0.0f32;
    let mut shot = screenshot::Capture::from_env();

    loop {
        let n = game.body.len().max(1) as f32;
        let hunger = if game.score >= 10 {
            ((game.ticks_hungry as f32 - n) / n).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let tick_interval = TICK * (1.0 - 0.5 * hunger);

        accum += get_frame_time();
        while accum >= tick_interval {
            accum -= tick_interval;
            if !game.tick() {
                game = Game::new(game.generation + 1);
                break;
            }
        }

        clear_background(Color { r: 0.07, g: 0.07, b: 0.12, a: 1.0 });

        let sw = screen_width();
        let sh = screen_height();
        let cell = (sw / COLS as f32).min((sh - 40.0) / ROWS as f32).floor().max(1.0);
        let ow = COLS as f32 * cell;
        let oh = ROWS as f32 * cell;
        let ox = ((sw - ow) * 0.5).floor();
        let oy = ((sh - oh - 30.0) * 0.5 + 30.0).floor();

        draw_rectangle(
            ox - 1.0, oy - 1.0, ow + 2.0, oh + 2.0,
            Color { r: 0.15, g: 0.17, b: 0.25, a: 1.0 },
        );

        for i in 0..GRID {
            if game.blocks[i] {
                let x = (i % COLS as usize) as f32;
                let y = (i / COLS as usize) as f32;
                draw_rectangle(
                    ox + x * cell + 1.0,
                    oy + y * cell + 1.0,
                    cell - 2.0,
                    cell - 2.0,
                    Color { r: 0.3, g: 0.3, b: 0.35, a: 1.0 },
                );
            }
        }

        let f = game.food;
        let pad = (cell * 0.12).max(2.0);
        draw_rectangle(
            ox + f.x as f32 * cell + pad,
            oy + f.y as f32 * cell + pad,
            cell - 2.0 * pad,
            cell - 2.0 * pad,
            Color { r: 0.95, g: 0.25, b: 0.25, a: 1.0 },
        );

        for (i, &seg) in game.body.iter().enumerate() {
            let color = if i == 0 {
                // blue → white/gray as hunger rises
                Color {
                    r: 0.08 + 0.92 * hunger,
                    g: 0.6 + 0.4 * hunger,
                    b: 0.95 + 0.05 * hunger,
                    a: 1.0,
                }
            } else {
                let t = 1.0 - (i as f32 / n) * 0.65;
                Color { r: 0.08, g: 0.6 * t, b: 0.95 * t, a: 1.0 }
            };
            draw_rectangle(
                ox + seg.x as f32 * cell + 1.0,
                oy + seg.y as f32 * cell + 1.0,
                cell - 2.0,
                cell - 2.0,
                color,
            );
        }

        let font_size = (cell * 0.9).max(14.0);
        let hud = format!("Score: {:>4}   Gen: {}", game.score, game.generation);
        draw_text(&hud, ox, oy - font_size * 0.35, font_size, Color { r: 0.65, g: 0.65, b: 0.82, a: 1.0 });

        shot.tick();
        next_frame().await;
    }
}
