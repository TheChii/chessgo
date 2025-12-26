//! Search limits and time management.
//!
//! Handles:
//! - Fixed depth search
//! - Fixed time search
//! - Time control with increment
//! - Infinite search (until stop)
//! - Soft/hard time limits for optimal iteration control

use crate::types::{Depth, Color};
use crate::uci::SearchParams;
use std::time::Instant;

/// Search limits configuration
#[derive(Debug, Clone, Default)]
pub struct SearchLimits {
    /// Maximum depth to search
    pub depth: Option<Depth>,
    /// Maximum time in milliseconds
    pub movetime: Option<u64>,
    /// Maximum nodes to search
    pub nodes: Option<u64>,
    /// White time remaining (ms)
    pub wtime: Option<u64>,
    /// Black time remaining (ms)
    pub btime: Option<u64>,
    /// White increment (ms)
    pub winc: Option<u64>,
    /// Black increment (ms)
    pub binc: Option<u64>,
    /// Moves until next time control
    pub movestogo: Option<u32>,
    /// Infinite search
    pub infinite: bool,
    /// Move overhead (safety buffer for network/GUI delay)
    pub move_overhead: u64,
}

impl SearchLimits {
    /// Default move overhead for timing safety (ms)
    pub const DEFAULT_MOVE_OVERHEAD: u64 = 50;
    
    pub fn new() -> Self {
        Self {
            move_overhead: Self::DEFAULT_MOVE_OVERHEAD,
            ..Default::default()
        }
    }

    pub fn depth(depth: i32) -> Self {
        Self {
            depth: Some(Depth::new(depth)),
            move_overhead: Self::DEFAULT_MOVE_OVERHEAD,
            ..Default::default()
        }
    }

    pub fn from_params(params: &SearchParams) -> Self {
        Self {
            depth: params.depth,
            movetime: params.movetime,
            nodes: params.nodes,
            wtime: params.wtime,
            btime: params.btime,
            winc: params.winc,
            binc: params.binc,
            movestogo: params.movestogo,
            infinite: params.infinite,
            move_overhead: Self::DEFAULT_MOVE_OVERHEAD,
        }
    }
    
    /// Set move overhead (from UCI option)
    pub fn with_move_overhead(mut self, overhead: u64) -> Self {
        self.move_overhead = overhead;
        self
    }
}

/// Time manager for search with soft and hard limits
#[derive(Debug, Clone)]
pub struct TimeManager {
    /// Soft time limit - target time to use (stop after iteration)
    soft_limit: u64,
    /// Hard time limit - absolute maximum (stop mid-search if exceeded)
    hard_limit: u64,
    /// Move overhead safety buffer
    _move_overhead: u64,
    /// Is this an infinite search?
    infinite: bool,
    /// Start time of search
    start_time: Option<Instant>,
}

impl TimeManager {
    pub fn new() -> Self {
        Self {
            soft_limit: u64::MAX,
            hard_limit: u64::MAX,
            _move_overhead: 10,
            infinite: true,
            start_time: Some(Instant::now()),
        }
    }

    /// Create time manager from search limits
    pub fn from_limits(limits: &SearchLimits, side: Color) -> Self {
        if limits.infinite {
            return Self::new();
        }

        let move_overhead = limits.move_overhead;

        // Fixed movetime - use stricter limits to avoid time losses
        // Soft limit: 85% of available time (stop after iteration)
        // Hard limit: 95% of available time (absolute stop during search)
        if let Some(mt) = limits.movetime {
            let available = mt.saturating_sub(move_overhead);
            // Use 85% for soft limit (when to stop starting new iterations)
            let soft = (available * 85) / 100;
            // Use 95% for hard limit (absolute cutoff mid-search)
            let hard = (available * 95) / 100;
            return Self {
                soft_limit: soft.max(1),
                hard_limit: hard.max(1),
                _move_overhead: move_overhead,
                infinite: false,
                start_time: Some(Instant::now()),
            };
        }

        // Time control
        let (time_left, increment) = match side {
            Color::White => (limits.wtime, limits.winc),
            Color::Black => (limits.btime, limits.binc),
        };

        if let Some(time) = time_left {
            let inc = increment.unwrap_or(0);
            let moves_to_go = limits.movestogo.unwrap_or(30) as u64;
            
            // Subtract overhead from available time
            let available = time.saturating_sub(move_overhead);
            
            // Base time allocation
            let base_time = available / moves_to_go.max(1);
            
            // Add portion of increment to soft limit
            let inc_bonus = (inc * 3) / 4;
            
            // Soft limit: base + increment bonus
            let soft = (base_time + inc_bonus).min(available);
            
            // Hard limit: min(3x soft, 25% of available time)
            // This prevents using too much time on one move
            let hard = (soft * 3).min(available / 4).max(soft);
            
            return Self {
                soft_limit: soft,
                hard_limit: hard,
                _move_overhead: move_overhead,
                infinite: false,
                start_time: Some(Instant::now()),
            };
        }

        // Fallback to infinite (but with timer started)
        Self {
            soft_limit: u64::MAX,
            hard_limit: u64::MAX,
            _move_overhead: move_overhead,
            infinite: true,
            start_time: Some(Instant::now()),
        }
    }
    
