//! Move ordering heuristics.
//!
//! Good move ordering is critical for alpha-beta pruning efficiency.
//! Uses lazy selection sort to avoid full sort overhead.

use crate::types::{Board, Move, Piece, Color, piece_value};
use super::history::HistoryTable;
use super::see;

/// Move score constants
const TT_MOVE_BONUS: i32 = 1_000_000;
const PROMOTION_BONUS: i32 = 100_000;
const GOOD_CAPTURE_BONUS: i32 = 60_000;
const KILLER_0_BONUS: i32 = 40_000;
const KILLER_1_BONUS: i32 = 35_000;
const COUNTER_MOVE_BONUS: i32 = 30_000;
const BAD_CAPTURE_PENALTY: i32 = -10_000;

/// MVV-LVA scores for capture ordering
#[inline]
fn mvv_lva_score(board: &Board, m: Move) -> i32 {
    let victim = board.piece_at(m.to()).map(|(p, _)| p);
    let attacker = board.piece_at(m.from()).map(|(p, _)| p);

    match (victim, attacker) {
        (Some(v), Some(a)) => {
            piece_value(v) * 10 - piece_value(a)
        }
        _ => 0,
    }
}

/// Score a move for ordering (higher = search first)
#[inline]
pub fn score_move(
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
    if let Some(promo) = m.flag().promotion_piece() {
        score += piece_value(promo) + PROMOTION_BONUS;
    }

    // Captures: skip SEE for obviously good captures (victim >= attacker)
    if m.is_capture() {
        let mvv_lva = mvv_lva_score(board, m);
        if mvv_lva >= 0 {
            // Winning or equal capture (e.g., PxQ, NxN) - skip expensive SEE
            score += GOOD_CAPTURE_BONUS + mvv_lva;
        } else {
            // Potentially losing capture - use SEE to verify
            let see_value = see::see(board, m);
            if see_value >= 0 {
                score += GOOD_CAPTURE_BONUS + mvv_lva;
            } else {
                score += BAD_CAPTURE_PENALTY + mvv_lva;
            }
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

#[allow(dead_code)]
pub fn order_moves_full(
    board: &Board, 
    moves: &mut [Move], 
    tt_move: Option<Move>,
    killers: [Option<Move>; 2],
    counter_move: Option<Move>,
    history: &HistoryTable,
    color: Color,
) {
    // Score moves in place
    let mut scores: [i32; 256] = [0; 256];
    let count = moves.len().min(256);
    
    for i in 0..count {
        scores[i] = score_move(board, moves[i], tt_move, killers, counter_move, history, color);
    }
    
    // Selection sort by scores (in-place, no allocation)
    for i in 0..count {
        let mut best_idx = i;
        let mut best_score = scores[i];
        
        for j in (i + 1)..count {
            if scores[j] > best_score {
                best_score = scores[j];
                best_idx = j;
            }
        }
        
        if best_idx != i {
            moves.swap(i, best_idx);
            scores.swap(i, best_idx);
        }
    }
}

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

#[allow(dead_code)]
pub fn order_captures(board: &Board, moves: &mut [Move]) {
    let mut scores: [i32; 256] = [0; 256];
    let count = moves.len().min(256);
    
    for i in 0..count {
        scores[i] = mvv_lva_score(board, moves[i]);
    }
    
    for i in 0..count {
        let mut best_idx = i;
        let mut best_score = scores[i];
        
        for j in (i + 1)..count {
            if scores[j] > best_score {
                best_score = scores[j];
                best_idx = j;
            }
        }
        
        if best_idx != i {
            moves.swap(i, best_idx);
            scores.swap(i, best_idx);
        }
    }
}
