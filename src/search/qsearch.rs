//! Quiescence search - search captures only to avoid horizon effect.
//!
//! When the main search reaches depth 0, we continue searching captures
//! to ensure we don't stop in the middle of a tactical sequence.
//!
//! Implements delta pruning to skip hopeless captures.
//!
//! Uses compile-time node type specialization via the `NodeType` trait.

use super::{Searcher, ordering};
use super::negamax::SearchResult;
use super::node_types::NodeType;
use super::see::is_good_capture;
use crate::types::{Board, Move, Score, Ply, Piece};
use crate::eval::SearchEvaluator;
use std::time::Instant;

/// Piece values for delta pruning (centipawns)
const PIECE_VALUES: [i32; 6] = [
    100,  // Pawn
    320,  // Knight
    330,  // Bishop
    500,  // Rook
    900,  // Queen
    0,    // King (never captured)
];

/// Delta margin: if stand_pat + best possible gain < alpha, prune
/// Using Queen value as the maximum possible gain from a single capture
const DELTA_MARGIN: i32 = 600;

/// Safety margin for individual move delta pruning
const DELTA_SAFETY: i32 = 100;

/// Get the value of a piece for delta pruning
#[inline]
fn piece_value(piece: Piece) -> i32 {
    PIECE_VALUES[piece.index()]
}

/// Quiescence search - search captures only to avoid horizon effect.
///
/// Uses compile-time node type specialization via the `NodeType` trait.
pub fn quiescence<NT: NodeType>(
    searcher: &mut Searcher,
    evaluator: &mut SearchEvaluator,
    board: &Board,
    ply: Ply,
    mut alpha: Score,
    beta: Score,
) -> SearchResult {
    searcher.inc_nodes();
    searcher.inc_qnodes();
    searcher.update_seldepth(ply);

    // Stand-pat evaluation using incremental evaluator
    searcher.inc_eval_calls();
    let t_eval = Instant::now();
    let stand_pat = evaluator.evaluate(board);
    searcher.add_eval_time(t_eval.elapsed().as_nanos() as u64);

    // Beta cutoff: position is already too good
    if stand_pat >= beta {
        return SearchResult {
            best_move: None,
            score: beta,
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    // === Delta Pruning (Big Delta) ===
    // If even capturing a queen wouldn't bring us close to alpha, give up
    let in_check = board.in_check();
    if !in_check && stand_pat.raw() + DELTA_MARGIN < alpha.raw() {
        return SearchResult {
            best_move: None,
            score: alpha,
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    // Generate all moves and filter captures
    let t_gen = Instant::now();
    let all_moves = board.generate_moves();
    let mut moves: Vec<Move> = all_moves.iter()
        .filter(|m| m.is_capture())
        .collect();
    searcher.add_gen_time(t_gen.elapsed().as_nanos() as u64);

    if moves.is_empty() {
        return SearchResult {
            best_move: None,
            score: alpha,
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    let t_order = Instant::now();
    ordering::order_captures(board, &mut moves);
    searcher.add_order_time(t_order.elapsed().as_nanos() as u64);

    let mut best_score = stand_pat;
    let mut pv = Vec::new();

    for i in 0..moves.len() {
        let m = moves[i];
        if searcher.should_stop() {
            break;
        }

        // Get captured piece value for delta pruning
        let captured = board.piece_at(m.to()).map(|(p, _)| p);
        let captured_value = captured.map(piece_value).unwrap_or(0);

        // === Delta Pruning (Per-Move) ===
        // If this capture + safety margin can't raise alpha, skip it
        // Skip this check for promotions (they gain material)
        if !in_check && !m.is_promotion() {
            if stand_pat.raw() + captured_value + DELTA_SAFETY < alpha.raw() {
                continue;
            }
        }

        // === SEE Pruning ===
        // Skip captures that lose material according to SEE
        if !in_check && !is_good_capture(board, m) {
            continue;
        }

        let new_board = board.make_move_new(m);
        
        // Clone evaluator for next depth and update incrementally
        let mut child_evaluator = evaluator.clone();
        child_evaluator.update_move(board, m); // board is position BEFORE move

        let result = quiescence::<NT::Next>(searcher, &mut child_evaluator, &new_board, ply.next(), -beta, -alpha);
        let score = -result.score;

        if score > best_score {
            best_score = score;

            pv.clear();
            pv.push(m);
            pv.extend(result.pv);

            if score > alpha {
                alpha = score;
                if score >= beta {
                    break;
                }
            }
        }
    }

    SearchResult {
        best_move: None,
        score: best_score,
        pv,
        stats: searcher.stats().clone(),
    }
}
