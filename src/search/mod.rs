//! Search module for the chess engine.
//!
//! This module implements the search algorithm with a scalable architecture.
//!
//! # Architecture
//! - `Searcher`: Main search controller with state management
//! - `negamax`: Alpha-beta search with negamax framework
//! - `ordering`: Move ordering heuristics (MVV-LVA, killer moves, history)
//! - `limits`: Search limits and time management
//! - `tt`: Transposition table for caching search results
//!
//! # Future Extensions
//! The architecture supports adding:
//! - Null move pruning
//! - Late move reductions (LMR)
//! - Aspiration windows
//! - Principal variation search (PVS)
//! - Multi-threaded search (Lazy SMP)

mod negamax;
mod ordering;
mod limits;
pub mod tt;
mod killers;
mod history;

pub use limits::{SearchLimits, TimeManager};
pub use negamax::SearchResult;
pub use tt::TranspositionTable;
pub use killers::KillerTable;
pub use history::HistoryTable;

use crate::types::{Board, Move, Score, Depth, Ply, NodeCount};
use crate::eval::nnue;
use std::time::Instant;

/// Search statistics collected during search
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    pub nodes: NodeCount,
    pub depth: Depth,
    pub seldepth: Ply,
    pub time_ms: u64,
    pub hashfull: u32,
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
    /// Transposition table
    pub tt: TranspositionTable,
    /// Killer moves table
    pub killers: KillerTable,
    /// History heuristic table
    pub history: HistoryTable,
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
    /// NNUE Model (thread-safe reference)
    pub nnue: Option<nnue::Model>,
}

impl Searcher {
    pub fn new() -> Self {
        Self {
            board: Board::default(),
            tt: TranspositionTable::default(),
            killers: KillerTable::new(),
            history: HistoryTable::new(),
            time_manager: TimeManager::new(),
            stats: SearchStats::default(),
            best_move: None,
            pv: Vec::new(),
            stop: false,
            start_time: None,
            nnue: None,
        }
    }

    /// Create with specific TT size
    pub fn with_hash_size(size_mb: usize) -> Self {
        let mut s = Self::new();
        s.tt = TranspositionTable::new(size_mb);
        s
    }

    /// Set NNUE model
    pub fn set_nnue(&mut self, model: Option<nnue::Model>) {
        self.nnue = model;
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
        
        // Increment TT generation for new search
        self.tt.new_search();
        
        // Clear killer moves for new search
        self.killers.clear();
        
        // Age history scores (decay old data, keep some history)
        self.history.age();
        
        // Configure time management
        self.time_manager = TimeManager::from_limits(&limits, self.board.side_to_move());
        
        let max_depth = limits.depth.unwrap_or(Depth::MAX);
        
        // Iterative deepening with aspiration windows
        let mut best_score = Score::neg_infinity();
        const INITIAL_WINDOW: i32 = 25;
        
        for depth in 1..=max_depth.raw() {
            if self.should_stop() {
                break;
            }

            // Aspiration window: use previous score +/- delta after depth 1
            let mut delta = INITIAL_WINDOW;
            let mut alpha = if depth > 1 && !best_score.is_mate() { 
                best_score - Score::cp(delta) 
            } else { 
                Score::neg_infinity() 
            };
            let mut beta = if depth > 1 && !best_score.is_mate() { 
                best_score + Score::cp(delta) 
            } else { 
                Score::infinity() 
            };

            // Aspiration loop: widen window on fail-high/low
            loop {
                let result = negamax::search(
                    self,
                    &self.board.clone(),
                    Depth::new(depth),
                    Ply::ZERO,
                    alpha,
                    beta,
                    true,
                );

                if self.should_stop() {
                    break;
                }

                // Check if score is within window
                if result.score <= alpha {
                    // Fail-low: widen alpha
                    alpha = Score::neg_infinity();
                } else if result.score >= beta {
                    // Fail-high: widen beta
                    beta = Score::infinity();
                } else {
                    // Score within window, accept result
                    if let Some(m) = result.best_move {
                        self.best_move = Some(m);
                        best_score = result.score;
                        self.pv = result.pv.clone();
                    }
                    break;
                }

                // Widen window for next attempt
                delta *= 2;
                if delta > 500 {
                    // Window too wide, use full window
                    alpha = Score::neg_infinity();
                    beta = Score::infinity();
                }
            }

            self.stats.depth = Depth::new(depth);
            self.stats.hashfull = self.tt.hashfull();
            
            // Update time
            if let Some(start) = self.start_time {
                self.stats.time_ms = start.elapsed().as_millis() as u64;
            }

            // Print info for this depth
            if !self.should_stop() {
                let pv_str: String = self.pv.iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                    
                println!(
                    "info depth {} seldepth {} score {} nodes {} nps {} time {} hashfull {} pv {}",
                    depth,
                    self.stats.seldepth.raw(),
                    best_score,
                    self.stats.nodes,
                    self.stats.nps(),
                    self.stats.time_ms,
                    self.stats.hashfull,
                    pv_str
                );
            }
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
