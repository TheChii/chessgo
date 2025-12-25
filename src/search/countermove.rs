//! Counter-move heuristic for move ordering.
//!
//! Tracks which move typically refutes the opponent's previous move.
//! Similar to killer moves but indexed by opponent's move rather than ply.

use crate::types::Move;

/// Counter-move table: [from_sq][to_sq] -> counter move
/// Stores the move that refuted a given opponent move
#[derive(Clone)]
pub struct CounterMoveTable {
    table: [[Option<Move>; 64]; 64],
}

impl CounterMoveTable {
    /// Create a new empty counter-move table
    pub fn new() -> Self {
        Self {
            table: [[None; 64]; 64],
        }
    }

    /// Store a counter-move for the opponent's previous move
    #[inline]
    pub fn store(&mut self, opponent_move: Move, counter: Move) {
        let from = opponent_move.get_source().to_index();
        let to = opponent_move.get_dest().to_index();
        self.table[from][to] = Some(counter);
    }

    /// Get the counter-move for the opponent's previous move
    #[inline]
    pub fn get(&self, opponent_move: Move) -> Option<Move> {
        let from = opponent_move.get_source().to_index();
        let to = opponent_move.get_dest().to_index();
        self.table[from][to]
    }

    /// Check if a move is the counter-move for the opponent's previous move
    #[inline]
    pub fn is_counter(&self, opponent_move: Move, mv: Move) -> bool {
        self.get(opponent_move) == Some(mv)
    }

    /// Clear all counter-moves (typically on new game, not new search)
    pub fn clear(&mut self) {
        self.table = [[None; 64]; 64];
    }
}

impl Default for CounterMoveTable {
    fn default() -> Self {
        Self::new()
    }
}
