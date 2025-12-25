//! Depth and Ply types for search.
//!
//! Provides type-safe wrappers for search depth and ply count.

use std::ops::{Add, Sub, AddAssign, SubAssign};

/// Maximum search depth
pub const MAX_DEPTH: i32 = 128;

/// Maximum ply (half-moves from root)
pub const MAX_PLY: i32 = 256;

/// Search depth (in plies).
///
/// Represents how deep to search. Can be fractional in some contexts
/// (for extensions/reductions), but stored as integer plies here.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Debug)]
#[repr(transparent)]
pub struct Depth(pub i32);

impl Depth {
    pub const ZERO: Depth = Depth(0);
    pub const ONE: Depth = Depth(1);
    pub const MAX: Depth = Depth(MAX_DEPTH);

    /// Quiescence search depth marker
    pub const QS: Depth = Depth(0);

    #[inline]
    pub const fn new(d: i32) -> Self {
        Depth(d)
    }

    #[inline]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Check if this depth requires quiescence search
    #[inline]
    pub const fn is_qs(self) -> bool {
        self.0 <= 0
    }
}

impl Add for Depth {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Depth(self.0 + rhs.0)
    }
}

impl Sub for Depth {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Depth(self.0 - rhs.0)
    }
}

impl Add<i32> for Depth {
    type Output = Self;
    #[inline]
    fn add(self, rhs: i32) -> Self {
        Depth(self.0 + rhs)
    }
}

impl Sub<i32> for Depth {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: i32) -> Self {
        Depth(self.0 - rhs)
    }
}

impl AddAssign<i32> for Depth {
    #[inline]
    fn add_assign(&mut self, rhs: i32) {
        self.0 += rhs;
    }
}

impl SubAssign<i32> for Depth {
    #[inline]
    fn sub_assign(&mut self, rhs: i32) {
        self.0 -= rhs;
    }
}

impl From<i32> for Depth {
    #[inline]
    fn from(d: i32) -> Self {
        Depth(d)
    }
}

/// Ply count (half-moves from the root position).
///
/// Used to track distance from root in search, for mate distance calculation,
/// and stack indexing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Debug)]
#[repr(transparent)]
pub struct Ply(pub i32);

impl Ply {
    pub const ZERO: Ply = Ply(0);
    pub const MAX: Ply = Ply(MAX_PLY);

    #[inline]
    pub const fn new(p: i32) -> Self {
        Ply(p)
    }

    #[inline]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Increment ply (for going deeper in search)
    #[inline]
    pub const fn next(self) -> Self {
        Ply(self.0 + 1)
    }

    /// Get as usize for array indexing
    #[inline]
    pub const fn as_index(self) -> usize {
        self.0 as usize
    }
}

impl Add<i32> for Ply {
    type Output = Self;
    #[inline]
    fn add(self, rhs: i32) -> Self {
        Ply(self.0 + rhs)
    }
}

impl Sub<i32> for Ply {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: i32) -> Self {
        Ply(self.0 - rhs)
    }
}

impl AddAssign<i32> for Ply {
    #[inline]
    fn add_assign(&mut self, rhs: i32) {
        self.0 += rhs;
    }
}

impl From<i32> for Ply {
    #[inline]
    fn from(p: i32) -> Self {
        Ply(p)
    }
}
