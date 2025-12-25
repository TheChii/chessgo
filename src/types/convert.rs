//! Conversion traits between `chess` crate and `nnue` crate types.
//!
//! The `chess` crate and `nnue` crate have their own Square, Piece, and Color types.
//! This module provides zero-cost conversions between them.

use chess::{Square as ChessSquare, Piece as ChessPiece, Color as ChessColor};
use nnue::{Square as NnueSquare, Piece as NnuePiece, Color as NnueColor};

/// Trait for converting chess crate types to nnue crate types.
///
/// Implementations are `#[inline]` for zero-cost abstraction.
pub trait ToNnue {
    type Output;
    fn to_nnue(self) -> Self::Output;
}

impl ToNnue for ChessSquare {
    type Output = NnueSquare;

    #[inline]
    fn to_nnue(self) -> NnueSquare {
        // Both crates use A1=0, H8=63 ordering
        NnueSquare::from_index(self.to_index())
    }
}

impl ToNnue for ChessPiece {
    type Output = NnuePiece;

    #[inline]
    fn to_nnue(self) -> NnuePiece {
        // Piece ordering: Pawn, Knight, Bishop, Rook, Queen, King
        match self {
            ChessPiece::Pawn => NnuePiece::Pawn,
            ChessPiece::Knight => NnuePiece::Knight,
            ChessPiece::Bishop => NnuePiece::Bishop,
            ChessPiece::Rook => NnuePiece::Rook,
            ChessPiece::Queen => NnuePiece::Queen,
            ChessPiece::King => NnuePiece::King,
        }
    }
}

impl ToNnue for ChessColor {
    type Output = NnueColor;

    #[inline]
    fn to_nnue(self) -> NnueColor {
        match self {
            ChessColor::White => NnueColor::White,
            ChessColor::Black => NnueColor::Black,
        }
    }
}

/// Helper to get the opposite color in nnue terms
#[inline]
#[allow(dead_code)]
pub fn nnue_color_flip(c: NnueColor) -> NnueColor {
    match c {
        NnueColor::White => NnueColor::Black,
        NnueColor::Black => NnueColor::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_square_conversion() {
        // Test a few key squares
        assert_eq!(ChessSquare::A1.to_nnue(), NnueSquare::A1);
        assert_eq!(ChessSquare::E4.to_nnue(), NnueSquare::E4);
        assert_eq!(ChessSquare::H8.to_nnue(), NnueSquare::H8);
    }

    #[test]
    fn test_piece_conversion() {
        assert_eq!(ChessPiece::Pawn.to_nnue(), NnuePiece::Pawn);
        assert_eq!(ChessPiece::King.to_nnue(), NnuePiece::King);
    }

    #[test]
    fn test_color_conversion() {
        assert_eq!(ChessColor::White.to_nnue(), NnueColor::White);
        assert_eq!(ChessColor::Black.to_nnue(), NnueColor::Black);
    }
}
