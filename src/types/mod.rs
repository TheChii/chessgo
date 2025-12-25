//! Core types for the chess engine.
//!
//! This module provides unified types that integrate seamlessly with:
//! - `chess` crate (move generation)
//! - `nnue` crate (evaluation)
//!
//! # Design Principles
//! - Re-export chess crate types as the canonical source for board/move types
//! - Provide conversion traits to bridge with nnue types
//! - Define engine-specific types (Score, Depth, etc.) optimized for search

mod score;
mod depth;
mod convert;

// Re-export our custom types
pub use score::{Score, SCORE_INFINITY, SCORE_MATE, SCORE_DRAW, SCORE_NONE};
pub use depth::{Depth, Ply, MAX_DEPTH, MAX_PLY};
pub use convert::ToNnue;

// Re-export chess crate types as canonical types
// This gives us a single source of truth and avoids confusion
pub use chess::{
    Board,
    ChessMove as Move,
    Square,
    Piece,
    Color,
    BitBoard,
    File,
    Rank,
    CastleRights,
    MoveGen,
    BoardStatus,
    ALL_SQUARES,
    EMPTY,
};

/// Type alias for move list (stack-allocated for speed)
pub type MoveList = chess::MoveGen;

/// Zobrist hash type (used for transposition table)
pub type Hash = u64;

/// Node count type
pub type NodeCount = u64;

/// Centipawn value type (for piece values, etc.)
pub type Value = i32;

// Piece values in centipawns (standard values)
pub const PAWN_VALUE: Value = 100;
pub const KNIGHT_VALUE: Value = 320;
pub const BISHOP_VALUE: Value = 330;
pub const ROOK_VALUE: Value = 500;
pub const QUEEN_VALUE: Value = 900;
pub const KING_VALUE: Value = 20000; // Arbitrary large value

/// Get the material value of a piece in centipawns
#[inline]
pub const fn piece_value(piece: Piece) -> Value {
    match piece {
        Piece::Pawn => PAWN_VALUE,
        Piece::Knight => KNIGHT_VALUE,
        Piece::Bishop => BISHOP_VALUE,
        Piece::Rook => ROOK_VALUE,
        Piece::Queen => QUEEN_VALUE,
        Piece::King => KING_VALUE,
    }
}
