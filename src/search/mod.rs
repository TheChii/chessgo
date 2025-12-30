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
//! # Multi-threading
//! Implements Lazy SMP with lock-free TT sharing between threads

mod negamax;
mod qsearch;
mod ordering;
mod limits;
pub mod tt;
mod killers;
mod history;
mod see;
mod countermove;
pub mod node_types;

pub use node_types::{NodeType, Root, OnPV, OffPV};

pub use limits::{SearchLimits, TimeManager};
pub use negamax::SearchResult;
pub use tt::TranspositionTable;
pub use killers::KillerTable;
pub use history::HistoryTable;
pub use countermove::CounterMoveTable;
pub use see::{see, see_ge, is_good_capture};

use crate::types::{Board, Move, Score, Depth, Ply, NodeCount};
use crate::eval::{nnue, SearchEvaluator};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

/// Search statistics collected during search
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    pub nodes: NodeCount,
    pub depth: Depth,
    pub seldepth: Ply,
    pub time_ms: u64,
    pub hashfull: u32,
    pub qnodes: NodeCount,
    pub eval_calls: u64,
    // Profiling stats (ns)
    pub time_gen: u64,
    pub time_eval: u64,
    pub time_order: u64,
    pub time_search: u64, // (Rest of time)
}

impl SearchStats {
    pub fn nps(&self) -> u64 {
        if self.time_ms > 0 {
            self.nodes * 1000 / self.time_ms
        } else {
            0
        }
    }

    pub fn print_profiling(&self) {
        let total_ns = self.time_ms * 1_000_000;
        if total_ns > 0 {
            let gen_pct = self.time_gen * 100 / total_ns;
            let eval_pct = self.time_eval * 100 / total_ns;
            let order_pct = self.time_order * 100 / total_ns;
            let other = total_ns.saturating_sub(self.time_gen + self.time_eval + self.time_order);
            let other_pct = other * 100 / total_ns;
            
            println!("profiling: gen {}% eval {}% order {}% other {}%", 
                gen_pct, eval_pct, order_pct, other_pct);
            
             println!("stats: qnodes {} evals {}", self.qnodes, self.eval_calls);
        }
    }
}

/// Shared state between search threads
pub struct SharedState {
    /// Lock-free transposition table
    pub tt: TranspositionTable,
    /// Global stop flag
    pub stop: AtomicBool,
    /// Total nodes searched (sum across all threads)
    pub total_nodes: AtomicU64,
}

impl SharedState {
    pub fn new(hash_size_mb: usize) -> Self {
        Self {
            tt: TranspositionTable::new(hash_size_mb),
            stop: AtomicBool::new(false),
            total_nodes: AtomicU64::new(0),
        }
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new(16)
    }
}

/// Main search controller
pub struct Searcher {
    /// Current board position
    board: Board,
    /// Shared state (TT, stop flag) - wrapped in Arc for thread sharing
    pub shared: Arc<SharedState>,
    /// Killer moves table (per-thread)
    pub killers: KillerTable,
    /// History heuristic table (per-thread)
    pub history: HistoryTable,
    /// Counter-move table (per-thread)
    pub countermoves: CounterMoveTable,
    /// Time manager for search limits
    time_manager: TimeManager,
    /// Search statistics
    stats: SearchStats,
    /// Best move found so far
    best_move: Option<Move>,
    /// Principal variation
    pv: Vec<Move>,
    /// NNUE Model (thread-safe reference)
    pub nnue: Option<nnue::Model>,
    /// Position history for repetition detection (stores Zobrist hashes)
    pub position_history: Vec<u64>,
    /// Move stability counter (how many iterations best move unchanged)
    stable_move_count: u32,
    /// Last iteration's best move for stability tracking
    last_best_move: Option<Move>,
    /// Number of threads to use for search
    num_threads: usize,
    /// Is this a helper thread (no UCI output)
    is_helper: bool,
}

