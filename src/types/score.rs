//! Score type for search.
//!
//! Handles regular centipawn scores, mate scores, draws, and special values.
//! Optimized for alpha-beta search with proper mate score handling.

use std::fmt;
use std::ops::{Add, Sub, Neg};

/// Special score values
pub const SCORE_NONE: i32 = -32001;
pub const SCORE_INFINITY: i32 = 32000;
pub const SCORE_MATE: i32 = 31000;
pub const SCORE_DRAW: i32 = 0;

// Mate score bounds for detection
const SCORE_MATE_IN_MAX: i32 = SCORE_MATE - 1000;
const SCORE_MATED_IN_MAX: i32 = -SCORE_MATE + 1000;

/// A chess engine score.
///
/// Internally stored as centipawns with special encoding for mate scores.
/// Mate in N is encoded as `SCORE_MATE - N`, mated in N as `-SCORE_MATE + N`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Score(pub i32);

impl Score {
    /// Create a new score from centipawns
    #[inline]
    pub const fn cp(centipawns: i32) -> Self {
        Score(centipawns)
    }

    /// Create a mate score (mate in N plies from root)
    #[inline]
    pub const fn mate_in(ply: i32) -> Self {
        Score(SCORE_MATE - ply)
    }

    /// Create a mated score (mated in N plies from root)
    #[inline]
    pub const fn mated_in(ply: i32) -> Self {
        Score(-SCORE_MATE + ply)
    }

    /// Draw score
    #[inline]
    pub const fn draw() -> Self {
        Score(SCORE_DRAW)
    }

    /// Infinity (for alpha-beta bounds)
    #[inline]
    pub const fn infinity() -> Self {
        Score(SCORE_INFINITY)
    }

    /// Negative infinity
    #[inline]
    pub const fn neg_infinity() -> Self {
        Score(-SCORE_INFINITY)
    }

    /// No score / undefined
    #[inline]
    pub const fn none() -> Self {
        Score(SCORE_NONE)
    }

    /// Get the raw value
    #[inline]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Check if this is a mate score (winning)
    #[inline]
    pub const fn is_mate(self) -> bool {
        self.0 >= SCORE_MATE_IN_MAX
    }

    /// Check if this is a mated score (losing)
    #[inline]
    pub const fn is_mated(self) -> bool {
        self.0 <= SCORE_MATED_IN_MAX
    }

    /// Check if this is any kind of mate score
    #[inline]
    pub const fn is_mate_score(self) -> bool {
        self.is_mate() || self.is_mated()
    }

    /// Get mate distance in plies (if this is a mate score)
    #[inline]
    pub const fn mate_distance(self) -> Option<i32> {
        if self.is_mate() {
            Some(SCORE_MATE - self.0)
        } else if self.is_mated() {
            Some(self.0 + SCORE_MATE)
        } else {
            None
        }
    }

    /// Adjust a mate score when storing in TT (relative to current ply)
    #[inline]
    pub const fn to_tt(self, ply: i32) -> Self {
        if self.is_mate() {
            Score(self.0 + ply)
        } else if self.is_mated() {
            Score(self.0 - ply)
        } else {
            self
        }
    }

    /// Adjust a mate score when retrieving from TT
    #[inline]
    pub const fn from_tt(self, ply: i32) -> Self {
        if self.is_mate() {
            Score(self.0 - ply)
        } else if self.is_mated() {
            Score(self.0 + ply)
        } else {
            self
        }
    }
}

impl Add for Score {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Score(self.0 + rhs.0)
    }
}

impl Sub for Score {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Score(self.0 - rhs.0)
    }
}

impl Neg for Score {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Score(-self.0)
    }
}

impl From<i32> for Score {
    #[inline]
    fn from(v: i32) -> Self {
        Score(v)
    }
}

impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_mate() {
            let moves = (SCORE_MATE - self.0 + 1) / 2;
            write!(f, "mate {}", moves)
        } else if self.is_mated() {
            let moves = (self.0 + SCORE_MATE + 1) / 2;
            write!(f, "mate -{}", moves)
        } else {
            write!(f, "cp {}", self.0)
        }
    }
}

impl fmt::Debug for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Score({})", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mate_scores() {
        let mate_in_3 = Score::mate_in(5); // 5 ply = mate in 3 moves
        assert!(mate_in_3.is_mate());
        assert!(!mate_in_3.is_mated());
        assert_eq!(mate_in_3.mate_distance(), Some(5));

        let mated_in_2 = Score::mated_in(3); // 3 ply = mated in 2 moves
        assert!(!mated_in_2.is_mate());
        assert!(mated_in_2.is_mated());
        assert_eq!(mated_in_2.mate_distance(), Some(3));
    }

    #[test]
    fn test_tt_adjustment() {
        let mate = Score::mate_in(5);
        let tt_score = mate.to_tt(2);
        let restored = tt_score.from_tt(2);
        assert_eq!(mate, restored);
    }
}
