use macroquad::prelude::*;
use minesweeper::board::GridKind;

fn conf() -> Conf {
    Conf {
        window_title: "Minesweeper — Hex".to_owned(),
        window_width: 900,
        window_height: 720,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    macroquad::rand::srand(macroquad::miniquad::date::now() as u64);
    minesweeper::run(GridKind::Hex).await;
}
