//! NNUE wrapper for `nnue-rs`.
//!
//! Provides a clean interface to evaluate `chess::Board` positions using `nnue-rs`.

use crate::types::{Board, Score, ToNnue};
use nnue::stockfish::halfkp::{SfHalfKpFullModel, SfHalfKpModel};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use binread::BinRead;

/// Global type for shared thread-safe model
pub type Model = Arc<SfHalfKpFullModel>;

/// Load NNUE model from file
pub fn load_model(path: &str) -> std::io::Result<Model> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    match SfHalfKpFullModel::read(&mut reader) {
        Ok(model) => Ok(Arc::new(model)),
        Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
    }
}

/// Evaluate a board using the NNUE model.
///
/// Note: This performs a fresh evaluation from scratch (no incremental updates).
/// It is slower but simpler and less bug-prone.
pub fn evaluate(model: &SfHalfKpModel, board: &Board) -> Score {
    let side_to_move = board.side_to_move().to_nnue();
    
    // Create new state (scratch calculation)
    // We need to find the king squares first
    let white_king = board.king_square(chess::Color::White).to_nnue();
    let black_king = board.king_square(chess::Color::Black).to_nnue();
    
    let mut state = model.new_state(white_king, black_king);

    // Add all pieces to the accumulator
    // Iterate over all squares with pieces
    for sq in chess::ALL_SQUARES {
        if let Some(piece) = board.piece_on(sq) {
            let color = board.color_on(sq).unwrap();
            
            // Skip kings (handled by new_state)
            if piece == chess::Piece::King {
                continue;
            }

            let nnue_piece = piece.to_nnue();
            let nnue_color = color.to_nnue();
            let nnue_sq = sq.to_nnue();

            // Add piece for both perspectives
            // White perspective
            state.add(nnue::Color::White, nnue_piece, nnue_color, nnue_sq);
            
            // Black perspective
            state.add(nnue::Color::Black, nnue_piece, nnue_color, nnue_sq);
        }
    }

    // Run inference
    let output = state.activate(side_to_move);
    
    // Scale to centipawns
    // Output[0] is the evaluation relative to side_to_move
    let cp = nnue::stockfish::halfkp::scale_nn_to_centipawns(output[0]);

    Score::cp(cp)
}