impl Searcher {
    pub fn new() -> Self {
        Self {
            board: Board::default(),
            shared: Arc::new(SharedState::default()),
            killers: KillerTable::new(),
            history: HistoryTable::new(),
            countermoves: CounterMoveTable::new(),
            time_manager: TimeManager::new(),
            stats: SearchStats::default(),
            best_move: None,
            pv: Vec::new(),
            nnue: None,
            position_history: Vec::with_capacity(512),
            stable_move_count: 0,
            last_best_move: None,
            num_threads: 1,
            is_helper: false,
        }
    }

    /// Create with specific TT size
    pub fn with_hash_size(size_mb: usize) -> Self {
        let mut s = Self::new();
        s.shared = Arc::new(SharedState::new(size_mb));
        s
    }
    
    /// Set number of search threads
    pub fn set_threads(&mut self, threads: usize) {
        self.num_threads = threads.max(1).min(64);
    }
    
    /// Get number of threads
    pub fn threads(&self) -> usize {
        self.num_threads
    }

    /// Set NNUE model
    pub fn set_nnue(&mut self, model: Option<nnue::Model>) {
        self.nnue = model;
    }

    /// Set the position to search with history for repetition detection
    pub fn set_position(&mut self, board: Board) {
        self.position_history.clear();
        self.position_history.push(board.hash());
        self.board = board;
    }
    
    /// Set position with move history for repetition detection
    pub fn set_position_with_history(&mut self, board: Board, history: Vec<u64>) {
        self.position_history = history;
        self.position_history.push(board.hash());
        self.board = board;
    }
    
    /// Check if position has repeated (for draw detection)
    pub fn is_repetition(&self, hash: u64) -> bool {
        self.position_history.iter().filter(|&&h| h == hash).count() >= 1
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
        self.shared.stop.store(true, Ordering::Relaxed);
    }

    /// Check if search should stop (hard time limit, nodes limit, etc.)
    pub fn should_stop(&self) -> bool {
        // Check global stop flag
        if self.shared.stop.load(Ordering::Relaxed) {
            return true;
        }
        
        // Check time periodically (every 512 nodes for stricter timing)
        // More frequent checks help prevent time losses in movetime mode
        if self.stats.nodes & 511 == 0 {
            if self.time_manager.hard_limit_exceeded() {
                return true;
            }
        }
        
        false
    }
    
    /// Check if we can start a new iteration (soft time limit)
    fn can_start_new_iteration(&self) -> bool {
        if self.shared.stop.load(Ordering::Relaxed) {
            return false;
        }
        
        // Check soft limit
        if !self.time_manager.can_start_iteration() {
            return false;
        }
        
        // Early termination: if best move has been stable for many iterations
        // and we've used a good portion of soft limit, we can stop early
        // Be conservative - only stop if very confident
        if self.stable_move_count >= 6 {
            let elapsed = self.time_manager.elapsed();
            let soft = self.time_manager.soft_limit_ms();
            // Only stop early if we've used at least 40% of our soft limit
            if elapsed > (soft * 2) / 5 {
                return false;
            }
        }
        
        true
    }
    
    /// Create a helper searcher that shares TT but has own tables
    fn create_helper(&self) -> Self {
        Self {
            board: self.board.clone(),
            shared: Arc::clone(&self.shared),
            killers: KillerTable::new(),
            history: HistoryTable::new(),
            countermoves: CounterMoveTable::new(),
            time_manager: self.time_manager.clone(),
            stats: SearchStats::default(),
            best_move: None,
            pv: Vec::new(),
            nnue: self.nnue.clone(),
            position_history: self.position_history.clone(),
            stable_move_count: 0,
            last_best_move: None,
            num_threads: 1,
            is_helper: true,
        }
    }

