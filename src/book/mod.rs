//! Opening book support for the chess engine.
//!
//! This module provides support for Polyglot format opening books (.bin files).
//! Polyglot is a widely used standard format for chess opening books.
//!
//! # Usage
//!
//! ```ignore
//! use chessinrust::book::PolyglotBook;
//!
//! let book = PolyglotBook::load("Human.bin")?;
//! let board = Board::default();
//!
//! // Get a weighted random move from the book
//! if let Some(m) = book.probe_move(&board) {
//!     println!("Book move: {}", m);
//! }
//! ```

mod polyglot;
mod zobrist;

pub use polyglot::{PolyglotBook, BookEntry};
pub use zobrist::polyglot_hash;
