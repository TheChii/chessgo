//! Transposition Table for caching search results.
//!
//! This module provides a high-performance, lock-free transposition table
//! that stores search results to avoid redundant computation.
//!
//! # Design
//! - 8-byte entries packed into AtomicU64 for lock-free access
//! - Depth-preferred replacement with age-based eviction
//! - Lock-free for Lazy SMP multi-threading support

use crate::types::{Move, Score, Depth, Hash};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// Type of bound stored in TT entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BoundType {
    /// No bound (empty entry)
    None = 0,
    /// Exact score (PV node)
    Exact = 1,
    /// Lower bound (fail-high, score >= beta)
    LowerBound = 2,
    /// Upper bound (fail-low, score <= alpha)
    UpperBound = 3,
}

impl From<u8> for BoundType {
    fn from(v: u8) -> Self {
        match v & 0x03 {
            1 => BoundType::Exact,
            2 => BoundType::LowerBound,
            3 => BoundType::UpperBound,
            _ => BoundType::None,
        }
    }
}

/// A single entry in the transposition table.
///
/// Packed into 8 bytes (64 bits) for atomic access:
/// - key: 16 bits (upper bits of hash for verification)
/// - best_move: 16 bits (encoded move)
/// - score: 16 bits
/// - depth: 8 bits
/// - bound_and_age: 8 bits (bound type in low 2 bits, age in high 6 bits)
#[derive(Debug, Clone, Copy, Default)]
pub struct TTEntry {
    /// Upper 16 bits of Zobrist hash for verification
    key: u16,
    /// Best move found (encoded)
    best_move: u16,
    /// Evaluation score
    score: i16,
    /// Search depth
    depth: i8,
    /// Bound type (2 bits) + generation/age (6 bits)
    bound_and_age: u8,
}

impl TTEntry {
    /// Create a new TT entry
    pub fn new(
        hash: Hash,
        best_move: Option<Move>,
        score: Score,
        depth: Depth,
        bound: BoundType,
        generation: u8,
    ) -> Self {
        Self {
            key: (hash >> 48) as u16,
            best_move: encode_move(best_move),
            score: score.raw() as i16,
            depth: depth.raw() as i8,
            bound_and_age: (bound as u8) | ((generation & 0x3F) << 2),
        }
    }
    
    /// Pack entry into a u64 for atomic storage
    /// Layout: key(16) | best_move(16) | score(16) | depth(8) | bound_and_age(8)
    #[inline]
    pub fn to_u64(&self) -> u64 {
        ((self.key as u64) << 48)
            | ((self.best_move as u64) << 32)
            | (((self.score as u16) as u64) << 16)
            | ((self.depth as u8 as u64) << 8)
            | (self.bound_and_age as u64)
    }
    
    /// Unpack entry from a u64
    #[inline]
    pub fn from_u64(raw: u64) -> Self {
        Self {
            key: (raw >> 48) as u16,
            best_move: (raw >> 32) as u16,
            score: (raw >> 16) as i16,
            depth: (raw >> 8) as i8,
            bound_and_age: raw as u8,
        }
    }

    /// Check if entry matches the given hash
    #[inline]
    pub fn matches(&self, hash: Hash) -> bool {
        self.key == (hash >> 48) as u16
    }

    /// Get the bound type
    #[inline]
    pub fn bound(&self) -> BoundType {
        BoundType::from(self.bound_and_age)
    }

    /// Get the generation/age
    #[inline]
    pub fn generation(&self) -> u8 {
        self.bound_and_age >> 2
    }

    /// Get the score
    #[inline]
    pub fn score(&self) -> Score {
        Score::cp(self.score as i32)
    }

    /// Get the depth
    #[inline]
    pub fn depth(&self) -> Depth {
        Depth::new(self.depth as i32)
    }

    /// Get the best move
    #[inline]
    pub fn best_move(&self) -> Option<Move> {
        decode_move(self.best_move)
    }

    /// Check if entry is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bound() == BoundType::None
    }
}

/// Encode a move into 16 bits: from (6) + to (6) + promo (4)
fn encode_move(m: Option<Move>) -> u16 {
    match m {
        Some(mv) => {
            let from = mv.get_source().to_index() as u16;
            let to = mv.get_dest().to_index() as u16;
            let promo = match mv.get_promotion() {
                Some(chess::Piece::Knight) => 1,
                Some(chess::Piece::Bishop) => 2,
                Some(chess::Piece::Rook) => 3,
                Some(chess::Piece::Queen) => 4,
                _ => 0,
            };
            (from) | (to << 6) | (promo << 12)
        }
        None => 0,
    }
}

/// Decode a 16-bit encoded move
fn decode_move(encoded: u16) -> Option<Move> {
    if encoded == 0 {
        return None;
    }

    let from_idx = (encoded & 0x3F) as u8;
    let to_idx = ((encoded >> 6) & 0x3F) as u8;
    let promo_bits = (encoded >> 12) & 0x0F;

    // Square::new is unsafe because it doesn't validate the index
    // We know our indices are valid (0-63) from the encoding
    let from = unsafe { chess::Square::new(from_idx) };
    let to = unsafe { chess::Square::new(to_idx) };

    let promo = match promo_bits {
        1 => Some(chess::Piece::Knight),
        2 => Some(chess::Piece::Bishop),
        3 => Some(chess::Piece::Rook),
        4 => Some(chess::Piece::Queen),
        _ => None,
    };

    Some(Move::new(from, to, promo))
}

