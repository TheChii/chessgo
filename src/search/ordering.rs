//! Move ordering heuristics.
//!
//! Good move ordering is critical for alpha-beta pruning efficiency.
//! This module provides ordering functions that can be extended with:
//! - Transposition table moves (best first)
//! - Killer moves (quiet moves that caused cutoffs)
//! - History heuristic (success rate of quiet moves)
//! - Counter-move heuristic

use crate::types::{Board, Move, piece_value};

/// MVV-LVA scores for capture ordering
/// Higher score = search first
/// MVV-LVA: Most Valuable Victim - Least Valuable Attacker
fn mvv_lva_score(board: &Board, m: Move) -> i32 {
    let victim = board.piece_on(m.get_dest());
    let attacker = board.piece_on(m.get_source());

    match (victim, attacker) {
        (Some(v), Some(a)) => {
            // Victim value * 10 - Attacker value
            // This prioritizes capturing high-value pieces with low-value pieces
            piece_value(v) * 10 - piece_value(a)
        }
        _ => 0,
    }
}

/// Score a move for ordering (higher = search first)
fn score_move(board: &Board, m: Move) -> i32 {
    let mut score = 0;

    // Promotions are very important
    if let Some(promo) = m.get_promotion() {
        score += piece_value(promo) + 10000;
    }

    // Captures scored by MVV-LVA
    if board.piece_on(m.get_dest()).is_some() {
        score += mvv_lva_score(board, m) + 5000;
    }

    // Future: add TT move bonus (+20000)
    // Future: add killer move bonus (+4000)
    // Future: add history heuristic score

    score
}

/// Order moves for main search
/// Call this before iterating through moves in negamax
pub fn order_moves(board: &Board, moves: &mut [Move]) {
    // Score all moves
    let mut scored: Vec<(Move, i32)> = moves.iter()
        .map(|&m| (m, score_move(board, m)))
        .collect();

    // Sort by score descending (highest first)
    scored.sort_by(|a, b| b.1.cmp(&a.1));

    // Write back sorted moves
    for (i, (m, _)) in scored.into_iter().enumerate() {
        moves[i] = m;
    }
}

/// Order captures for quiescence search (MVV-LVA only)
pub fn order_captures(board: &Board, moves: &mut [Move]) {
    // Score by MVV-LVA
    let mut scored: Vec<(Move, i32)> = moves.iter()
        .map(|&m| (m, mvv_lva_score(board, m)))
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));

    for (i, (m, _)) in scored.into_iter().enumerate() {
        moves[i] = m;
    }
}

// === Future: Killer moves tracking ===
// pub struct KillerTable {
//     killers: [[Option<Move>; 2]; MAX_PLY],
// }

// === Future: History heuristic ===
// pub struct HistoryTable {
//     // [color][from][to] -> score
//     table: [[[i32; 64]; 64]; 2],
// }
