//! UCI command handler and main loop.

use super::parser::{parse_command, UciCommand};
use super::{parse_move, format_move, SearchParams, ENGINE_NAME, ENGINE_AUTHOR};
use crate::types::{Board, Move, Score};
use crate::search::{Searcher, SearchLimits};
use std::io::{self, BufRead, Write};
use std::str::FromStr;

/// UCI protocol handler
pub struct UciHandler {
    /// Current board position
    board: Board,
    /// Search engine
    searcher: Searcher,
    /// Debug mode enabled
    debug: bool,
    /// Should the engine quit
    quit: bool,
}

impl Default for UciHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl UciHandler {
    pub fn new() -> Self {
        Self {
            board: Board::default(),
            searcher: Searcher::new(),
            debug: false,
            quit: false,
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
        
        // Send options here
        // self.send("option name Hash type spin default 16 min 1 max 1024");
        
        self.send("uciok");
    }

    fn cmd_debug(&mut self, on: bool) {
        self.debug = on;
    }

    fn cmd_isready(&self) {
        self.send("readyok");
    }

    fn cmd_setoption(&mut self, _name: &str, _value: Option<&str>) {
        // TODO: Handle options like Hash size, Threads, etc.
    }

    fn cmd_ucinewgame(&mut self) {
        self.board = Board::default();
        self.searcher = Searcher::new();
        // TODO: Clear transposition table, history, etc.
    }

    fn cmd_position(&mut self, fen: Option<&str>, moves: &[String]) {
        // Set up the position
        self.board = match fen {
            Some(f) => Board::from_str(f).unwrap_or_default(),
            None => Board::default(),
        };

        // Apply moves
        for move_str in moves {
            if let Some(m) = parse_move(&self.board, move_str) {
                self.board = self.board.make_move_new(m);
            } else if self.debug {
                eprintln!("Invalid move: {}", move_str);
            }
        }
    }

    fn cmd_go(&mut self, params: SearchParams) {
        // Set up search limits
        let limits = SearchLimits::from_params(&params);
        
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
