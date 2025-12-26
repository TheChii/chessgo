//! NNUE wrapper for `nnue-rs` with incremental update support.
//!
//! Uses forked nnue-rs with exposed state for efficient incremental updates.

use crate::types::{Board, Score, ToNnue, Move};
use nnue::stockfish::halfkp::{SfHalfKpFullModel, SfHalfKpModel, SfHalfKpState};
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

/// Create a fresh NNUE state from a board position
pub fn create_state<'m>(model: &'m SfHalfKpModel, board: &Board) -> SfHalfKpState<'m> {
    let white_king = board.king_square(chess::Color::White).to_nnue();
    let black_king = board.king_square(chess::Color::Black).to_nnue();
    
    let mut state = model.new_state(white_king, black_king);

    // Add all non-king pieces using bitboard iteration
    for &piece in &[chess::Piece::Pawn, chess::Piece::Knight, chess::Piece::Bishop, 
                    chess::Piece::Rook, chess::Piece::Queen] {
        for &color in &[chess::Color::White, chess::Color::Black] {
            let bb = board.pieces(piece) & board.color_combined(color);
            let nnue_piece = piece.to_nnue();
            let nnue_color = color.to_nnue();
            
            for sq in bb {
                let nnue_sq = sq.to_nnue();
                state.add(nnue::Color::White, nnue_piece, nnue_color, nnue_sq);
                state.add(nnue::Color::Black, nnue_piece, nnue_color, nnue_sq);
            }
        }
    }
    
    state
}

/// Evaluate using a pre-built state (fast - just runs network)
#[inline]
pub fn evaluate_state(state: &mut SfHalfKpState<'_>, side_to_move: chess::Color) -> Score {
    let output = state.activate(side_to_move.to_nnue());
    let cp = nnue::stockfish::halfkp::scale_nn_to_centipawns(output[0]);
    Score::cp(cp)
}

/// Evaluate from scratch (creates new state)
#[inline]
pub fn evaluate_scratch(model: &SfHalfKpModel, board: &Board) -> Score {
    let mut state = create_state(model, board);
    evaluate_state(&mut state, board.side_to_move())
}

/// Helper: add all non-king pieces to one side of the accumulator
#[inline]
fn refresh_side_accumulator(state: &mut SfHalfKpState<'_>, board: &Board, perspective: nnue::Color) {
    // Add all non-king pieces for this perspective
    for &piece in &[chess::Piece::Pawn, chess::Piece::Knight, chess::Piece::Bishop, 
                    chess::Piece::Rook, chess::Piece::Queen] {
        for &color in &[chess::Color::White, chess::Color::Black] {
            let bb = board.pieces(piece) & board.color_combined(color);
            let nnue_piece = piece.to_nnue();
            let nnue_color = color.to_nnue();
            
            for sq in bb {
                let nnue_sq = sq.to_nnue();
                state.add(perspective, nnue_piece, nnue_color, nnue_sq);
            }
        }
    }
}

