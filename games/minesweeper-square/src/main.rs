use macroquad::prelude::*;
use minesweeper::board::GridKind;

fn conf() -> Conf {
    Conf {
        window_title: "Minesweeper — Square".to_owned(),
        window_width: 900,
        window_height: 720,
        ..Default::default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let cli = minesweeper::parse_cli_args();
    if cli.no_ui {
        minesweeper::run_headless(GridKind::Square, cli);
    } else {
        macroquad::Window::from_config(conf(), minesweeper::run(GridKind::Square, cli));
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    macroquad::Window::from_config(
        conf(),
        minesweeper::run(GridKind::Square, minesweeper::parse_cli_args()),
    );
}
