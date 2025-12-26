//! Polyglot opening book format reader.
//!
//! This module implements reading and probing of Polyglot format (.bin) opening books.
//! The format consists of 16-byte entries sorted by position hash.

use super::zobrist::polyglot_hash;
use chess::{Board, Square, Piece, File, Rank};
use std::fs::File as FsFile;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Size of a single Polyglot entry in bytes
const ENTRY_SIZE: usize = 16;

/// A single entry from a Polyglot opening book
#[derive(Debug, Clone, Copy)]
pub struct BookEntry {
    /// Zobrist hash of the position
    pub key: u64,
    /// Encoded move
    pub raw_move: u16,
    /// Weight/priority of this move
    pub weight: u16,
    /// Learning data (usually 0)
    pub learn: u32,
}

impl BookEntry {
    /// Parse an entry from raw bytes (big-endian format)
    fn from_bytes(bytes: &[u8; 16]) -> Self {
        Self {
            key: u64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
                bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            raw_move: u16::from_be_bytes([bytes[8], bytes[9]]),
            weight: u16::from_be_bytes([bytes[10], bytes[11]]),
            learn: u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        }
    }

    /// Decode the raw move to source square, destination square, and promotion
    pub fn decode_move(&self) -> (Square, Square, Option<Piece>) {
        let to_file = (self.raw_move & 0x7) as usize;
        let to_rank = ((self.raw_move >> 3) & 0x7) as usize;
        let from_file = ((self.raw_move >> 6) & 0x7) as usize;
        let from_rank = ((self.raw_move >> 9) & 0x7) as usize;
        let promo = ((self.raw_move >> 12) & 0x7) as usize;

        let from = Square::make_square(
            Rank::from_index(from_rank),
            File::from_index(from_file),
        );
        let to = Square::make_square(
            Rank::from_index(to_rank),
            File::from_index(to_file),
        );

        // Promotion: 0=none, 1=knight, 2=bishop, 3=rook, 4=queen
        let promotion = match promo {
            1 => Some(Piece::Knight),
            2 => Some(Piece::Bishop),
            3 => Some(Piece::Rook),
            4 => Some(Piece::Queen),
            _ => None,
        };

        (from, to, promotion)
    }

    /// Convert this entry to a chess::ChessMove for the given board
    pub fn to_chess_move(&self, board: &Board) -> Option<chess::ChessMove> {
        let (from, to, promo) = self.decode_move();
        
        // Handle castling - Polyglot uses king captures rook notation
        let actual_to = self.adjust_castling_move(board, from, to);
        
        // Find matching legal move
        let movegen = chess::MoveGen::new_legal(board);
        for m in movegen {
            if m.get_source() == from && m.get_dest() == actual_to {
                // For promotions, also check the promotion piece
                if promo.is_some() {
                    if m.get_promotion() == promo {
                        return Some(m);
                    }
                } else if m.get_promotion().is_none() {
                    return Some(m);
                }
            }
        }
        None
    }

    /// Adjust castling moves from Polyglot format (king captures rook) to standard format
    fn adjust_castling_move(&self, board: &Board, from: Square, to: Square) -> Square {
        // Check if this is a castling move
        if let Some(piece) = board.piece_on(from) {
            if piece == Piece::King {
                // Polyglot encodes castling as king captures rook
                // We need to convert to standard king move
                let from_file = from.get_file();
                let to_file = to.get_file();
                
                // E-file king moving to A or H file indicates castling
                if from_file == File::E {
                    if to_file == File::H {
                        // Kingside castling: e1h1 -> e1g1 or e8h8 -> e8g8
                        return Square::make_square(to.get_rank(), File::G);
                    } else if to_file == File::A {
                        // Queenside castling: e1a1 -> e1c1 or e8a8 -> e8c8
                        return Square::make_square(to.get_rank(), File::C);
                    }
                }
            }
        }
        to
    }
}

/// Polyglot opening book reader
pub struct PolyglotBook {
    /// Book entries loaded in memory (for small books) or file handle
    data: BookData,
    /// Number of entries in the book
    entry_count: usize,
    /// Book description/path for logging
    pub desc: String,
}

enum BookData {
    /// Entries stored in memory
    Memory(Vec<BookEntry>),
    /// File-based access (for large books)
    File { path: String },
}

impl PolyglotBook {
    /// Load a Polyglot book from a file
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path = path.as_ref();
        let mut file = FsFile::open(path)?;
        
        // Get file size
        let file_size = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;
        
