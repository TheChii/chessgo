//! Hand-Crafted Evaluation (HCE) - Advanced classical evaluation
//!
//! Features:
//! - Tapered evaluation (midgame/endgame interpolation)
//! - Piece-Square Tables
//! - Mobility
//! - Pawn structure (doubled, isolated, passed)
//! - King safety (midgame) and centralization (endgame)
//! - Endgame-specific bonuses

use crate::types::{Board, Score};
use chess::{Color, Piece, Square, BitBoard, Rank, File, EMPTY};

// ============================================================================
// PIECE VALUES (centipawns)
// ============================================================================

const PAWN_MG: i32 = 100;
const KNIGHT_MG: i32 = 320;
const BISHOP_MG: i32 = 330;
const ROOK_MG: i32 = 500;
const QUEEN_MG: i32 = 900;

const PAWN_EG: i32 = 120;
const KNIGHT_EG: i32 = 300;
const BISHOP_EG: i32 = 320;
const ROOK_EG: i32 = 550;
const QUEEN_EG: i32 = 950;

// ============================================================================
// PIECE-SQUARE TABLES (from white's perspective, a1=0)
// ============================================================================

// Pawn PST (encourage center control and advancement)
#[rustfmt::skip]
const PAWN_PST_MG: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

#[rustfmt::skip]
const PAWN_PST_EG: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    80, 80, 80, 80, 80, 80, 80, 80,
    50, 50, 50, 50, 50, 50, 50, 50,
    30, 30, 30, 30, 30, 30, 30, 30,
    20, 20, 20, 20, 20, 20, 20, 20,
    10, 10, 10, 10, 10, 10, 10, 10,
     5,  5,  5,  5,  5,  5,  5,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

// Knight PST (encourage centralization)
#[rustfmt::skip]
const KNIGHT_PST_MG: [i32; 64] = [
   -50,-40,-30,-30,-30,-30,-40,-50,
   -40,-20,  0,  0,  0,  0,-20,-40,
   -30,  0, 10, 15, 15, 10,  0,-30,
   -30,  5, 15, 20, 20, 15,  5,-30,
   -30,  0, 15, 20, 20, 15,  0,-30,
   -30,  5, 10, 15, 15, 10,  5,-30,
   -40,-20,  0,  5,  5,  0,-20,-40,
   -50,-40,-30,-30,-30,-30,-40,-50,
];

// Bishop PST
#[rustfmt::skip]
const BISHOP_PST_MG: [i32; 64] = [
   -20,-10,-10,-10,-10,-10,-10,-20,
   -10,  0,  0,  0,  0,  0,  0,-10,
   -10,  0,  5, 10, 10,  5,  0,-10,
   -10,  5,  5, 10, 10,  5,  5,-10,
   -10,  0, 10, 10, 10, 10,  0,-10,
   -10, 10, 10, 10, 10, 10, 10,-10,
   -10,  5,  0,  0,  0,  0,  5,-10,
   -20,-10,-10,-10,-10,-10,-10,-20,
];

// Rook PST (7th rank bonus, open files)
#[rustfmt::skip]
const ROOK_PST_MG: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];

// Queen PST
#[rustfmt::skip]
const QUEEN_PST_MG: [i32; 64] = [
   -20,-10,-10, -5, -5,-10,-10,-20,
   -10,  0,  0,  0,  0,  0,  0,-10,
   -10,  0,  5,  5,  5,  5,  0,-10,
    -5,  0,  5,  5,  5,  5,  0, -5,
     0,  0,  5,  5,  5,  5,  0, -5,
   -10,  5,  5,  5,  5,  5,  0,-10,
   -10,  0,  5,  0,  0,  0,  0,-10,
   -20,-10,-10, -5, -5,-10,-10,-20,
];

// King PST - midgame (encourage castling, hide)
#[rustfmt::skip]
const KING_PST_MG: [i32; 64] = [
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -20,-30,-30,-40,-40,-30,-30,-20,
   -10,-20,-20,-20,-20,-20,-20,-10,
    20, 20,  0,  0,  0,  0, 20, 20,
    20, 30, 10,  0,  0, 10, 30, 20,
];

// King PST - endgame (encourage centralization)
#[rustfmt::skip]
const KING_PST_EG: [i32; 64] = [
   -50,-40,-30,-20,-20,-30,-40,-50,
   -30,-20,-10,  0,  0,-10,-20,-30,
   -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30,
   -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-30,  0,  0,  0,  0,-30,-30,
   -50,-30,-30,-30,-30,-30,-30,-50,
];

// ============================================================================
// BONUSES AND PENALTIES
// ============================================================================