/// Lock-free Transposition Table using AtomicU64
pub struct TranspositionTable {
    /// Table entries as atomic u64 values
    entries: Vec<AtomicU64>,
    /// Current generation (incremented each new search)
    generation: AtomicU8,
    /// Size in MB (for reporting)
    size_mb: usize,
}

// Safety: AtomicU64 and AtomicU8 are Send + Sync
unsafe impl Send for TranspositionTable {}
unsafe impl Sync for TranspositionTable {}

impl TranspositionTable {
    /// Create a new TT with given size in MB
    pub fn new(size_mb: usize) -> Self {
        // TTEntry is 8 bytes
        let entry_size = 8;
        let num_entries = (size_mb * 1024 * 1024) / entry_size;
        // Round to power of 2 for fast modulo
        let num_entries = num_entries.next_power_of_two() / 2;
        let num_entries = num_entries.max(1024); // Minimum 1024 entries

        let entries = (0..num_entries)
            .map(|_| AtomicU64::new(0))
            .collect();

        Self {
            entries,
            generation: AtomicU8::new(0),
            size_mb,
        }
    }

    /// Get the number of entries
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if table is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get size in MB
    pub fn size_mb(&self) -> usize {
        self.size_mb
    }
    
    /// Get current generation
    #[inline]
    pub fn generation(&self) -> u8 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Increment generation (call at start of each search)
    /// Takes &self for thread-safety - uses atomic operation
    pub fn new_search(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Get index for a hash
    #[inline]
    fn index(&self, hash: Hash) -> usize {
        // Fast modulo for power-of-2 size
        (hash as usize) & (self.entries.len() - 1)
    }

    /// Probe the TT for an entry (lock-free)
    #[inline]
    pub fn probe(&self, hash: Hash) -> Option<TTEntry> {
        let raw = self.entries[self.index(hash)].load(Ordering::Relaxed);
        if raw == 0 {
            return None;
        }
        
        let entry = TTEntry::from_u64(raw);
        if entry.matches(hash) && !entry.is_empty() {
            Some(entry)
        } else {
            None
        }
    }

    /// Store an entry in the TT (lock-free)
    ///
    /// Uses depth-preferred replacement with age consideration
    /// Takes &self - uses atomic operations for thread-safety
    pub fn store(
        &self,
        hash: Hash,
        best_move: Option<Move>,
        score: Score,
        depth: Depth,
        bound: BoundType,
    ) {
        let idx = self.index(hash);
        let existing_raw = self.entries[idx].load(Ordering::Relaxed);
        let existing = TTEntry::from_u64(existing_raw);
        let gen = self.generation();

        // Replacement strategy:
        // 1. Always replace empty entries
        // 2. Always replace entries from older generations
        // 3. Replace if new depth >= existing depth
        let should_replace = existing.is_empty()
            || existing.generation() != gen
            || depth.raw() >= existing.depth.into();

        if should_replace {
            let new_entry = TTEntry::new(hash, best_move, score, depth, bound, gen);
            self.entries[idx].store(new_entry.to_u64(), Ordering::Relaxed);
        }
    }

    /// Clear the table
    pub fn clear(&self) {
        for entry in &self.entries {
            entry.store(0, Ordering::Relaxed);
        }
        self.generation.store(0, Ordering::Relaxed);
    }

    /// Get hashfull in permill (for UCI info)
    pub fn hashfull(&self) -> u32 {
        let gen = self.generation();
        // Sample first 1000 entries
        let sample_size = self.entries.len().min(1000);
        let used = self.entries[..sample_size]
            .iter()
            .filter(|e| {
                let entry = TTEntry::from_u64(e.load(Ordering::Relaxed));
                !entry.is_empty() && entry.generation() == gen
            })
            .count();
        ((used * 1000) / sample_size) as u32
    }

    /// Prefetch entry for a hash (performance optimization)
    #[inline]
    pub fn prefetch(&self, hash: Hash) {
        let _ = self.index(hash);
        // Future: use platform-specific prefetch intrinsics
    }
}

impl Default for TranspositionTable {
    fn default() -> Self {
        Self::new(16) // 16 MB default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tt_basic() {
        let tt = TranspositionTable::new(1);
        let hash: Hash = 0x123456789ABCDEF0;

        // Initially empty
        assert!(tt.probe(hash).is_none());

        // Store and retrieve
        tt.store(hash, None, Score::cp(100), Depth::new(5), BoundType::Exact);

        let entry = tt.probe(hash).expect("Entry should exist");
        assert_eq!(entry.score().raw(), 100);
        assert_eq!(entry.depth().raw(), 5);
        assert_eq!(entry.bound(), BoundType::Exact);
    }

    #[test]
    fn test_move_encoding() {
        let mv = Move::new(
            chess::Square::E2,
            chess::Square::E4,
            None,
        );
        let encoded = encode_move(Some(mv));
        let decoded = decode_move(encoded).unwrap();
        assert_eq!(mv.get_source(), decoded.get_source());
        assert_eq!(mv.get_dest(), decoded.get_dest());
    }
    
    #[test]
    fn test_entry_pack_unpack() {
        let entry = TTEntry::new(
            0xABCD123456789000,
            None,
            Score::cp(150),
            Depth::new(8),
            BoundType::LowerBound,
            5,
        );
        
        let packed = entry.to_u64();
        let unpacked = TTEntry::from_u64(packed);
        
        assert_eq!(entry.key, unpacked.key);
        assert_eq!(entry.score, unpacked.score);
        assert_eq!(entry.depth, unpacked.depth);
        assert_eq!(entry.bound(), unpacked.bound());
        assert_eq!(entry.generation(), unpacked.generation());
    }
}
