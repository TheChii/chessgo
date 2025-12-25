//! Negamax alpha-beta search implementation.
//!
//! This is the core search algorithm with:
//! - Transposition table probing and storing
//! - Alpha-beta pruning
//! - Quiescence search for captures
//!
//! Future extensions: null move pruning, LMR, futility pruning

use super::{Searcher, SearchStats, ordering};
use super::tt::BoundType;
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

/// Main negamax search function with TT integration and null move pruning
pub fn search(
    searcher: &mut Searcher,
    board: &Board,
    depth: Depth,
    ply: Ply,
    mut alpha: Score,
    beta: Score,
    allow_null: bool,
) -> SearchResult {
    searcher.inc_nodes();
    searcher.update_seldepth(ply);

    let orig_alpha = alpha;
    let hash = board.get_hash();
    let mut tt_move: Option<Move> = None;

    // === TT Probe ===
    if let Some(entry) = searcher.tt.probe(hash) {
        tt_move = entry.best_move();
        
        // Only use TT score if depth is sufficient
        if entry.depth() >= depth {
            let tt_score = entry.score().from_tt(ply.raw());
            
            match entry.bound() {
                BoundType::Exact => {
                    return SearchResult {
                        best_move: tt_move,
                        score: tt_score,
                        pv: tt_move.map(|m| vec![m]).unwrap_or_default(),
                        stats: searcher.stats().clone(),
                    };
                }
                BoundType::LowerBound => {
                    if tt_score >= beta {
                        return SearchResult {
                            best_move: tt_move,
                            score: tt_score,
                            pv: tt_move.map(|m| vec![m]).unwrap_or_default(),
                            stats: searcher.stats().clone(),
                        };
                    }
                    if tt_score > alpha {
                        alpha = tt_score;
                    }
                }
                BoundType::UpperBound => {
                    if tt_score <= alpha {
                        return SearchResult {
                            best_move: tt_move,
                            score: tt_score,
                            pv: tt_move.map(|m| vec![m]).unwrap_or_default(),
                            stats: searcher.stats().clone(),
                        };
                    }
                }
                BoundType::None => {}
            }
        }
    }

    // Check for stop condition
    if searcher.should_stop() {
        return SearchResult {
            best_move: None,
            score: Score::draw(),
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    let in_check = *board.checkers() != chess::EMPTY;

    // === Null Move Pruning ===
    // Skip if: in check, depth too low, null move disabled, or only king+pawns
    if allow_null && !in_check && depth.raw() >= 3 {
        // Don't do null move in pure pawn endgames (zugzwang risk)
        let dominated_by_pawns = (board.pieces(chess::Piece::Knight)
            | board.pieces(chess::Piece::Bishop)
            | board.pieces(chess::Piece::Rook)
            | board.pieces(chess::Piece::Queen)).popcnt() == 0;
        
        if !dominated_by_pawns {
            // Reduction: R=3 if depth > 6, else R=2
            let r = if depth.raw() > 6 { 3 } else { 2 };
            
            if let Some(null_board) = board.null_move() {
                let null_result = search(
                    searcher,
                    &null_board,
                    Depth::new((depth.raw() - 1 - r).max(0)),
                    ply.next(),
                    -beta,
                    -beta + Score::cp(1),
                    false,  // Don't allow consecutive null moves
                );
                
                let null_score = -null_result.score;
                
                if null_score >= beta {
                    // Null move cutoff
                    return SearchResult {
                        best_move: None,
                        score: beta,
                        pv: Vec::new(),
                        stats: searcher.stats().clone(),
                    };
                }
            }
        }
    }

    // Generate legal moves
    let mut moves: Vec<Move> = MoveGen::new_legal(board).collect();

    // Check for checkmate or stalemate
    if moves.is_empty() {
        let score = if *board.checkers() != chess::EMPTY {
            Score::mated_in(ply.raw())
        } else {
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

    // Get killers for this ply
    let killers = searcher.killers.get(ply);

    // Order moves (TT move and killers will be searched first)
    ordering::order_moves_with_tt_and_killers(board, &mut moves, tt_move, killers);

    let mut best_move = None;
    let mut best_score = Score::neg_infinity();
    let mut pv = Vec::new();

    for &m in moves.iter() {
        let new_board = board.make_move_new(m);

        // Prefetch TT entry for next position
        searcher.tt.prefetch(new_board.get_hash());

        let result = search(
            searcher,
            &new_board,
            depth - 1,
            ply.next(),
            -beta,
            -alpha,
            true,  // Allow null move in recursive search
        );

        let score = -result.score;

        if searcher.should_stop() {
            break;
        }

        if score > best_score {
            best_score = score;
            best_move = Some(m);

            pv.clear();
            pv.push(m);
            pv.extend(result.pv);

            if score > alpha {
                alpha = score;

                if score >= beta {
                    // Beta cutoff - store killer if quiet move
                    if board.piece_on(m.get_dest()).is_none() && m.get_promotion().is_none() {
                        searcher.killers.store(ply, m);
                    }
                    break;
                }
            }
        }
    }

    // === TT Store ===
    if !searcher.should_stop() {
        let bound = if best_score >= beta {
            BoundType::LowerBound
        } else if best_score > orig_alpha {
            BoundType::Exact
        } else {
            BoundType::UpperBound
        };

        searcher.tt.store(
            hash,
            best_move,
            best_score.to_tt(ply.raw()),
            depth,
            bound,
        );
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

    // Stand-pat evaluation
    let stand_pat = eval::evaluate(board, searcher.nnue.as_ref());

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
