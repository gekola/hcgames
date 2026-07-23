//! Generic beam search over a small, `Clone`-able, fully-observable single-player game
//! state — the solver engine shared by the self-playing solitaire games (Klondike,
//! Spider) in this workspace. Branches by `clone()` + `apply()` rather than real undo,
//! which is cheap here since a full board fits in a handful of small `Vec`s.
//!
//! The engine itself (ply-by-ply expand/score/truncate, carrying terminal lines forward
//! instead of dropping them, hard-excluding already-visited states) is domain-agnostic.
//! Move legality, "pointless move" filtering, and scoring are entirely up to the caller —
//! every game's heuristics are tuned to its own rules and stay in that game's own solver.

use std::collections::HashSet;

/// A single-player, fully-observable, deterministic game state a [`BeamSearch`] can
/// search over.
pub trait SearchState: Clone {
    type Move: Copy;

    fn legal_moves(&self) -> Vec<Self::Move>;
    fn apply(&mut self, m: Self::Move);
    fn state_hash(&self) -> u64;
    /// No further moves should be generated from this state (won, stuck, or otherwise
    /// finished) — `legal_moves` may still return an empty vec on its own, but checking
    /// this first avoids the wasted call on every terminal beam line, every ply.
    fn is_terminal(&self) -> bool;
}

/// Beam search parameters plus the running set of previously-visited states, which
/// persists across `choose_move` calls for the lifetime of one game (a fresh
/// `BeamSearch` per game, same as a fresh `Solver` per game in each caller).
pub struct BeamSearch<S: SearchState> {
    width: usize,
    depth: u32,
    /// Hard ceiling on total `legal_moves()` candidates expanded across an entire
    /// `choose_move` call (all plies combined). `width`/`depth` bound the *typical* cost,
    /// but a handful of pathological board states (e.g. Spider with several empty tableau
    /// columns at once, before symmetric-destination filtering trims them) can blow the
    /// per-ply candidate count up by 10-50x, which compounds across plies — measured
    /// spikes past 1 second on states `width`/`depth` alone predicted would cost single-
    /// digit milliseconds. This budget is a node count, not wall-clock time: no
    /// `Instant`/`SystemTime` available cross-platform here, since this crate stays
    /// dependency-free and callers include WASM builds where `std::time::Instant` isn't
    /// reliably usable without wasm-bindgen (this workspace deliberately doesn't use it —
    /// see the WASM caveats in the root CLAUDE.md). Node count correlates closely enough
    /// with wall time in practice to serve the same purpose.
    node_budget: usize,
    visited: HashSet<u64>,
    /// Moves exempt from the revisit hard-exclusion below — legitimately repeatable
    /// actions (Klondike's `DrawStock`/`ResetStock`, Spider's `Deal`) that consume stock
    /// rather than "returning" to an earlier board, so revisiting the state they produce
    /// isn't the dead-end signal it is for every other move.
    is_revisit_exempt: fn(&S::Move) -> bool,
}

struct BeamNode<S: SearchState> {
    game: S,
    /// Which of the real, immediately-legal moves this line branched from — carried
    /// unchanged through every further ply so the search can report it back at the end.
    first_move: S::Move,
    /// Cumulative score across the whole line so far. Comparing cumulative sums is only
    /// meaningful between nodes at the *same* ply, which is all this ever does — every
    /// surviving line is expanded (or carried forward, if it can't be) exactly once per
    /// ply.
    score: i32,
}

impl<S: SearchState> BeamSearch<S> {
    pub fn new(
        width: usize,
        depth: u32,
        node_budget: usize,
        is_revisit_exempt: fn(&S::Move) -> bool,
    ) -> Self {
        Self {
            width,
            depth,
            node_budget,
            visited: HashSet::new(),
            is_revisit_exempt,
        }
    }