        if file_size % ENTRY_SIZE as u64 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid Polyglot book file size",
            ));
        }
        
        let entry_count = (file_size / ENTRY_SIZE as u64) as usize;
        let desc = path.to_string_lossy().to_string();
        
        // For books under 50MB, load into memory for faster access
        const MEMORY_THRESHOLD: u64 = 50 * 1024 * 1024;
        
        if file_size <= MEMORY_THRESHOLD {
            let mut data = vec![0u8; file_size as usize];
            file.read_exact(&mut data)?;
            
            let entries: Vec<BookEntry> = data
                .chunks_exact(ENTRY_SIZE)
                .map(|chunk| {
                    let bytes: [u8; 16] = chunk.try_into().unwrap();
                    BookEntry::from_bytes(&bytes)
                })
                .collect();
            
            Ok(Self {
                data: BookData::Memory(entries),
                entry_count,
                desc,
            })
        } else {
            Ok(Self {
                data: BookData::File { path: desc.clone() },
                entry_count,
                desc,
            })
        }
    }

    /// Get all book entries for a position
    pub fn probe(&self, board: &Board) -> Vec<BookEntry> {
        let key = polyglot_hash(board);
        self.find_entries(key)
    }

    /// Get a weighted random move from the book for a position
    pub fn probe_move(&self, board: &Board) -> Option<chess::ChessMove> {
        let entries = self.probe(board);
        if entries.is_empty() {
            return None;
        }

        // Calculate total weight
        let total_weight: u32 = entries.iter().map(|e| e.weight as u32).sum();
        
        if total_weight == 0 {
            // If all weights are 0, just pick the first entry
            return entries[0].to_chess_move(board);
        }

        // Simple weighted random selection using a basic LCG
        // This avoids needing the rand crate
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(12345);
        let random = (seed.wrapping_mul(6364136223846793005).wrapping_add(1)) % total_weight as u64;
        
        let mut cumulative = 0u64;
        for entry in &entries {
            cumulative += entry.weight as u64;
            if random < cumulative {
                return entry.to_chess_move(board);
            }
        }
        
        // Fallback to first entry
        entries[0].to_chess_move(board)
    }

    /// Get the best move (highest weight) from the book
    pub fn probe_best_move(&self, board: &Board) -> Option<chess::ChessMove> {
        let entries = self.probe(board);
        entries
            .iter()
            .max_by_key(|e| e.weight)
            .and_then(|e| e.to_chess_move(board))
    }

    /// Find all entries matching a key using binary search
    fn find_entries(&self, key: u64) -> Vec<BookEntry> {
        match &self.data {
            BookData::Memory(entries) => self.find_entries_memory(entries, key),
            BookData::File { path } => self.find_entries_file(path, key).unwrap_or_default(),
        }
    }

    fn find_entries_memory(&self, entries: &[BookEntry], key: u64) -> Vec<BookEntry> {
        // Binary search for first matching entry
        let idx = match entries.binary_search_by_key(&key, |e| e.key) {
            Ok(i) => i,
            Err(_) => return Vec::new(),
        };

        // Find all entries with this key (there may be multiple)
        let mut result = Vec::new();
        
        // Scan backwards to find first entry with this key
        let mut start = idx;
        while start > 0 && entries[start - 1].key == key {
            start -= 1;
        }
        
        // Collect all entries with this key
        let mut i = start;
        while i < entries.len() && entries[i].key == key {
            result.push(entries[i]);
            i += 1;
        }
        
        result
    }

    fn find_entries_file(&self, path: &str, key: u64) -> io::Result<Vec<BookEntry>> {
        let mut file = FsFile::open(path)?;
        
        // Binary search in file
        let mut low = 0usize;
        let mut high = self.entry_count;
        
        while low < high {
            let mid = (low + high) / 2;
            file.seek(SeekFrom::Start((mid * ENTRY_SIZE) as u64))?;
            
            let mut bytes = [0u8; 16];
            file.read_exact(&mut bytes)?;
            let entry = BookEntry::from_bytes(&bytes);
            
            if entry.key < key {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        
        // Collect all entries with this key
        let mut result = Vec::new();
        let mut pos = low;
        
        while pos < self.entry_count {
            file.seek(SeekFrom::Start((pos * ENTRY_SIZE) as u64))?;
            let mut bytes = [0u8; 16];
            file.read_exact(&mut bytes)?;
            let entry = BookEntry::from_bytes(&bytes);
            
            if entry.key != key {
                break;
            }
            result.push(entry);
            pos += 1;
        }
        
        Ok(result)
    }

    /// Get the number of entries in the book
    pub fn len(&self) -> usize {
        self.entry_count
    }

    /// Check if the book is empty
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_decode_move() {
        // Test e2e4 (from=12, to=28)
        // from_file=4 (e), from_rank=1 (2)
        // to_file=4 (e), to_rank=3 (4)
        // raw = to_file | (to_rank << 3) | (from_file << 6) | (from_rank << 9)
        // raw = 4 | (3 << 3) | (4 << 6) | (1 << 9) = 4 | 24 | 256 | 512 = 796
        let entry = BookEntry {
            key: 0,
            raw_move: 796,
            weight: 100,
            learn: 0,
        };
        let (from, to, promo) = entry.decode_move();
        assert_eq!(from, Square::make_square(Rank::Second, File::E));
        assert_eq!(to, Square::make_square(Rank::Fourth, File::E));
        assert!(promo.is_none());
    }
}