const BISHOP_PAIR_BONUS: i32 = 30;
const DOUBLED_PAWN_PENALTY: i32 = -10;
const ISOLATED_PAWN_PENALTY: i32 = -20;
const PASSED_PAWN_BONUS: [i32; 8] = [0, 10, 20, 40, 60, 90, 130, 0]; // by rank (2-7)
const ROOK_ON_OPEN_FILE: i32 = 20;
const ROOK_ON_SEMI_OPEN: i32 = 10;
const ROOK_ON_7TH: i32 = 30;
// Reserved for future mobility evaluation
// const MOBILITY_BONUS: i32 = 3; // per legal move

// ============================================================================
// GAME PHASE
// ============================================================================

/// Calculate game phase (0 = endgame, 256 = opening)
/// Based on non-pawn material
#[inline]
fn game_phase(board: &Board) -> i32 {
    let knight_phase = 1;
    let bishop_phase = 1;
    let rook_phase = 2;
    let queen_phase = 4;
    let total_phase = 4 * knight_phase + 4 * bishop_phase + 4 * rook_phase + 2 * queen_phase;
    
    let mut phase = total_phase;
    
    phase -= (board.pieces(Piece::Knight).popcnt() as i32) * knight_phase;
    phase -= (board.pieces(Piece::Bishop).popcnt() as i32) * bishop_phase;
    phase -= (board.pieces(Piece::Rook).popcnt() as i32) * rook_phase;
    phase -= (board.pieces(Piece::Queen).popcnt() as i32) * queen_phase;
    
    // Normalize to 0-256 range
    ((phase * 256 + total_phase / 2) / total_phase).max(0).min(256)
}

