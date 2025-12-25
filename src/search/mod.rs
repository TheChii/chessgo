//! Search module for the chess engine.
//!
//! This module implements the search algorithm with a scalable architecture.
//!
//! # Architecture
//! - `Searcher`: Main search controller with state management
//! - `negamax`: Alpha-beta search with negamax framework
//! - `ordering`: Move ordering heuristics (MVV-LVA, killer moves, history)
//! - `limits`: Search limits and time management
//!
//! # Future Extensions
//! The architecture supports adding:
//! - Transposition table
//! - Null move pruning
//! - Late move reductions (LMR)
//! - Aspiration windows
//! - Principal variation search (PVS)
//! - Multi-threaded search (Lazy SMP)

mod negamax;
mod ordering;
mod limits;

pub use limits::{SearchLimits, TimeManager};
pub use negamax::SearchResult;

use crate::types::{Board, Move, Score, Depth, Ply, NodeCount};
use std::time::Instant;

/// Search statistics collected during search
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    pub nodes: NodeCount,
    pub depth: Depth,
    pub seldepth: Ply,
    pub time_ms: u64,
}

impl SearchStats {
    pub fn nps(&self) -> u64 {
        if self.time_ms > 0 {
            self.nodes * 1000 / self.time_ms
        } else {
            0
        }
    }
}

/// Main search controller
pub struct Searcher {
    /// Current board position
    board: Board,
    /// Time manager for search limits
    time_manager: TimeManager,
    /// Search statistics
    stats: SearchStats,
    /// Best move found so far
    best_move: Option<Move>,
    /// Principal variation
    pv: Vec<Move>,
    /// Should stop searching
    stop: bool,
    /// Start time of search
    start_time: Option<Instant>,
    // === Future extensibility ===
    // pub tt: TranspositionTable,
    // pub history: HistoryTable,
    // pub killers: KillerTable,
}

impl Searcher {
    pub fn new() -> Self {
        Self {
            board: Board::default(),
            time_manager: TimeManager::new(),
            stats: SearchStats::default(),
            best_move: None,
            pv: Vec::new(),
            stop: false,
            start_time: None,
        }
    }

    /// Set the position to search
    pub fn set_position(&mut self, board: Board) {
        self.board = board;
    }

    /// Get current statistics
    pub fn stats(&self) -> &SearchStats {
        &self.stats
    }

    /// Get best move found
    pub fn best_move(&self) -> Option<Move> {
        self.best_move
    }

    /// Get principal variation
    pub fn pv(&self) -> &[Move] {
        &self.pv
    }

    /// Signal the search to stop
    pub fn stop(&mut self) {
        self.stop = true;
    }

    /// Check if search should stop (time limit, nodes limit, etc.)
    pub fn should_stop(&self) -> bool {
        if self.stop {
            return true;
        }
        
        // Check time periodically (every 2048 nodes for efficiency)
        if self.stats.nodes & 2047 == 0 {
            if let Some(start) = self.start_time {
                let elapsed = start.elapsed().as_millis() as u64;
                if self.time_manager.should_stop(elapsed) {
                    return true;
                }
            }
        }
        
        false
    }

    /// Run the search with given limits
    pub fn search(&mut self, limits: SearchLimits) -> SearchResult {
        self.stop = false;
        self.stats = SearchStats::default();
        self.best_move = None;
        self.pv.clear();
        self.start_time = Some(Instant::now());
        
        // Configure time management
        self.time_manager = TimeManager::from_limits(&limits, self.board.side_to_move());
        
        let max_depth = limits.depth.unwrap_or(Depth::MAX);
        
        // Iterative deepening
        let mut best_score = Score::neg_infinity();
        
        for depth in 1..=max_depth.raw() {
            if self.should_stop() {
                break;
            }

            let result = negamax::search(
                self,
                &self.board.clone(),
                Depth::new(depth),
                Ply::ZERO,
                Score::neg_infinity(),
                Score::infinity(),
            );

            // Only update best move if search completed this depth
            if !self.should_stop() || self.best_move.is_none() {
                if let Some(m) = result.best_move {
                    self.best_move = Some(m);
                    best_score = result.score;
                    self.pv = result.pv.clone();
                }
            }

            self.stats.depth = Depth::new(depth);
            
            // Update time
            if let Some(start) = self.start_time {
                self.stats.time_ms = start.elapsed().as_millis() as u64;
            }

            // Report info (callback could be added here)
            // For now, the UCI handler will query stats after search
        }

        SearchResult {
            best_move: self.best_move,
            score: best_score,
            pv: self.pv.clone(),
            stats: self.stats.clone(),
        }
    }

    /// Increment node counter
    #[inline]
    pub fn inc_nodes(&mut self) {
        self.stats.nodes += 1;
    }

    /// Update selective depth
    #[inline]
    pub fn update_seldepth(&mut self, ply: Ply) {
        if ply.raw() > self.stats.seldepth.raw() {
            self.stats.seldepth = ply;
        }
    }
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}
