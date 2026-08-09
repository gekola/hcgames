use crate::game::{Bottle, Color, Game, Move, Phase};
use beam_solver::{BeamSearch, SearchState};

/// See `lib/beam_solver` docs and root CLAUDE.md's "Self-playing solver games" section
/// for the shared engine and why this game fits it: many pours are simultaneously
/// legal each tick and need to be scored against each other, same shape as
/// klondike/spider/match-3/bubble-shooter. Constants reasoned from board scale (up to
/// ~16 bottles at the highest tiers, small `legal_moves()` per state) rather than
/// measured/tuned yet — see this crate's CLAUDE.md.
const BEAM_WIDTH: usize = 10;
const BEAM_DEPTH: u32 = 6;
const NODE_BUDGET: usize = 4000;

impl SearchState for Game {
    type Move = Move;

    fn legal_moves(&self) -> Vec<Move> {
        Game::legal_moves(self)
    }

    fn apply(&mut self, m: Move) {
        Game::apply(self, m)
    }

    fn state_hash(&self) -> u64 {
        Game::state_hash(self)
    }

    fn is_terminal(&self) -> bool {
        self.phase != Phase::Playing
    }
}

pub struct Solver {
    beam: BeamSearch<Game>,
}

impl Solver {
    pub fn new() -> Self {
        Self {
            beam: BeamSearch::new(BEAM_WIDTH, BEAM_DEPTH, NODE_BUDGET, |_| false),
        }
    }

    pub fn choose_move(&mut self, game: &Game) -> Option<Move> {
        self.beam.choose_move(game, is_pointless, score, score)
    }
}

/// Pouring an already-monochrome source into an empty bottle only relabels which
/// bottle holds that color — it doesn't merge anything, free up a differently-colored
/// bottle, or move any bottle closer to solved. Provably never helps: whatever the
/// monochrome group could do from the empty bottle, it could already do from its
/// current one, since a monochrome partial stack accepts the same future pours (more
/// of its own color) wherever it sits. Without this filter the beam spent width on
/// pure bottle-shuffling that never changed the board's real shape.
///
/// Deliberately *not* also collapsing interchangeable empty destinations (only offering
/// the lowest-indexed empty bottle, since no rule in `game.rs` tells two empty unlocked
/// bottles apart). That looks like free branching-factor savings and was measured: it
/// cut levels reached in a fixed 60s budget from 669 to 654 across 8 seeds, because the
/// candidate enumeration it trims isn't where the per-move cost is, while the distinct
/// board hashes it removes do cost the beam real line diversity.
fn is_pointless(game: &Game, m: &Move) -> bool {
    let src = &game.bottles[m.from];
    let dst = &game.bottles[m.to];
    dst.liquid.is_empty() && src.top_run() == src.liquid.len()
}

/// Score is a **potential difference**, not a bag of per-move rewards: every move is
/// worth `Φ(after) - Φ(before)`, where `Φ` is the sum of `bottle_potential` over the
/// board (plus `UNLOCK` per unlocked bottle). The cumulative score `beam_solver`
/// accumulates along a line therefore telescopes to `Φ(final) - Φ(root)`: two lines
/// reaching the *same* board score identically no matter how many moves they took to
/// get there, and any round trip sums to exactly zero. Nothing in the search can be
/// paid for taking a detour.
///
/// **Fixed bug** — this shape is what the "same color poured into two different empty
/// bottles" report turned out to be. The old scoring handed out `+150` every time a
/// pour emptied its source and only `-30` when a pour consumed an empty bottle, so the
/// solver could *farm* the difference: splitting a color into a fresh empty bottle
/// (`-30`) and then merging it straight back (`+150` for emptying the bottle it had
/// just filled, `+74` for the pure-color merge) scored `+194` over two plies, against
/// `+74` for pouring the same unit directly onto the same-color bottle in one ply —
/// even though both lines end on an identical board. With `BEAM_DEPTH=6` the beam saw
/// the two-move detour and took it, every time, which is exactly the "systematically
/// creates a second partial bottle of a color that already has one" pattern. Reproduced
/// on `HCG_SEED=7`, level 7 generation 6, moves 3-4 (`3->5` scored 74 and was passed
/// over for `3->6` at -30). Making the two bonuses two sides of one state term — an
/// empty bottle is simply worth 150 in `bottle_potential` — removes the arbitrage by
/// construction rather than by re-tuning the `-30`, which could only ever have traded
/// this bug against over-penalizing the empty pours that genuinely are necessary.
fn score(game: &Game, after: &Game, m: &Move) -> i32 {
    if after.phase == Phase::Won {
        return 100_000;
    }

    // Evaluated as a delta over only the two bottles the pour touched (every other
    // bottle's contents are identical in `game` and `after`, so their terms cancel)
    // plus the unlock flags — exactly `potential(after) - potential(game)`, without
    // rescanning a 16-bottle board twice per candidate. `score` runs once per generated
    // move at every ply, so this is the hot path.
    let mut d = bottle_potential(&after.bottles[m.from]) - bottle_potential(&game.bottles[m.from]);
    d += bottle_potential(&after.bottles[m.to]) - bottle_potential(&game.bottles[m.to]);

    let newly_unlocked = after
        .bottles
        .iter()
        .zip(&game.bottles)
        .filter(|(a, b)| a.unlocked && !b.unlocked)
        .count() as i32;
    d + newly_unlocked * UNLOCK
}

/// Weight of one unlocked locked bottle — the only `potential` term that isn't a
/// property of a single bottle's `liquid` alone, and the one place the potential is
/// one-way (`Bottle::unlocked` is sticky, so it can never be paid twice or refunded).
const UNLOCK: i32 = 300;

/// One bottle's contribution to the board potential above. In the same units the old
/// per-move bonuses used, so the relative weights carry over:
///
/// * solved (full + monochrome, and therefore sealed forever): `500`, replacing the old
///   `+500`-on-completion move bonus.
/// * empty: `150`, replacing the old `+150`-for-emptying-a-source move bonus. As a
///   *state* term this is symmetric — a pour that consumes an empty bottle now costs the
///   same 150 that emptying one earns, which is the fix described on `score`.
/// * anything else: `15` per unit in the top run (a deeper matching run is closer to a
///   merge), `+55` if the bottle is monochrome (a pure partial stack is the one thing
///   that can still go on to complete a color), `-20` per extra distinct color and `-10`
///   per extra run (fragmentation — `abab` is strictly worse to untangle than `aabb`).
///
/// The old `destination_bonus`'s pure-vs-mixed destination preference falls out of this
/// rather than needing its own rule: pouring onto a mixed bottle leaves that bottle
/// mixed, so the resulting board keeps paying its `-20`/`-10`, while a pure destination
/// keeps its `+55`.
fn bottle_potential(b: &Bottle) -> i32 {
    if b.is_solved() {
        return 500;
    }
    if b.liquid.is_empty() {
        return 150;
    }

    let mut color_mask: u16 = 0;
    let mut runs = 0;
    let mut prev: Option<Color> = None;
    for &c in &b.liquid {
        color_mask |= 1 << c;
        if prev != Some(c) {
            runs += 1;
        }
        prev = Some(c);
    }
    let distinct = color_mask.count_ones() as i32;

    let mut s = 15 * b.top_run() as i32;
    if distinct == 1 {
        s += 55;
    }
    s -= 20 * (distinct - 1);
    s -= 10 * (runs - 1);
    s
}
