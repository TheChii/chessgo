//! Search limits and time management.
//!
//! Handles:
//! - Fixed depth search
//! - Fixed time search
//! - Time control with increment
//! - Infinite search (until stop)

use crate::types::{Depth, Color};
use crate::uci::SearchParams;

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
}

impl SearchLimits {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn depth(depth: i32) -> Self {
        Self {
            depth: Some(Depth::new(depth)),
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
        }
    }
}

/// Time manager for search
#[derive(Debug, Clone)]
pub struct TimeManager {
    /// Allocated time for this move (ms)
    allocated_time: u64,
    /// Maximum time allowed (hard limit)
    max_time: u64,
    /// Is this an infinite search?
    infinite: bool,
}

impl TimeManager {
    pub fn new() -> Self {
        Self {
            allocated_time: u64::MAX,
            max_time: u64::MAX,
            infinite: true,
        }
    }

    /// Create time manager from search limits
    pub fn from_limits(limits: &SearchLimits, side: Color) -> Self {
        if limits.infinite {
            return Self::new();
        }

        // Fixed movetime
        if let Some(mt) = limits.movetime {
            return Self {
                allocated_time: mt,
                max_time: mt,
                infinite: false,
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

            // Simple time allocation:
            // Use time_left / moves_to_go + some portion of increment
            let base_time = time / moves_to_go.max(1);
            let inc_bonus = inc * 3 / 4;

            let allocated = base_time + inc_bonus;
            // Don't use more than 1/3 of remaining time
            let max = time / 3;

            return Self {
                allocated_time: allocated.min(max),
                max_time: max,
                infinite: false,
            };
        }

        // Fallback to infinite
        Self::new()
    }

    /// Check if we should stop searching
    pub fn should_stop(&self, elapsed_ms: u64) -> bool {
        if self.infinite {
            return false;
        }
        elapsed_ms >= self.allocated_time
    }

    /// Check if we can start a new iteration
    #[allow(dead_code)]
    pub fn can_start_iteration(&self, elapsed_ms: u64) -> bool {
        if self.infinite {
            return true;
        }
        // Start new iteration if we have at least 50% of allocated time left
        elapsed_ms < self.allocated_time / 2
    }

    /// Should we stop now regardless of iteration?
    #[allow(dead_code)]
    pub fn hard_stop(&self, elapsed_ms: u64) -> bool {
        if self.infinite {
            return false;
        }
        elapsed_ms >= self.max_time
    }
}

impl Default for TimeManager {
    fn default() -> Self {
        Self::new()
    }
}