    /// Start the timer (call at search start)
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed(&self) -> u64 {
        self.start_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    /// Check if we should stop searching (hard limit - for mid-search check)
    pub fn should_stop(&self) -> bool {
        if self.infinite {
            return false;
        }
        self.elapsed() >= self.hard_limit
    }

    /// Check if we can start a new iteration (soft limit)
    pub fn can_start_iteration(&self) -> bool {
        if self.infinite {
            return true;
        }
        // Start new iteration if we have time remaining below soft limit
        // and predict we can complete at least a partial iteration
        self.elapsed() < self.soft_limit
    }

    /// Check if we've exceeded soft limit (use between iterations)
    pub fn soft_limit_exceeded(&self) -> bool {
        if self.infinite {
            return false;
        }
        self.elapsed() >= self.soft_limit
    }

    /// Hard stop check (absolute limit - never exceed)
    pub fn hard_limit_exceeded(&self) -> bool {
        if self.infinite {
            return false;
        }
        self.elapsed() >= self.hard_limit
    }
    
    /// Extend time limits (when search is in trouble, e.g., score dropped)
    /// factor > 1.0 extends time, factor < 1.0 reduces time
    #[allow(dead_code)]
    pub fn extend_time(&mut self, factor: f64) {
        if !self.infinite {
            self.soft_limit = ((self.soft_limit as f64) * factor) as u64;
            // Hard limit extends less aggressively
            self.hard_limit = ((self.hard_limit as f64) * factor.sqrt()) as u64;
        }
    }
    
    /// Get the soft limit in ms
    pub fn soft_limit_ms(&self) -> u64 {
        self.soft_limit
    }
    
    /// Get the hard limit in ms
    pub fn hard_limit_ms(&self) -> u64 {
        self.hard_limit
    }
    
    /// Check if this is an infinite search
    pub fn is_infinite(&self) -> bool {
        self.infinite
    }
}

impl Default for TimeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fixed_movetime() {
        let limits = SearchLimits {
            movetime: Some(1000),
            move_overhead: 50,
            ..Default::default()
        };
        let tm = TimeManager::from_limits(&limits, Color::White);
        
        assert!(!tm.is_infinite());
        // 1000 - 50 overhead = 950 available
        // soft = 950 * 85% = 807
        // hard = 950 * 95% = 902
        assert_eq!(tm.soft_limit_ms(), 807);
        assert_eq!(tm.hard_limit_ms(), 902);
    }
    
    #[test]
    fn test_time_control() {
        let limits = SearchLimits {
            wtime: Some(60000),
            btime: Some(60000),
            winc: Some(1000),
            binc: Some(1000),
            move_overhead: 10,
            ..Default::default()
        };
        let tm = TimeManager::from_limits(&limits, Color::White);
        
        assert!(!tm.is_infinite());
        // 60000 - 10 = 59990 available
        // base = 59990 / 30 = ~1999
        // inc_bonus = 1000 * 0.75 = 750
        // soft = ~2749
        assert!(tm.soft_limit_ms() > 2000);
        assert!(tm.soft_limit_ms() < 4000);
        // hard = min(3 * soft, available / 4)
        assert!(tm.hard_limit_ms() >= tm.soft_limit_ms());
    }
    
    #[test]
    fn test_infinite() {
        let limits = SearchLimits {
            infinite: true,
            ..Default::default()
        };
        let tm = TimeManager::from_limits(&limits, Color::White);
        
        assert!(tm.is_infinite());
        assert!(tm.can_start_iteration());
        assert!(!tm.should_stop());
    }
}
