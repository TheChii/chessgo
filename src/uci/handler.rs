//! UCI command handler and main loop.

use super::parser::{parse_command, UciCommand};
use super::{parse_move, format_move, SearchParams, ENGINE_NAME, ENGINE_AUTHOR};
use crate::types::{Board, Move, Score};
use crate::search::{Searcher, SearchLimits};
use crate::eval::nnue;
use crate::book::PolyglotBook;
use std::io::{self, BufRead, Write};
use std::str::FromStr;

/// UCI protocol handler
pub struct UciHandler {
    /// Current board position
    board: Board,
    /// Search engine
    searcher: Searcher,
    /// Opening book
    book: Option<PolyglotBook>,
    /// Use opening book
    use_own_book: bool,
    /// Path to opening book file
    book_path: String,
    /// Debug mode enabled
    debug: bool,
    /// Should the engine quit
    quit: bool,
    /// Move overhead in milliseconds (safety buffer for time control)
    move_overhead: u64,
}

impl Default for UciHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl UciHandler {
    pub fn new() -> Self {
        let mut searcher = Searcher::new();
        
        // Attempt to load NNUE model (look next to executable first, then current dir)
        let exe_dir_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("network.nnue")));
        
        let nnue_path = if let Some(ref p) = exe_dir_path {
            if p.exists() {
                println!("info string Found NNUE next to exe: {:?}", p);
                p.clone()
            } else {
                println!("info string NNUE not at exe path: {:?}", p);
                std::path::PathBuf::from("network.nnue")
            }
        } else {
            println!("info string Could not determine exe path");
            std::path::PathBuf::from("network.nnue")
        };
        
        match nnue::load_model(nnue_path.to_str().unwrap_or("network.nnue")) {
            Ok(model) => {
                println!("info string NNUE loaded: {}", model.desc);
                searcher.set_nnue(Some(model));
            },
            Err(e) => {
                println!("info string NNUE load failed: {:?}", e);
                println!("info string Using material eval");
            }
        }

        // Attempt to load opening book (look next to executable first, then current dir)
        let book_filename = "Human.bin";
        let exe_dir_book = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(book_filename)));
        
        let book_path = if let Some(ref p) = exe_dir_book {
            if p.exists() {
                println!("info string Found book next to exe: {:?}", p);
                p.to_string_lossy().to_string()
            } else {
                println!("info string Book not at exe path: {:?}, trying current dir", p);
                std::path::PathBuf::from(book_filename).to_string_lossy().to_string()
            }
        } else {
            println!("info string Could not determine exe path for book");
            book_filename.to_string()
        };

        let book = match PolyglotBook::load(&book_path) {
            Ok(b) => {
                println!("info string Opening book loaded: {} ({} entries)", b.desc, b.len());
                Some(b)
            }
            Err(e) => {
                println!("info string Opening book not loaded: {:?}", e);
                None
            }
        };

        Self {
            board: Board::default(),
            searcher,
            book,
            use_own_book: true, // Enable book by default
            book_path,
            debug: false,
            quit: false,
            move_overhead: 10, // Default 10ms
        }
    }

    /// Run the UCI main loop (blocking)
    pub fn run(&mut self) {
        let stdin = io::stdin();
        let reader = stdin.lock();

        for line in reader.lines() {
            match line {
                Ok(input) => {
                    if self.debug {
                        eprintln!("< {}", input);
                    }
                    self.handle_input(&input);
                    if self.quit {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }

    /// Handle a single UCI command
    pub fn handle_input(&mut self, input: &str) {
        let cmd = parse_command(input);
        self.handle_command(cmd);
    }

    fn handle_command(&mut self, cmd: UciCommand) {
        match cmd {
            UciCommand::Uci => self.cmd_uci(),
            UciCommand::Debug(on) => self.cmd_debug(on),
            UciCommand::IsReady => self.cmd_isready(),
            UciCommand::SetOption { name, value } => self.cmd_setoption(&name, value.as_deref()),
            UciCommand::Register => {} // Ignore registration
            UciCommand::UciNewGame => self.cmd_ucinewgame(),
            UciCommand::Position { fen, moves } => self.cmd_position(fen.as_deref(), &moves),
            UciCommand::Go(params) => self.cmd_go(params),
            UciCommand::Stop => self.cmd_stop(),
            UciCommand::PonderHit => self.cmd_ponderhit(),
            UciCommand::Quit => self.cmd_quit(),
            UciCommand::Display => self.cmd_display(),
            UciCommand::Unknown(s) => {
                if self.debug {
                    eprintln!("Unknown command: {}", s);
                }
            }
        }
    }

    /// Send output to GUI
    fn send(&self, msg: &str) {
        println!("{}", msg);
        io::stdout().flush().ok();
    }

    // === UCI Commands ===

    fn cmd_uci(&self) {
        self.send(&format!("id name {}", ENGINE_NAME));
        self.send(&format!("id author {}", ENGINE_AUTHOR));
        
        // Send options
        self.send("option name Threads type spin default 1 min 1 max 64");
        self.send("option name MoveOverhead type spin default 10 min 0 max 5000");
        self.send("option name OwnBook type check default true");
        self.send("option name BookPath type string default Human.bin");
        
        self.send("uciok");
    }

    fn cmd_debug(&mut self, on: bool) {
        self.debug = on;
    }

    fn cmd_isready(&self) {
        self.send("readyok");
    }

    fn cmd_setoption(&mut self, name: &str, value: Option<&str>) {
        match name.to_lowercase().as_str() {
            "threads" => {
                if let Some(v) = value {
                    if let Ok(n) = v.parse::<usize>() {
                        self.searcher.set_threads(n);
                    }
                }
            }
            "moveoverhead" => {
                if let Some(v) = value {
                    if let Ok(ms) = v.parse::<u64>() {
                        self.move_overhead = ms.min(5000);
                    }
                }
            }
            "ownbook" => {
                if let Some(v) = value {
                    self.use_own_book = v.to_lowercase() == "true";
                    if self.debug {
                        eprintln!("OwnBook set to: {}", self.use_own_book);
                    }
                }
            }
            "bookpath" => {
                if let Some(v) = value {
                    self.book_path = v.to_string();
                    // Try to load the new book
                    match PolyglotBook::load(&self.book_path) {
                        Ok(b) => {
                            println!("info string Opening book loaded: {} ({} entries)", b.desc, b.len());
                            self.book = Some(b);
                        }
                        Err(e) => {
                            println!("info string Failed to load book {}: {:?}", self.book_path, e);
                            self.book = None;
                        }
                    }
                }
            }
            _ => {
                if self.debug {
                    eprintln!("Unknown option: {}", name);
                }
            }
        }
    }

    fn cmd_ucinewgame(&mut self) {
        // Preserve NNUE model before resetting
        let nnue_model = self.searcher.nnue.take();
        
        self.board = Board::default();
        self.searcher = Searcher::new();
        
        // Restore NNUE model
        self.searcher.nnue = nnue_model;
    }

    fn cmd_position(&mut self, fen: Option<&str>, moves: &[String]) {
        // Set up the position
        self.board = match fen {
            Some(f) => Board::from_str(f).unwrap_or_default(),
            None => Board::default(),
        };

        // Track position hashes for repetition detection
        let mut history: Vec<u64> = Vec::with_capacity(moves.len() + 1);
        history.push(self.board.get_hash());

        // Apply moves
        for move_str in moves {
            if let Some(m) = parse_move(&self.board, move_str) {
                self.board = self.board.make_move_new(m);
                history.push(self.board.get_hash());
            } else if self.debug {
                eprintln!("Invalid move: {}", move_str);
            }
        }
        
        // Store history in searcher for repetition detection
        self.searcher.set_position_with_history(self.board, history);
    }

    fn cmd_go(&mut self, params: SearchParams) {
        // Try opening book first (unless infinite or analysis mode)
        if self.use_own_book && !params.infinite && params.searchmoves.is_empty() {
            if let Some(ref book) = self.book {
                if let Some(book_move) = book.probe_move(&self.board) {
                    self.send(&format!("info string book move"));
                    self.send(&format!("bestmove {}", format_move(book_move)));
                    return;
                }
            }
        }

        // Set up search limits with move overhead
        let limits = SearchLimits::from_params(&params)
            .with_move_overhead(self.move_overhead);
        
        // Set position and run search
        self.searcher.set_position(self.board);
        let result = self.searcher.search(limits);

        // Send info
        let stats = result.stats;
        let pv_str: String = result.pv.iter()
            .map(|m| format_move(*m))
            .collect::<Vec<_>>()
            .join(" ");

        self.send(&format!(
            "info depth {} seldepth {} score {} nodes {} nps {} time {} pv {}",
            stats.depth.raw(),
            stats.seldepth.raw(),
            result.score,
            stats.nodes,
            stats.nps(),
            stats.time_ms,
            pv_str
        ));

        // Send best move
        match result.best_move {
            Some(m) => self.send(&format!("bestmove {}", format_move(m))),
            None => self.send("bestmove 0000"),
        }
    }

    fn cmd_stop(&mut self) {
        self.searcher.stop();
    }

    fn cmd_ponderhit(&mut self) {
        // TODO: Switch from pondering to normal search
    }

    fn cmd_quit(&mut self) {
        self.quit = true;
    }

    fn cmd_display(&self) {
        // Non-standard debug command to display the board
        eprintln!("{}", self.board);
        eprintln!("FEN: {}", self.board);
        eprintln!("Side to move: {:?}", self.board.side_to_move());
    }
}

/// Info message builder for search output
#[allow(dead_code)]
pub struct InfoBuilder {
    parts: Vec<String>,
}

#[allow(dead_code)]
impl InfoBuilder {
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    pub fn depth(mut self, d: i32) -> Self {
        self.parts.push(format!("depth {}", d));
        self
    }

    pub fn seldepth(mut self, d: i32) -> Self {
        self.parts.push(format!("seldepth {}", d));
        self
    }

    pub fn score(mut self, s: Score) -> Self {
        self.parts.push(format!("score {}", s));
        self
    }

    pub fn nodes(mut self, n: u64) -> Self {
        self.parts.push(format!("nodes {}", n));
        self
    }

    pub fn nps(mut self, n: u64) -> Self {
        self.parts.push(format!("nps {}", n));
        self
    }

    pub fn time(mut self, ms: u64) -> Self {
        self.parts.push(format!("time {}", ms));
        self
    }

    pub fn pv(mut self, moves: &[Move]) -> Self {
        if !moves.is_empty() {
            let pv_str: Vec<String> = moves.iter().map(|m| format_move(*m)).collect();
            self.parts.push(format!("pv {}", pv_str.join(" ")));
        }
        self
    }

    pub fn hashfull(mut self, permill: u32) -> Self {
        self.parts.push(format!("hashfull {}", permill));
        self
    }

    pub fn build(self) -> String {
        format!("info {}", self.parts.join(" "))
    }
}

impl Default for InfoBuilder {
    fn default() -> Self {
        Self::new()
    }
}
