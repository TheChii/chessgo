//! Static Exchange Evaluation (SEE)
//!
//! Determines if a capture sequence is winning, losing, or neutral.
//! Uses fixed-size arrays to avoid allocations.

use crate::types::{Board, Move, piece_value};
use chess::{BitBoard, Piece, Color, Square, EMPTY};

/// Piece values for SEE (using lower values for faster cutoffs)
const SEE_VALUES: [i32; 6] = [100, 300, 300, 500, 900, 20000]; // P, N, B, R, Q, K

/// Get SEE value for a piece
#[inline]
fn see_piece_value(piece: Piece) -> i32 {
    SEE_VALUES[piece.to_index()]
}

/// Get least valuable attacker of a square
#[inline]
fn get_lva(board: &Board, sq: Square, side: Color, occupied: BitBoard) -> Option<(Square, Piece)> {
    // Check each piece type from least to most valuable
    for piece in [Piece::Pawn, Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen, Piece::King] {
        let attackers = get_piece_attacks(board, sq, piece, side, occupied);
        if attackers != EMPTY {
            return Some((attackers.to_square(), piece));
        }
    }
    None
}

/// Get attacks from a specific piece type to a square
#[inline]
fn get_piece_attacks(board: &Board, target: Square, piece: Piece, side: Color, occupied: BitBoard) -> BitBoard {
    let our_pieces = board.pieces(piece) & board.color_combined(side) & occupied;
    
    match piece {
        Piece::Pawn => {
            let pawn_attacks = chess::get_pawn_attacks(target, !side, EMPTY);
            our_pieces & pawn_attacks
        }
        Piece::Knight => {
            our_pieces & chess::get_knight_moves(target)
        }
        Piece::Bishop => {
            our_pieces & chess::get_bishop_moves(target, occupied)
        }
        Piece::Rook => {
            our_pieces & chess::get_rook_moves(target, occupied)
        }
        Piece::Queen => {
            our_pieces & (chess::get_bishop_moves(target, occupied) | chess::get_rook_moves(target, occupied))
        }
        Piece::King => {
            our_pieces & chess::get_king_moves(target)
        }
    }
}

/// Static Exchange Evaluation
/// Returns the material balance after a capture sequence.
/// Uses fixed-size array to avoid allocations.
#[inline]
pub fn see(board: &Board, mv: Move) -> i32 {
    let from = mv.get_source();
    let to = mv.get_dest();
    
    // Get initial capture value
    let victim = board.piece_on(to);
    let attacker = board.piece_on(from);
    
    let (attacker_piece, mut gain) = match (attacker, victim) {
        (Some(a), Some(v)) => (a, see_piece_value(v)),
        (Some(a), None) => {
            // En passant capture
            if a == Piece::Pawn {
                (a, see_piece_value(Piece::Pawn))
            } else {
                return 0; // Not a capture
            }
        }
        _ => return 0,
    };

    // Handle promotion
    if let Some(promo) = mv.get_promotion() {
        gain += see_piece_value(promo) - see_piece_value(Piece::Pawn);
    }

    // Fixed-size gains stack (max 32 captures possible)
    let mut gains: [i32; 32] = [0; 32];
    let mut depth = 0;
    gains[depth] = gain;
    depth += 1;

    let mut occupied = *board.combined() ^ BitBoard::from_square(from);
    let mut side = !board.side_to_move();
    let mut last_value = see_piece_value(attacker_piece);
    
    // Simulate the exchange
    loop {
        if let Some((sq, piece)) = get_lva(board, to, side, occupied) {
            occupied ^= BitBoard::from_square(sq);
            gains[depth] = last_value;
            last_value = see_piece_value(piece);
            depth += 1;
            side = !side;
            
            // King capture ends the sequence
            if piece == Piece::King {
                break;
            }
        } else {
            break;
        }
    }
    
    // Negamax-style evaluation from the end
    while depth > 1 {
        depth -= 1;
        gains[depth - 1] = gains[depth - 1].max(-gains[depth]);
    }
    
    gains[0]
}

/// Check if SEE is greater than or equal to threshold
#[inline]
pub fn see_ge(board: &Board, mv: Move, threshold: i32) -> bool {
    see(board, mv) >= threshold
}

/// Check if a capture is winning (SEE >= 0)
#[inline]
pub fn is_good_capture(board: &Board, mv: Move) -> bool {
    see_ge(board, mv, 0)
}