    /// Picks the best move from `game` by expanding a beam of `width` candidate lines
    /// `depth` plies deep. `is_pointless` filters candidates at every ply (not just the
    /// root) — it should be cheap, since it runs once per generated move. `score_root`
    /// scores the real, immediately-legal first move (ply 0: one call per candidate, so
    /// affording something pricier like a mobility lookahead is fine); `score_step`
    /// scores every move at every later ply (called far more often — keep it cheap).
    /// Both receive `(state_before, state_after, move)`.
    pub fn choose_move(
        &mut self,
        game: &S,
        is_pointless: impl Fn(&S, &S::Move) -> bool,
        score_root: impl Fn(&S, &S, &S::Move) -> i32,
        score_step: impl Fn(&S, &S, &S::Move) -> i32,
    ) -> Option<S::Move> {
        let moves: Vec<S::Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| !is_pointless(game, m))
            .collect();

        if moves.is_empty() {
            return None;
        }

        self.visited.insert(game.state_hash());

        // Hard-exclude (rather than merely penalize) moves that lead straight back into
        // an already-visited state: a soft penalty only deters a revisit when there's
        // something else to compare it against, but when it's the *only* candidate left
        // (a fully locked position's sole legal move can be a reversible round trip) it
        // would get picked, and re-picked, forever.
        let fresh: Vec<S::Move> = moves
            .into_iter()
            .filter(|m| {
                if (self.is_revisit_exempt)(m) {
                    return true;
                }
                let mut preview = game.clone();
                preview.apply(*m);
                !self.visited.contains(&preview.state_hash())
            })
            .collect();

        if fresh.is_empty() {
            return None;
        }

        let mut beam: Vec<BeamNode<S>> = fresh
            .into_iter()
            .map(|m| {
                let mut g = game.clone();
                g.apply(m);
                let score = score_root(game, &g, &m);
                BeamNode {
                    game: g,
                    first_move: m,
                    score,
                }
            })
            .collect();
        beam.sort_unstable_by_key(|n| std::cmp::Reverse(n.score));
        beam.truncate(self.width);

        let mut nodes_expanded = beam.len();
        let mut budget_exhausted = false;

        for _ in 1..self.depth {
            if budget_exhausted {
                break;
            }

            let mut next: Vec<BeamNode<S>> = Vec::with_capacity(beam.len() * 4);

            for node in &beam {
                // Checked per-node, not just per-ply: a single pathological board state
                // (several empty tableau columns colliding with a long consolidation
                // opportunity) can make one node's own `legal_moves()` call return
                // hundreds of candidates, blowing the budget mid-ply before a per-ply-only
                // check would ever see it. Once exhausted, every remaining node in this
                // ply — including the one that pushed us over — is carried forward
                // unchanged rather than expanded, same treatment as a terminal/dead-end
                // line, so no line is unfairly dropped just because of processing order.
                if budget_exhausted || nodes_expanded >= self.node_budget {
                    budget_exhausted = true;
                    next.push(BeamNode {
                        game: node.game.clone(),
                        first_move: node.first_move,
                        score: node.score,
                    });
                    continue;
                }

                let candidates: Vec<S::Move> = if node.game.is_terminal() {
                    Vec::new()
                } else {
                    node.game
                        .legal_moves()
                        .into_iter()
                        .filter(|m| !is_pointless(&node.game, m))
                        .collect()
                };
                nodes_expanded += candidates.len();

                if candidates.is_empty() {
                    // Nothing left down this line — won, stuck, or filtered to nothing.
                    // Carry it forward unchanged rather than dropping it, so a strong
                    // line found early (a win, especially) isn't discarded just because
                    // it can't be expanded further, and still gets compared fairly
                    // against lines that ran the full depth.
                    next.push(BeamNode {
                        game: node.game.clone(),
                        first_move: node.first_move,
                        score: node.score,
                    });
                    continue;
                }

                for m in candidates {
                    let mut g = node.game.clone();
                    g.apply(m);
                    let step = score_step(&node.game, &g, &m);
                    next.push(BeamNode {
                        game: g,
                        first_move: node.first_move,
                        score: node.score + step,
                    });
                }
            }

            next.sort_unstable_by_key(|n| std::cmp::Reverse(n.score));
            next.truncate(self.width);
            beam = next;
        }

        beam.into_iter().next().map(|n| n.first_move)
    }
}
