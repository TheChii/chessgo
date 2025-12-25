//! Negamax alpha-beta search implementation.
//!
//! This is the core search algorithm. Currently implements basic alpha-beta
//! with negamax framework. Designed for easy extension with:
//! - Transposition table lookups/stores
//! - Null move pruning
//! - Late move reductions
//! - Futility pruning
//! - etc.

use super::{Searcher, SearchStats, ordering};
use crate::types::{Board, Move, Score, Depth, Ply, MoveGen};
use crate::eval;

/// Result from a search
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: Score,
    pub pv: Vec<Move>,
    pub stats: SearchStats,
}

/// Main negamax search function
///
/// # Arguments
/// * `searcher` - Searcher state for statistics and stop checking
/// * `board` - Current position
/// * `depth` - Remaining depth to search
/// * `ply` - Current ply from root
/// * `alpha` - Alpha bound
/// * `beta` - Beta bound
///
/// # Returns
/// SearchResult with best move and score
pub fn search(
    searcher: &mut Searcher,
    board: &Board,
    depth: Depth,
    ply: Ply,
    mut alpha: Score,
    beta: Score,
) -> SearchResult {
    searcher.inc_nodes();
    searcher.update_seldepth(ply);

    // Check for stop condition
    if searcher.should_stop() {
        return SearchResult {
            best_move: None,
            score: Score::draw(),
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    // Generate legal moves
    let mut moves: Vec<Move> = MoveGen::new_legal(board).collect();

    // Check for checkmate or stalemate
    if moves.is_empty() {
        let score = if *board.checkers() != chess::EMPTY {
            // Checkmate - return mate score adjusted for ply
            Score::mated_in(ply.raw())
        } else {
            // Stalemate
            Score::draw()
        };
        return SearchResult {
            best_move: None,
            score,
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    // Quiescence search at depth 0
    if depth.is_qs() {
        return quiescence(searcher, board, ply, alpha, beta);
    }

    // Order moves for better pruning
    ordering::order_moves(board, &mut moves);

    let mut best_move = None;
    let mut best_score = Score::neg_infinity();
    let mut pv = Vec::new();

    for &m in moves.iter() {
        // Make move
        let new_board = board.make_move_new(m);

        // Recursive search
        let result = search(
            searcher,
            &new_board,
            depth - 1,
            ply.next(),
            -beta,
            -alpha,
        );

        let score = -result.score;

        // Check for stop during search
        if searcher.should_stop() {
            break;
        }

        if score > best_score {
            best_score = score;
            best_move = Some(m);

            // Build PV
            pv.clear();
            pv.push(m);
            pv.extend(result.pv);

            if score > alpha {
                alpha = score;

                // Beta cutoff
                if score >= beta {
                    // Future: update killer moves, history heuristic here
                    break;
                }
            }
        }
    }

    SearchResult {
        best_move,
        score: best_score,
        pv,
        stats: searcher.stats().clone(),
    }
}

/// Quiescence search - search captures only to avoid horizon effect
fn quiescence(
    searcher: &mut Searcher,
    board: &Board,
    ply: Ply,
    mut alpha: Score,
    beta: Score,
) -> SearchResult {
    searcher.inc_nodes();
    searcher.update_seldepth(ply);

    // Stand-pat: evaluate the position
    let stand_pat = eval::evaluate(board);

    // Beta cutoff
    if stand_pat >= beta {
        return SearchResult {
            best_move: None,
            score: beta,
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    // Generate capture moves only
    let mut moves: Vec<Move> = MoveGen::new_legal(board)
        .filter(|m| board.piece_on(m.get_dest()).is_some())
        .collect();

    if moves.is_empty() {
        return SearchResult {
            best_move: None,
            score: alpha,
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    // Order captures by MVV-LVA
    ordering::order_captures(board, &mut moves);

    let mut best_score = stand_pat;
    let mut pv = Vec::new();

    for &m in &moves {
        if searcher.should_stop() {
            break;
        }

        let new_board = board.make_move_new(m);

        let result = quiescence(searcher, &new_board, ply.next(), -beta, -alpha);
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
