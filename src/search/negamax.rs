//! Negamax alpha-beta search implementation.
//!
//! This is the core search algorithm with:
//! - Transposition table probing and storing
//! - Alpha-beta pruning
//! - Quiescence search for captures
//!
//! Future extensions: null move pruning, LMR, futility pruning

use super::{Searcher, SearchStats, ordering, qsearch, see};
use super::tt::BoundType;
use crate::types::{Board, Move, Score, Depth, Ply, Piece, SCORE_MATE};
use crate::eval::SearchEvaluator;
use std::time::Instant;

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
    evaluator: &mut SearchEvaluator,
    board: &Board,
    depth: Depth,
    ply: Ply,
    mut alpha: Score,
    mut beta: Score,
    allow_null: bool,
    prev_move: Option<Move>,
) -> SearchResult {
    searcher.inc_nodes();
    searcher.update_seldepth(ply);

    let hash = board.hash();

    // === Repetition Detection with Contempt ===
    // Check for draw by repetition (position seen before in game history)
    // Use contempt: avoid draws when winning, seek draws when losing
    if ply.raw() > 0 && searcher.is_repetition(hash) {
        // Contempt factor: small penalty/bonus for draws based on expected score
        // If alpha > 0 (we expect to be winning), penalize draws to avoid them
        // If beta < 0 (we expect to be losing), reward draws to seek them
        const CONTEMPT: i32 = 10; // Small contempt factor (centipawns)
        
        let draw_score = if alpha.raw() > CONTEMPT {
            // We're winning - penalize draws to avoid repetition
            Score::cp(-CONTEMPT)
        } else if beta.raw() < -CONTEMPT {
            // We're losing - reward draws to seek repetition  
            Score::cp(CONTEMPT)
        } else {
            // Close to equal - treat as pure draw
            Score::draw()
        };
        
        return SearchResult {
            best_move: None,
            score: draw_score,
            pv: Vec::new(),
            stats: searcher.stats().clone(),
        };
    }

    // Mate distance pruning
    let mate_score = SCORE_MATE - ply.raw() as i32;
    let mated_score = -SCORE_MATE + ply.raw() as i32;

    if alpha.raw() < mated_score {
        alpha = Score(mated_score);
        if alpha >= beta {
            return SearchResult {
                best_move: None,
                score: alpha,
                pv: Vec::new(),
                stats: searcher.stats().clone(),
            };
        }
    }

    if beta.raw() > mate_score {
        beta = Score(mate_score);
        if alpha >= beta {
            return SearchResult {
                best_move: None,
                score: beta,
                pv: Vec::new(),
                stats: searcher.stats().clone(),
            };
        }
    }

    let orig_alpha = alpha;
    let mut tt_move: Option<Move> = None;

    // === TT Probe ===
    if let Some(entry) = searcher.shared.tt.probe(hash) {
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

    let in_check = board.in_check();

    // === Reverse Futility Pruning (RFP) ===
    // If we are way ahead, we can prune without searching
    // Distinct from standard Futility Pruning which prunes *moves*
    let mut static_eval = None;
    if !in_check && depth.raw() <= 7 {
        searcher.inc_eval_calls();
        let t_eval = Instant::now();
        let eval = evaluator.evaluate(board);
        searcher.add_eval_time(t_eval.elapsed().as_nanos() as u64);
        static_eval = Some(eval);

        // RFP Margin: 75 * depth (tuneable)
        let margin = Score::cp(75 * depth.raw() as i32);
        
        if eval - margin >= beta {
             return SearchResult {
                best_move: None,
                score: eval - margin, // Soft cap to avoid crazy scores
                pv: Vec::new(),
                stats: searcher.stats().clone(),
            };
        }
    }

    // === ProbCut ===
    const PROBCUT_MARGIN: i32 = 100;
    if depth.raw() >= 5 && (beta.raw() - alpha.raw() == 1) && !in_check && beta.raw().abs() < (SCORE_MATE - 1000) {
        let probe_beta = beta + Score::cp(PROBCUT_MARGIN);
        let probe_depth = Depth::new(depth.raw() - 4);

        let result = search(
            searcher,
            evaluator,
            board,
            probe_depth,
            ply,
            probe_beta - Score::cp(1),
            probe_beta,
            false,
            None
        );

        if result.score >= probe_beta {
            return SearchResult {
                 best_move: result.best_move,
                 score: beta,
                 pv: Vec::new(),
                 stats: searcher.stats().clone()
            };
        }
    }

    // === Null Move Pruning ===
    // Skip if: in check, depth too low, null move disabled, or only king+pawns
    if allow_null && !in_check && depth.raw() >= 3 {
        // Don't do null move in pure pawn endgames (zugzwang risk)
        let dominated_by_pawns = (board.piece_bb(Piece::Knight)
            | board.piece_bb(Piece::Bishop)
            | board.piece_bb(Piece::Rook)
            | board.piece_bb(Piece::Queen)).is_empty();
        
        if !dominated_by_pawns {
            // Reduction: R=5 if depth > 6, else R=4 (aggressive)
            let r = if depth.raw() > 6 { 5 } else { 4 };
            
            // Create a null move board (pass the turn)
            let null_board = board.make_null_move();
            
            // Clone evaluator for null move (no piece updates needed)
            let mut null_evaluator = evaluator.clone();
            
            let null_result = search(
                searcher,
                &mut null_evaluator,
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

    // === Internal Iterative Deepening (IID) ===
    // If we are at a PV node and have no TT move, search shallower to find one
    if tt_move.is_none() && depth.raw() >= 6 && (beta.raw() - alpha.raw() > 1) {
        let iid_depth = Depth::new(depth.raw() - 2);
        
        let result = search(
            searcher,
            evaluator,
            board,
            iid_depth,
            ply,
            alpha,
            beta,
            allow_null,
            prev_move,
        );
        
        tt_move = result.best_move;
    }

    // Generate legal moves
    let t_gen = Instant::now();
    let moves = board.generate_moves();
    searcher.add_gen_time(t_gen.elapsed().as_nanos() as u64);

    // Check for checkmate or stalemate
    if moves.is_empty() {
        let score = if board.in_check() {
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
        return qsearch::quiescence(searcher, evaluator, board, ply, alpha, beta);
    }

    // Get killers for this ply
    let killers = searcher.killers.get(ply);
    let color = board.turn();
    
    // Get counter-move for opponent's previous move
    let counter_move = prev_move.and_then(|pm| searcher.countermoves.get(pm));

    // Collect moves into a Vec for ordering
    let mut move_vec: Vec<Move> = moves.iter().collect();
    
    // Order moves (TT, killers, counter-move, and history)
    let t_order = Instant::now();
    ordering::order_moves_full(board, &mut move_vec, tt_move, killers, counter_move, &searcher.history, color);
    searcher.add_order_time(t_order.elapsed().as_nanos() as u64);

    // Static eval is already computed for RFP if depth <= 7
    // If not (e.g. was in check check or deeper), compute it now if needed for Razoring/Futility
    if static_eval.is_none() && depth.raw() <= 3 && !in_check {
        searcher.inc_eval_calls();
        let t_eval = Instant::now();
        let val = evaluator.evaluate(board);
        searcher.add_eval_time(t_eval.elapsed().as_nanos() as u64);
        static_eval = Some(val);
    }
    
    // Razoring
    if depth.raw() <= 3 && (beta.raw() - alpha.raw() == 1) && !in_check {
        if let Some(eval) = static_eval {
            let threshold = alpha - Score::cp(200 + depth.raw() as i32 * 60);
            if eval < threshold {
                let result = qsearch::quiescence(searcher, evaluator, board, ply, alpha, beta);
                 if result.score < alpha {
                    return result; 
                }
            }
        }
    }

    let mut best_move = None;
    let mut best_score = Score::neg_infinity();
    let mut pv = Vec::new();
    // Use fixed-size array for searched quiets to avoid allocations
    let mut searched_quiets: [Move; 64] = [Move::NULL; 64];
    let mut quiets_count = 0usize;

    for (move_idx, &m) in move_vec.iter().enumerate() {
        let new_board = board.make_move_new(m);

        // Prefetch TT entry for next position
        searcher.shared.tt.prefetch(new_board.hash());

        // Determine if this is a quiet move (for LMR)
        let is_capture = m.is_capture();
        let is_promotion = m.is_promotion();
        let is_killer = killers[0] == Some(m) || killers[1] == Some(m);
        let is_quiet = !is_capture && !is_promotion;
        let gives_check = new_board.in_check();

        // LMR: Late Move Reductions
        // Reduce depth for late quiet moves that aren't special
        let mut reduced = false;
        
        // Check extension: extend +1 when in check to avoid horizon effect
        let extension = if in_check { 1 } else { 0 };
        
        let search_depth = if move_idx >= 2 
            && depth.raw() >= 3 
            && is_quiet 
            && !in_check 
            && !gives_check
            && !is_killer
        {
            // Logarithmic reduction formula
            let d = (depth.raw() as f32).ln();
            let m_idx = ((move_idx + 1) as f32).ln();
            let reduction = ((d * m_idx) / 1.9) as i32;
            let reduction = reduction.min(depth.raw() - 2).max(1);
            reduced = true;
            Depth::new((depth.raw() - 1 - reduction + extension).max(1))
        } else {
            Depth::new((depth.raw() - 1 + extension).max(0))
        };

        // === History Pruning ===
        // Prune quiet moves that have historically failed significantly
        if depth.raw() < 4 && is_quiet && !in_check && !gives_check && !is_killer && move_idx > 0 {
            // Threshold: -3000 * depth (e.g. -3000 at d1, -6000 at d2)
            let threshold = -3000 * depth.raw() as i32;
            if searcher.history.get(color, m) < threshold {
                 // Track for history stats if needed, or just prune
                continue;
            }
        }

        // === SEE Pruning for Quiet Moves ===
        // Prune quiet moves that are obvious blunders (e.g. putting a piece en prise)
        if depth.raw() <= 4 && is_quiet && !in_check && !gives_check && move_idx > 0 {
             // If move loses material (at least 50cp), prune it
             // This uses SEE to see if the move is "safe"
             if !see::see_ge(board, m, -50) {
                 continue;
             }
        }

        // === Futility Pruning ===
        // At shallow depths, skip quiet moves if eval + margin is below alpha
        if let Some(se) = static_eval {
            if is_quiet && !gives_check && move_idx > 0 {
                let margin = 150 * depth.raw();
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
            // Incremental update for next depth
            let mut child_eval = evaluator.clone();
            if !child_eval.update_move(board, m) {
                child_eval.refresh(&new_board);
            }

            // First move: search with full window
            result = search(
                searcher,
                &mut child_eval,
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
            // Incremental update
            let mut child_eval = evaluator.clone();
            if !child_eval.update_move(board, m) {
                child_eval.refresh(&new_board);
            }

            // Later moves: null window search first
            result = search(
                searcher,
                &mut child_eval,
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
                // Re-use same child_eval since board/move didn't change
                result = search(
                    searcher,
                    &mut child_eval,
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
            let mut child_eval = evaluator.clone();
            if !child_eval.update_move(board, m) {
                child_eval.refresh(&new_board);
            }

            result = search(
                searcher,
                &mut child_eval,
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

        searcher.shared.tt.store(
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
