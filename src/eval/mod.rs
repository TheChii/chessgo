//! Board evaluation module.
//!
//! Uses NNUE if available, otherwise falls back to material.

use crate::types::{Board, Score, Color, Piece, piece_value, Value};
// use crate::uci::UciHandler;

pub mod nnue;

/// Evaluate the position.
///
/// Uses NNUE if a model is provided, otherwise simple material fallback.
pub fn evaluate(board: &Board, model: Option<&nnue::Model>) -> Score {
    if let Some(m) = model {
        // Use NNUE
        nnue::evaluate(&m.model, board)
    } else {
        // Fallback to simple material
        material_eval_wrapper(board)
    }
}

/// Wrapper for material eval that returns Score
fn material_eval_wrapper(board: &Board) -> Score {
    let eval = material_eval(board);
    if board.side_to_move() == Color::White {
        Score::cp(eval)
    } else {
        Score::cp(-eval)
    }
}

/// Simple material evaluation (white's perspective)
fn material_eval(board: &Board) -> Value {
    let mut score: Value = 0;

    for piece in &[Piece::Pawn, Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
        let white_pieces = board.pieces(*piece) & board.color_combined(Color::White);
        let black_pieces = board.pieces(*piece) & board.color_combined(Color::Black);

        let white_count = white_pieces.popcnt() as Value;
        let black_count = black_pieces.popcnt() as Value;

        score += piece_value(*piece) * (white_count - black_count);
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_starting_position_material() {
        let board = Board::default();
        let score = material_eval_wrapper(&board);
        assert!(score.raw().abs() < 50);
    }
}
