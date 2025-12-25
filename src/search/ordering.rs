//! Move ordering heuristics.
//!
//! Good move ordering is critical for alpha-beta pruning efficiency.
//! Uses lazy selection sort to avoid full sort overhead.

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
#[inline]
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

/// Move picker struct for lazy move ordering
/// Uses selection sort - only finds next best move when needed
pub struct MovePicker {
    moves: [Move; 256],
    scores: [i32; 256],
    count: usize,
    current: usize,
}

impl MovePicker {
    /// Create a new move picker with scored moves
    pub fn new(
        board: &Board,
        moves_in: &[Move],
        tt_move: Option<Move>,
        killers: [Option<Move>; 2],
        counter_move: Option<Move>,
        history: &HistoryTable,
        color: Color,
    ) -> Self {
        let mut picker = MovePicker {
            moves: [Move::default(); 256],
            scores: [0; 256],
            count: moves_in.len(),
            current: 0,
        };
        
        for (i, &m) in moves_in.iter().enumerate() {
            picker.moves[i] = m;
            picker.scores[i] = score_move(board, m, tt_move, killers, counter_move, history, color);
        }
        
        picker
    }

    /// Get next best move using selection sort (find max, swap to front)
    #[inline]
    pub fn next(&mut self) -> Option<Move> {
        if self.current >= self.count {
            return None;
        }

        // Find best remaining move
        let mut best_idx = self.current;
        let mut best_score = self.scores[self.current];
        
        for i in (self.current + 1)..self.count {
            if self.scores[i] > best_score {
                best_score = self.scores[i];
                best_idx = i;
            }
        }

        // Swap best to current position
        self.moves.swap(self.current, best_idx);
        self.scores.swap(self.current, best_idx);
        
        let mv = self.moves[self.current];
        self.current += 1;
        Some(mv)
    }

    /// Get current move index (for LMR)
    #[inline]
    pub fn move_index(&self) -> usize {
        self.current.saturating_sub(1)
    }
}

/// Simple capture ordering for quiescence (no allocations)
pub struct CapturePicker {
    moves: [Move; 256],
    scores: [i32; 256],
    count: usize,
    current: usize,
}

impl CapturePicker {
    /// Create picker for captures only (MVV-LVA + SEE)
    pub fn new(board: &Board, moves_in: &[Move]) -> Self {
        let mut picker = CapturePicker {
            moves: [Move::default(); 256],
            scores: [0; 256],
            count: moves_in.len(),
            current: 0,
        };
        
        for (i, &m) in moves_in.iter().enumerate() {
            picker.moves[i] = m;
            // Use MVV-LVA as primary, SEE for tie-breaking
            picker.scores[i] = mvv_lva_score(board, m) * 100 + see::see(board, m).min(100);
        }
        
        picker
    }

    /// Get next best capture
    #[inline]
    pub fn next(&mut self) -> Option<Move> {
        if self.current >= self.count {
            return None;
        }

        let mut best_idx = self.current;
        let mut best_score = self.scores[self.current];
        
        for i in (self.current + 1)..self.count {
            if self.scores[i] > best_score {
                best_score = self.scores[i];
                best_idx = i;
            }
        }

        self.moves.swap(self.current, best_idx);
        self.scores.swap(self.current, best_idx);
        
        let mv = self.moves[self.current];
        self.current += 1;
        Some(mv)
    }
}

// Keep old functions for compatibility during migration
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
