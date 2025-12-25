//! UCI (Universal Chess Interface) protocol handler.
//!
//! This module implements the UCI protocol for communication with chess GUIs.
//! See: http://wbec-ridderkerk.nl/html/UCIProtocol.html

mod parser;
mod handler;

pub use handler::UciHandler;

use crate::types::{Board, Move, Depth};
use std::str::FromStr;

/// UCI engine identification
pub const ENGINE_NAME: &str = "ChessInRust";
pub const ENGINE_AUTHOR: &str = "Anonymous";

/// Time control parameters from "go" command
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    /// Search to this depth
    pub depth: Option<Depth>,
    /// Search for this many milliseconds
    pub movetime: Option<u64>,
    /// White time remaining (ms)
    pub wtime: Option<u64>,
    /// Black time remaining (ms)
    pub btime: Option<u64>,
    /// White increment per move (ms)
    pub winc: Option<u64>,
    /// Black increment per move (ms)
    pub binc: Option<u64>,
    /// Moves until next time control
    pub movestogo: Option<u32>,
    /// Infinite search (until "stop")
    pub infinite: bool,
    /// Ponder mode
    pub ponder: bool,
    /// Only search these moves
    pub searchmoves: Vec<Move>,
    /// Search for mate in N moves
    pub mate: Option<u32>,
    /// Maximum nodes to search
    pub nodes: Option<u64>,
}

impl SearchParams {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create params for a fixed depth search
    pub fn fixed_depth(depth: i32) -> Self {
        Self {
            depth: Some(Depth::new(depth)),
            ..Default::default()
        }
    }

    /// Create params for a fixed time search
    pub fn fixed_time(ms: u64) -> Self {
        Self {
            movetime: Some(ms),
            ..Default::default()
        }
    }
}

/// Parse a move string (e.g., "e2e4", "e7e8q") into a Move for the given board
pub fn parse_move(board: &Board, move_str: &str) -> Option<Move> {
    use crate::types::MoveGen;
    
    let move_str = move_str.trim();
    if move_str.len() < 4 {
        return None;
    }

    // Parse source and destination squares
    let from_str = &move_str[0..2];
    let to_str = &move_str[2..4];
    
    let from = chess::Square::from_str(from_str).ok()?;
    let to = chess::Square::from_str(to_str).ok()?;
    
    // Parse promotion piece if present
    let promo = if move_str.len() > 4 {
        match move_str.chars().nth(4)? {
            'q' | 'Q' => Some(chess::Piece::Queen),
            'r' | 'R' => Some(chess::Piece::Rook),
            'b' | 'B' => Some(chess::Piece::Bishop),
            'n' | 'N' => Some(chess::Piece::Knight),
            _ => None,
        }
    } else {
        None
    };

    // Find the matching legal move
    let movegen = MoveGen::new_legal(board);
    for m in movegen {
        if m.get_source() == from && m.get_dest() == to {
            // For promotions, also check the promotion piece
            if let Some(p) = promo {
                if m.get_promotion() == Some(p) {
                    return Some(m);
                }
            } else if m.get_promotion().is_none() {
                return Some(m);
            }
        }
    }

    None
}

/// Format a move to UCI notation (e.g., "e2e4", "e7e8q")
pub fn format_move(m: Move) -> String {
    let mut s = format!("{}{}", m.get_source(), m.get_dest());
    if let Some(promo) = m.get_promotion() {
        let c = match promo {
            chess::Piece::Queen => 'q',
            chess::Piece::Rook => 'r',
            chess::Piece::Bishop => 'b',
            chess::Piece::Knight => 'n',
            _ => unreachable!(),
        };
        s.push(c);
    }
    s
}