/// Interpolate between midgame and endgame scores based on phase
#[inline]
fn taper(mg: i32, eg: i32, phase: i32) -> i32 {
    // phase: 0 = endgame, 256 = opening
    ((mg * (256 - phase)) + (eg * phase)) / 256
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Get PST index for a square from white's perspective
#[inline]
fn pst_index(sq: Square, color: Color) -> usize {
    let idx = sq.to_index();
    if color == Color::White {
        // Flip for white (rank 1 -> rank 8)
        idx ^ 56
    } else {
        idx
    }
}

/// Get bitboard for a file
#[inline]
const fn get_file_bb(file: File) -> u64 {
    0x0101010101010101u64 << (file as u8)
}

/// Count pawns on a file for a color
#[inline]
fn pawns_on_file(board: &Board, color: Color, file: File) -> u32 {
    let file_bb = BitBoard::new(get_file_bb(file));
    (board.pieces(Piece::Pawn) & board.color_combined(color) & file_bb).popcnt()
}

/// Check if a file is open (no pawns)
#[inline]
fn is_open_file(board: &Board, file: File) -> bool {
    let file_bb = BitBoard::new(get_file_bb(file));
    (board.pieces(Piece::Pawn) & file_bb) == EMPTY
}

/// Check if a file is semi-open for a color (no friendly pawns)
#[inline]
fn is_semi_open_file(board: &Board, color: Color, file: File) -> bool {
    let file_bb = BitBoard::new(get_file_bb(file));
    (board.pieces(Piece::Pawn) & board.color_combined(color) & file_bb) == EMPTY
}

/// Check if a pawn is passed
#[inline]
fn is_passed_pawn(board: &Board, sq: Square, color: Color) -> bool {
    let file = sq.get_file();
    let rank = sq.get_rank();
    let enemy = !color;
    
    // All files as array for lookup
    const FILES: [File; 8] = [File::A, File::B, File::C, File::D, File::E, File::F, File::G, File::H];
    let file_idx = file.to_index();
    
    // Get adjacent files + same file
    let mut check_mask = 0u64;
    if file_idx > 0 {
        check_mask |= get_file_bb(FILES[file_idx - 1]);
    }
    check_mask |= get_file_bb(file);
    if file_idx < 7 {
        check_mask |= get_file_bb(FILES[file_idx + 1]);
    }
    let check_files = BitBoard::new(check_mask);
    
    // Get ranks in front of pawn
    let front_ranks: BitBoard = if color == Color::White {
        // Ranks above this pawn
        BitBoard::new(!((1u64 << ((rank.to_index() as u8 + 1) * 8)) - 1))
    } else {
        // Ranks below this pawn
        BitBoard::new((1u64 << (rank.to_index() as u8 * 8)) - 1)
    };
    
    let blocking_area = check_files & front_ranks;
    (board.pieces(Piece::Pawn) & board.color_combined(enemy) & blocking_area) == EMPTY
}

/// Check if a pawn is isolated (no friendly pawns on adjacent files)
#[inline]
fn is_isolated_pawn(board: &Board, sq: Square, color: Color) -> bool {
    let file = sq.get_file();
    const FILES: [File; 8] = [File::A, File::B, File::C, File::D, File::E, File::F, File::G, File::H];
    let file_idx = file.to_index();
    
    let mut adj_mask = 0u64;
    if file_idx > 0 {
        adj_mask |= get_file_bb(FILES[file_idx - 1]);
    }
    if file_idx < 7 {
        adj_mask |= get_file_bb(FILES[file_idx + 1]);
    }
    let adj_files = BitBoard::new(adj_mask);
    
    (board.pieces(Piece::Pawn) & board.color_combined(color) & adj_files) == EMPTY
}

// ============================================================================
// MAIN EVALUATION FUNCTION
// ============================================================================

/// Evaluate the position from white's perspective
pub fn evaluate(board: &Board) -> Score {
    let phase = game_phase(board);
    let mut mg_score: i32 = 0;
    let mut eg_score: i32 = 0;
    
    // Evaluate each color
    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        
        // === MATERIAL AND PST ===
        
        // Pawns
        for sq in board.pieces(Piece::Pawn) & board.color_combined(color) {
            mg_score += sign * PAWN_MG;
            eg_score += sign * PAWN_EG;
            let idx = pst_index(sq, color);
            mg_score += sign * PAWN_PST_MG[idx];
            eg_score += sign * PAWN_PST_EG[idx];
            
            // Pawn structure
            let file = sq.get_file();
            
            // Doubled pawns
            if pawns_on_file(board, color, file) > 1 {
                mg_score += sign * DOUBLED_PAWN_PENALTY;
                eg_score += sign * DOUBLED_PAWN_PENALTY;
            }
            
            // Isolated pawns
            if is_isolated_pawn(board, sq, color) {
                mg_score += sign * ISOLATED_PAWN_PENALTY;
                eg_score += sign * ISOLATED_PAWN_PENALTY;
            }
            
            // Passed pawns
            if is_passed_pawn(board, sq, color) {
                let rank = sq.get_rank();
                let rank_idx = if color == Color::White {
                    rank as usize
                } else {
                    7 - rank as usize
                };
                let bonus = PASSED_PAWN_BONUS[rank_idx.min(7)];
                mg_score += sign * bonus / 2; // Half in midgame
                eg_score += sign * bonus;     // Full in endgame
            }
        }
        
        // Knights
        for sq in board.pieces(Piece::Knight) & board.color_combined(color) {
            mg_score += sign * KNIGHT_MG;
            eg_score += sign * KNIGHT_EG;
            let idx = pst_index(sq, color);
            mg_score += sign * KNIGHT_PST_MG[idx];
            eg_score += sign * KNIGHT_PST_MG[idx]; // Same for EG
        }
        
        // Bishops
        let bishops = board.pieces(Piece::Bishop) & board.color_combined(color);
        for sq in bishops {
            mg_score += sign * BISHOP_MG;
            eg_score += sign * BISHOP_EG;
            let idx = pst_index(sq, color);
            mg_score += sign * BISHOP_PST_MG[idx];
            eg_score += sign * BISHOP_PST_MG[idx];
        }
        // Bishop pair
        if bishops.popcnt() >= 2 {
            mg_score += sign * BISHOP_PAIR_BONUS;
            eg_score += sign * BISHOP_PAIR_BONUS;
        }
        
        // Rooks
        for sq in board.pieces(Piece::Rook) & board.color_combined(color) {
            mg_score += sign * ROOK_MG;
            eg_score += sign * ROOK_EG;
            let idx = pst_index(sq, color);
            mg_score += sign * ROOK_PST_MG[idx];
            eg_score += sign * ROOK_PST_MG[idx];
            
            let file = sq.get_file();
            let rank = sq.get_rank();
            
            // Open/semi-open file bonus
            if is_open_file(board, file) {
                mg_score += sign * ROOK_ON_OPEN_FILE;
                eg_score += sign * ROOK_ON_OPEN_FILE;
            } else if is_semi_open_file(board, color, file) {
                mg_score += sign * ROOK_ON_SEMI_OPEN;
                eg_score += sign * ROOK_ON_SEMI_OPEN;
            }
            
            // Rook on 7th rank
            let seventh = if color == Color::White { Rank::Seventh } else { Rank::Second };
            if rank == seventh {
                mg_score += sign * ROOK_ON_7TH;
                eg_score += sign * ROOK_ON_7TH;
            }
        }
        
        // Queens
        for sq in board.pieces(Piece::Queen) & board.color_combined(color) {
            mg_score += sign * QUEEN_MG;
            eg_score += sign * QUEEN_EG;
            let idx = pst_index(sq, color);
            mg_score += sign * QUEEN_PST_MG[idx];
            eg_score += sign * QUEEN_PST_MG[idx];
        }
        
        // King
        let king_sq = board.king_square(color);
        let idx = pst_index(king_sq, color);
        mg_score += sign * KING_PST_MG[idx];
        eg_score += sign * KING_PST_EG[idx];
    }
    
    // Taper the score
    let final_score = taper(eg_score, mg_score, phase);
    
    // Return from side-to-move perspective
    if board.side_to_move() == Color::White {
        Score::cp(final_score)
    } else {
        Score::cp(-final_score)
    }
}

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
        // White up a queen
        let board = Board::from_str("rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let score = evaluate(&board);
        // White should have big advantage
        assert!(score.raw() > 800);
    }
}
