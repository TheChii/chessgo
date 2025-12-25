//! Static Exchange Evaluation (SEE)
//!
//! Determines if a capture sequence is winning, losing, or neutral.
//! Used for move ordering and pruning bad captures.

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
fn get_lva(board: &Board, sq: Square, side: Color, occupied: BitBoard) -> Option<(Square, Piece)> {
    // Check each piece type from least to most valuable
    for &piece in &[Piece::Pawn, Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen, Piece::King] {
        let attackers = get_piece_attacks(board, sq, piece, side, occupied);
        if attackers != EMPTY {
            // Get any attacker square
            let attacker_sq = attackers.to_square();
            return Some((attacker_sq, piece));
        }
    }
    None
}

/// Get attacks from a specific piece type to a square
fn get_piece_attacks(board: &Board, target: Square, piece: Piece, side: Color, occupied: BitBoard) -> BitBoard {
    let our_pieces = board.pieces(piece) & board.color_combined(side) & occupied;
    
    match piece {
        Piece::Pawn => {
            // Pawn attacks the target
            let pawn_attacks = chess::get_pawn_attacks(target, !side, EMPTY);
            our_pieces & pawn_attacks
        }
        Piece::Knight => {
            let knight_attacks = chess::get_knight_moves(target);
            our_pieces & knight_attacks
        }
        Piece::Bishop => {
            let bishop_attacks = chess::get_bishop_moves(target, occupied);
            our_pieces & bishop_attacks
        }
        Piece::Rook => {
            let rook_attacks = chess::get_rook_moves(target, occupied);
            our_pieces & rook_attacks
        }
        Piece::Queen => {
            let queen_attacks = chess::get_bishop_moves(target, occupied) | chess::get_rook_moves(target, occupied);
            our_pieces & queen_attacks
        }
        Piece::King => {
            let king_attacks = chess::get_king_moves(target);
            our_pieces & king_attacks
        }
    }
}

/// Static Exchange Evaluation
/// Returns the material balance after a capture sequence
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

    let mut balance = gain;
    let mut occupied = *board.combined() ^ BitBoard::from_square(from);
    let mut side = !board.side_to_move();
    let mut last_value = see_piece_value(attacker_piece);
    
    // Stack for negamax-style evaluation
    let mut gains = vec![gain];
    
    // Simulate the exchange
    loop {
        // Find least valuable attacker
        if let Some((sq, piece)) = get_lva(board, to, side, occupied) {
            // Remove attacker from occupied
            occupied ^= BitBoard::from_square(sq);
            
            // Value we can capture
            gains.push(last_value);
            last_value = see_piece_value(piece);
            
            side = !side;
            
            // King capture ends the sequence (illegal for opponent to recapture)
            if piece == Piece::King {
                break;
            }
        } else {
            break;
        }
    }
    
    // Evaluate from the end (like negamax)
    while gains.len() > 1 {
        let their_gain = gains.pop().unwrap();
        let our_score = gains.last_mut().unwrap();
        *our_score = (*our_score).max(-their_gain);
    }
    
    gains.pop().unwrap_or(0)
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