    /// Run the search with given limits (with Lazy SMP multi-threading)
    pub fn search(&mut self, limits: SearchLimits) -> SearchResult {
        // Reset state
        self.shared.stop.store(false, Ordering::Relaxed);
        self.shared.total_nodes.store(0, Ordering::Relaxed);
        self.stats = SearchStats::default();
        self.best_move = None;
        self.pv.clear();
        self.stable_move_count = 0;
        self.last_best_move = None;
        
        // Increment TT generation for new search
        self.shared.tt.new_search();
        
        // Clear killer moves for new search
        self.killers.clear();
        
        // Age history scores (decay old data, keep some history)
        self.history.age();
        
        // Configure time management
        self.time_manager = TimeManager::from_limits(&limits, self.board.turn());
        
        let max_depth = limits.depth.unwrap_or(Depth::MAX);
        
        // Spawn helper threads for Lazy SMP
        let mut handles = Vec::new();
        
        if self.num_threads > 1 {
            for _ in 1..self.num_threads {
                let mut helper = self.create_helper();
                let limits_clone = limits.clone();
                let max_d = max_depth;
                
                let handle = thread::spawn(move || {
                    helper.search_internal(limits_clone, max_d);
                });
                handles.push(handle);
            }
        }
        
        // Main thread search (prints UCI output)
        let result = self.search_internal(limits, max_depth);
        
        // Signal all helpers to stop
        self.shared.stop.store(true, Ordering::Relaxed);
        
        // Wait for all helper threads
        for handle in handles {
            let _ = handle.join();
        }
        
        // Get total nodes from all threads
        self.stats.nodes = self.shared.total_nodes.load(Ordering::Relaxed);
        
        result
    }
    
    /// Internal search loop (called by main and helper threads)
    fn search_internal(&mut self, _limits: SearchLimits, max_depth: Depth) -> SearchResult {
        let mut best_score = Score::neg_infinity();
        const INITIAL_WINDOW: i32 = 25;
        
        // Initialize evaluator at root
        let local_nnue = self.nnue.clone();
        let mut root_evaluator = SearchEvaluator::new(local_nnue.as_ref(), &self.board);

        for depth in 1..=max_depth.raw() {
            // Check if we can start a new iteration
            if !self.can_start_new_iteration() {
                break;
            }
            
            // Early termination: stop when forced mate is found (winning or losing)
            // No point searching further if we've found a forced mate
            if best_score.is_mate_score() && self.best_move.is_some() {
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
                let result = negamax::search::<Root>(
                    self,
                    &mut root_evaluator,
                    &self.board.clone(),
                    Depth::new(depth),
                    Ply::ZERO,
                    alpha,
                    beta,
                    None,  // No prev move at root
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
                    alpha = Score::neg_infinity();
                    beta = Score::infinity();
                }
            }

            self.stats.depth = Depth::new(depth);
            self.stats.hashfull = self.shared.tt.hashfull();
            
            // Update time from time manager
            self.stats.time_ms = self.time_manager.elapsed();
            
            // Report nodes to shared counter
            self.shared.total_nodes.fetch_add(self.stats.nodes, Ordering::Relaxed);
            
            // Track move stability for early termination
            if self.best_move == self.last_best_move {
                self.stable_move_count += 1;
            } else {
                self.stable_move_count = 0;
                self.last_best_move = self.best_move;
            }

            // Print info for this depth (main thread only)
            if !self.is_helper && !self.should_stop() {
                self.stats.print_profiling();
                self.stats.time_search = (self.time_manager.elapsed() as u64) * 1_000_000;
                let pv_str: String = self.pv.iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                    
                println!(
                    "info depth {} seldepth {} score {} nodes {} qnodes {} evals {} nps {} time {} hashfull {} pv {}",
                    depth,
                    self.stats.seldepth.raw(),
                    best_score,
                    self.shared.total_nodes.load(Ordering::Relaxed),
                    self.stats.qnodes,
                    self.stats.eval_calls,
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

    #[inline]
    pub fn add_gen_time(&mut self, ns: u64) {
        self.stats.time_gen += ns;
    }

    #[inline]
    pub fn add_eval_time(&mut self, ns: u64) {
        self.stats.time_eval += ns;
    }

    #[inline]
    pub fn add_order_time(&mut self, ns: u64) {
        self.stats.time_order += ns;
    }

    /// Increment qnodes counter
    #[inline]
    pub fn inc_qnodes(&mut self) {
        self.stats.qnodes += 1;
    }

    /// Increment eval call counter
    #[inline]
    pub fn inc_eval_calls(&mut self) {
        self.stats.eval_calls += 1;
    }
    
    /// Access the shared TT for probing
    #[inline]
    pub fn tt(&self) -> &TranspositionTable {
        &self.shared.tt
    }
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}
