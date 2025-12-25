//! Board evaluation module.
//!
//! Provides static evaluation of chess positions.
//! Currently uses simple material counting.
//! Designed for easy extension to NNUE evaluation.

use crate::types::{Board, Score, Color, Piece, piece_value, Value};

/// Evaluate the position from the side-to-move's perspective.
///
/// Returns a score in centipawns.
/// Positive = good for side to move, negative = bad.
pub fn evaluate(board: &Board) -> Score {
    let eval = material_eval(board);
    
    // Convert to side-to-move perspective
    let score = if board.side_to_move() == Color::White {
        eval
    } else {
        -eval
    };

    Score::cp(score)
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

// === Future: NNUE Evaluation ===
// pub struct NnueEvaluator {
//     model: nnue::stockfish::halfkp::SfHalfKpModel,
//     state: Option<nnue::stockfish::halfkp::SfHalfKpState>,
// }
//
// impl NnueEvaluator {
//     pub fn evaluate(&mut self, board: &Board) -> Score {
//         // Build NNUE state from board
//         // Call activate()
//         // Scale output to centipawns
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_starting_position() {
        let board = Board::default();
        let score = evaluate(&board);
        // Starting position should be roughly equal
        assert!(score.raw().abs() < 50);
    }

    #[test]
    fn test_material_advantage() {
        // Position where white is up a queen
        let board = Board::from_str("rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let score = evaluate(&board);
        // White should be significantly ahead
        assert!(score.raw() > 800);
    }
}
