//! Move ordering heuristics.
//!
//! Good move ordering is critical for alpha-beta pruning efficiency.
//! This module provides ordering functions with:
//! - Transposition table moves (best first)
//! - Good captures via SEE (MVV-LVA, skip losing)
//! - Killer moves (quiet moves that caused cutoffs)
//! - Counter-moves (moves that refute opponent's last move)
//! - History heuristic (success rate of quiet moves)
//! - Promotion bonuses

use crate::types::{Board, Move, piece_value};
use super::history::HistoryTable;
use super::see;
use chess::Color;

/// Move score constants
const TT_MOVE_BONUS: i32 = 1_000_000;
const PROMOTION_BONUS: i32 = 100_000;
const GOOD_CAPTURE_BONUS: i32 = 60_000;
const KILLER_0_BONUS: i32 = 40_000;
const KILLER_1_BONUS: i32 = 35_000;
const COUNTER_MOVE_BONUS: i32 = 30_000;
const BAD_CAPTURE_PENALTY: i32 = -10_000;

/// MVV-LVA scores for capture ordering
fn mvv_lva_score(board: &Board, m: Move) -> i32 {
    let victim = board.piece_on(m.get_dest());
    let attacker = board.piece_on(m.get_source());

    match (victim, attacker) {
        (Some(v), Some(a)) => {
            piece_value(v) * 10 - piece_value(a)
        }
        _ => 0,
    }
}

/// Score a move for ordering (higher = search first)
fn score_move(
    board: &Board, 
    m: Move, 
    tt_move: Option<Move>,
    killers: [Option<Move>; 2],
    counter_move: Option<Move>,
    history: &HistoryTable,
    color: Color,
) -> i32 {
    // TT move is always searched first
    if tt_move == Some(m) {
        return TT_MOVE_BONUS;
    }

    let mut score = 0;

    // Promotions are very important
    if let Some(promo) = m.get_promotion() {
        score += piece_value(promo) + PROMOTION_BONUS;
    }

    // Captures scored by SEE
    if board.piece_on(m.get_dest()).is_some() {
        let see_value = see::see(board, m);
        if see_value >= 0 {
            // Good capture: use MVV-LVA within the bonus
            score += GOOD_CAPTURE_BONUS + mvv_lva_score(board, m);
        } else {
            // Bad capture: penalize
            score += BAD_CAPTURE_PENALTY + mvv_lva_score(board, m);
        }
    } else {
        // Quiet move - check killers and counter-move
        if killers[0] == Some(m) {
            score += KILLER_0_BONUS;
        } else if killers[1] == Some(m) {
            score += KILLER_1_BONUS;
        } else if counter_move == Some(m) {
            score += COUNTER_MOVE_BONUS;
        } else {
            // Use history score for other quiet moves
            score += history.get(color, m);
        }
    }

    score
}

/// Order moves for main search with all heuristics
pub fn order_moves_full(
    board: &Board, 
    moves: &mut [Move], 
    tt_move: Option<Move>,
    killers: [Option<Move>; 2],
    counter_move: Option<Move>,
    history: &HistoryTable,
    color: Color,
) {
    let mut scored: Vec<(Move, i32)> = moves.iter()
        .map(|&m| (m, score_move(board, m, tt_move, killers, counter_move, history, color)))
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));

    for (i, (m, _)) in scored.into_iter().enumerate() {
        moves[i] = m;
    }
}

/// Order moves without counter-move
#[allow(dead_code)]
pub fn order_moves_with_tt_and_killers(
    board: &Board, 
    moves: &mut [Move], 
    tt_move: Option<Move>,
    killers: [Option<Move>; 2],
) {
    let dummy_history = HistoryTable::new();
    order_moves_full(board, moves, tt_move, killers, None, &dummy_history, Color::White);
}

/// Order captures for quiescence search (MVV-LVA + SEE filtering)
pub fn order_captures(board: &Board, moves: &mut [Move]) {
    let mut scored: Vec<(Move, i32)> = moves.iter()
        .map(|&m| {
            let see_val = see::see(board, m);
            // Use SEE as primary, MVV-LVA as tiebreaker
            (m, see_val * 1000 + mvv_lva_score(board, m))
        })
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));

    for (i, (m, _)) in scored.into_iter().enumerate() {
        moves[i] = m;
    }
}