/// Update state for a move (incremental)
/// Returns true if update succeeded, false if full refresh needed
#[inline]
pub fn update_state_for_move(
    state: &mut SfHalfKpState<'_>,
    board: &Board,  // Position BEFORE the move
    mv: Move,
) -> bool {
    let from = mv.get_source();
    let to = mv.get_dest();
    let moving_piece = match board.piece_on(from) {
        Some(p) => p,
        None => return false,
    };
    let moving_color = board.side_to_move();
    let captured = board.piece_on(to);

    // === King Move Handling (Optimized) ===
    // In HalfKP, the King is NOT a feature - only non-king pieces are features.
    // The king position is used to INDEX features for other pieces.
    // Therefore:
    // - Passive side: NO UPDATE needed (enemy king is not a feature)
    // - Active side: Full refresh (all feature indices change with king position)
    if moving_piece == chess::Piece::King {
        let active = moving_color.to_nnue();
        let passive = (!moving_color).to_nnue();
        let to_sq = to.to_nnue();
        
        // Handle capture for passive side BEFORE updating king
        // (captured piece IS a feature that needs to be removed)
        if let Some(captured_piece) = captured {
            if captured_piece != chess::Piece::King {
                let cap_nnue = captured_piece.to_nnue();
                let cap_color = (!moving_color).to_nnue();
                let cap_sq = to.to_nnue();
                state.sub(passive, cap_nnue, cap_color, cap_sq);
            }
        }
        
        // Active side: update king position and clear accumulator
        state.update_king(active, to_sq);
        
        // Create a temporary board with the move applied to rebuild active side
        let new_board = board.make_move_new(mv);
        
        // Refresh the active side's accumulator with all pieces
        refresh_side_accumulator(state, &new_board, active);
        
        // Handle castling: rook also moves (rook IS a feature)
        let is_castling = (from.get_file() == chess::File::E) 
            && (to.get_file() == chess::File::G || to.get_file() == chess::File::C);
        
        if is_castling {
            // Determine rook squares based on castling type
            let nnue_rook_color = moving_color.to_nnue();
            let (rook_from, rook_to) = if to.get_file() == chess::File::G {
                // King-side castling
                let rank = from.get_rank();
                (
                    chess::Square::make_square(rank, chess::File::H),
                    chess::Square::make_square(rank, chess::File::F)
                )
            } else {
                // Queen-side castling
                let rank = from.get_rank();
                (
                    chess::Square::make_square(rank, chess::File::A),
                    chess::Square::make_square(rank, chess::File::D)
                )
            };
            
            let rook_from_nnue = rook_from.to_nnue();
            let rook_to_nnue = rook_to.to_nnue();
            
            // Update rook for passive side only (active side was already refreshed)
            state.sub(passive, nnue::Piece::Rook, nnue_rook_color, rook_from_nnue);
            state.add(passive, nnue::Piece::Rook, nnue_rook_color, rook_to_nnue);
        }
        
        return true;
    }

    // === Regular piece moves (non-king) ===
    let nnue_piece = moving_piece.to_nnue();
    let nnue_color = moving_color.to_nnue();
    let from_sq = from.to_nnue();
    let to_sq = to.to_nnue();

    // Remove piece from old square (both perspectives)
    state.sub(nnue::Color::White, nnue_piece, nnue_color, from_sq);
    state.sub(nnue::Color::Black, nnue_piece, nnue_color, from_sq);

    // Handle capture
    if let Some(captured_piece) = captured {
        if captured_piece != chess::Piece::King {
            let cap_nnue = captured_piece.to_nnue();
            let cap_color = (!moving_color).to_nnue();
            state.sub(nnue::Color::White, cap_nnue, cap_color, to_sq);
            state.sub(nnue::Color::Black, cap_nnue, cap_color, to_sq);
        }
    }

    // Handle en passant capture
    if moving_piece == chess::Piece::Pawn && board.en_passant() == Some(to) {
        // Remove en passant captured pawn
        let ep_sq = if moving_color == chess::Color::White {
            chess::Square::make_square(chess::Rank::Fifth, to.get_file()).to_nnue()
        } else {
            chess::Square::make_square(chess::Rank::Fourth, to.get_file()).to_nnue()
        };
        let cap_color = (!moving_color).to_nnue();
        state.sub(nnue::Color::White, nnue::Piece::Pawn, cap_color, ep_sq);
        state.sub(nnue::Color::Black, nnue::Piece::Pawn, cap_color, ep_sq);
    }

    // Handle promotion
    let final_piece = if let Some(promo) = mv.get_promotion() {
        promo.to_nnue()
    } else {
        nnue_piece
    };

    // Add piece to new square (both perspectives)
    state.add(nnue::Color::White, final_piece, nnue_color, to_sq);
    state.add(nnue::Color::Black, final_piece, nnue_color, to_sq);

    true
}

/// Refresh state completely from a board position
#[inline]
pub fn refresh_state<'m>(state: &mut SfHalfKpState<'m>, model: &'m SfHalfKpModel, board: &Board) {
    // Create a new state and copy it
    *state = create_state(model, board);
}

/// Stateful NNUE evaluator for use in search
/// Manages a cloneable state for efficient incremental updates
pub struct NnueEvaluator<'m> {
    model: &'m SfHalfKpModel,
    state: SfHalfKpState<'m>,
}

impl<'m> NnueEvaluator<'m> {
    /// Create a new evaluator for a position
    pub fn new(model: &'m SfHalfKpModel, board: &Board) -> Self {
        Self {
            model,
            state: create_state(model, board),
        }
    }

    /// Evaluate current position
    #[inline]
    pub fn evaluate(&mut self, side_to_move: chess::Color) -> Score {
        evaluate_state(&mut self.state, side_to_move)
    }

    /// Update for a move, returns false if refresh needed
    #[inline]
    pub fn update_move(&mut self, board: &Board, mv: Move) -> bool {
        update_state_for_move(&mut self.state, board, mv)
    }

    /// Refresh state for a new position (after king move or when needed)
    #[inline]
    pub fn refresh(&mut self, board: &Board) {
        self.state = create_state(self.model, board);
    }

    /// Clone the current state (for search recursion)
    #[inline]
    pub fn clone_state(&self) -> SfHalfKpState<'m> {
        self.state.clone()
    }

    /// Restore state from a clone
    #[inline]
    pub fn restore_state(&mut self, state: SfHalfKpState<'m>) {
        self.state = state;
    }
}

impl<'m> Clone for NnueEvaluator<'m> {
    fn clone(&self) -> Self {
        Self {
            model: self.model,
            state: self.state.clone(),
        }
    }
}
