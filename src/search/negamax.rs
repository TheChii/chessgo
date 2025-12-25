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
    prev_move: Option<Move>,
) -> SearchResult {
    searcher.inc_nodes();
    searcher.update_seldepth(ply);

    let hash = board.get_hash();

    // === Repetition Detection ===
    // Check for draw by repetition (position seen before in game history)
    if ply.raw() > 0 && searcher.is_repetition(hash) {
        return SearchResult {
            best_move: None,
            score: Score::draw(),
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    let orig_alpha = alpha;
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
                    false,
                    None,  // No prev move for null move
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
    let color = board.side_to_move();
    
    // Get counter-move for opponent's previous move
    let counter_move = prev_move.and_then(|pm| searcher.countermoves.get(pm));

    // Order moves (TT, killers, counter-move, and history)
    ordering::order_moves_full(board, &mut moves, tt_move, killers, counter_move, &searcher.history, color);

    // Cache static eval for futility pruning (only compute once per node)
    let static_eval = if depth.raw() <= 3 && !in_check {
        Some(eval::evaluate(board, searcher.nnue.as_ref()))
    } else {
        None
    };

    let mut best_move = None;
    let mut best_score = Score::neg_infinity();
    let mut pv = Vec::new();
    // Use fixed-size array for searched quiets to avoid allocations
    let mut searched_quiets: [Move; 64] = [Move::default(); 64];
    let mut quiets_count = 0usize;

    for (move_idx, &m) in moves.iter().enumerate() {
        let new_board = board.make_move_new(m);

        // Prefetch TT entry for next position
        searcher.tt.prefetch(new_board.get_hash());

        // Determine if this is a quiet move (for LMR)
        let is_capture = board.piece_on(m.get_dest()).is_some();
        let is_promotion = m.get_promotion().is_some();
        let is_killer = killers[0] == Some(m) || killers[1] == Some(m);
        let is_quiet = !is_capture && !is_promotion;
        let gives_check = new_board.checkers().popcnt() > 0;

        // LMR: Late Move Reductions
        // Reduce depth for late quiet moves that aren't special
        let mut reduced = false;
        
        // Check extension: extend +1 when in check to avoid horizon effect
        let extension = if in_check { 1 } else { 0 };
        
        let search_depth = if move_idx >= 4 
            && depth.raw() >= 3 
            && is_quiet 
            && !in_check 
            && !gives_check
            && !is_killer
        {
            // Logarithmic reduction formula
            let d = (depth.raw() as f32).ln();
            let m_idx = ((move_idx + 1) as f32).ln();
            let reduction = ((d * m_idx) / 2.5) as i32;
            let reduction = reduction.min(depth.raw() - 2).max(1);
            reduced = true;
            Depth::new((depth.raw() - 1 - reduction + extension).max(1))
        } else {
            Depth::new((depth.raw() - 1 + extension).max(0))
        };

        // === Futility Pruning ===
        // At shallow depths, skip quiet moves if eval + margin is below alpha
        if let Some(se) = static_eval {
            if is_quiet && !gives_check && move_idx > 0 {
                let margin = 100 * depth.raw();
                if se.raw() + margin < alpha.raw() {
                    // Track for history
                    if quiets_count < 64 {
                        searched_quiets[quiets_count] = m;
                        quiets_count += 1;
                    }
                    continue;  // Prune this move
                }
            }
        }

        // === Principal Variation Search (PVS) ===
        let mut result;
        let mut score;
        
        if move_idx == 0 {
            // First move: search with full window
            result = search(
                searcher,
                &new_board,
                search_depth,
                ply.next(),
                -beta,
                -alpha,
                true,
                Some(m),  // Pass current move as prev_move
            );
            score = -result.score;
        } else {
            // Later moves: null window search first
            result = search(
                searcher,
                &new_board,
                search_depth,
                ply.next(),
                -alpha - Score::cp(1),
                -alpha,
                true,
                Some(m),
            );
            score = -result.score;
            
            // Re-search with full window if fails high
            if score > alpha && score < beta && !searcher.should_stop() {
                result = search(
                    searcher,
                    &new_board,
                    search_depth,
                    ply.next(),
                    -beta,
                    -alpha,
                    true,
                    Some(m),
                );
                score = -result.score;
            }
        }

        // Re-search at full depth if LMR reduced search beats alpha
        if reduced && score > alpha && !searcher.should_stop() {
            result = search(
                searcher,
                &new_board,
                Depth::new((depth.raw() - 1 + extension).max(0)),
                ply.next(),
                -beta,
                -alpha,
                true,
                Some(m),
            );
            score = -result.score;
        }

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
                    // Beta cutoff - update killer, history, and counter-move for quiet moves
                    if is_quiet {
                        searcher.killers.store(ply, m);
                        // Update history: bonus for cutoff move, penalty for searched quiets
                        searcher.history.update_on_cutoff(color, m, depth.raw(), &searched_quiets[..quiets_count]);
                        // Update counter-move
                        if let Some(pm) = prev_move {
                            searcher.countermoves.store(pm, m);
                        }
                    }
                    break;
                }
            }
        }
        
        // Track searched quiet moves for history penalty
        if is_quiet && quiets_count < 64 {
            searched_quiets[quiets_count] = m;
            quiets_count += 1;
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
    let stand_pat = crate::eval::evaluate(board, searcher.nnue.as_ref());

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

    // Generate capture moves only - use fixed array
    let mut moves: [Move; 64] = [Move::default(); 64];
    let mut move_count = 0;
    
    for m in MoveGen::new_legal(board) {
        if board.piece_on(m.get_dest()).is_some() && move_count < 64 {
            moves[move_count] = m;
            move_count += 1;
        }
    }

    if move_count == 0 {
        return SearchResult {
            best_move: None,
            score: alpha,
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    ordering::order_captures(board, &mut moves[..move_count]);

    let mut best_score = stand_pat;
    let mut pv = Vec::new();

    for i in 0..move_count {
        let m = moves[i];
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
