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

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let cli = minesweeper::parse_cli_args();
    if cli.no_ui {
        minesweeper::run_headless(GridKind::Hex, cli);
    } else {
        macroquad::Window::from_config(conf(), minesweeper::run(GridKind::Hex, cli));
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    macroquad::Window::from_config(
        conf(),
        minesweeper::run(GridKind::Hex, minesweeper::parse_cli_args()),
    );
}
