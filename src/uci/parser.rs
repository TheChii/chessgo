//! UCI command parser.

use super::SearchParams;
use crate::types::Depth;

/// Parsed UCI command
#[derive(Debug, Clone)]
pub enum UciCommand {
    /// "uci" - Initialize UCI mode
    Uci,
    /// "debug on/off"
    Debug(bool),
    /// "isready" - Synchronization
    IsReady,
    /// "setoption name X value Y"
    SetOption { name: String, value: Option<String> },
    /// "register" - Registration (we ignore this)
    Register,
    /// "ucinewgame" - New game starting
    UciNewGame,
    /// "position startpos/fen [moves ...]"
    Position { fen: Option<String>, moves: Vec<String> },
    /// "go ..." - Start searching
    Go(SearchParams),
    /// "stop" - Stop searching
    Stop,
    /// "ponderhit" - Opponent played expected move
    PonderHit,
    /// "quit" - Exit the engine
    Quit,
    /// "d" - Debug: display board (non-standard but common)
    Display,
    /// Unknown command
    Unknown(String),
}

/// Parse a UCI command string into a UciCommand
pub fn parse_command(input: &str) -> UciCommand {
    let input = input.trim();
    let mut parts = input.split_whitespace();
    
    match parts.next() {
        Some("uci") => UciCommand::Uci,
        Some("debug") => {
            let on = parts.next() == Some("on");
            UciCommand::Debug(on)
        }
        Some("isready") => UciCommand::IsReady,
        Some("setoption") => parse_setoption(&mut parts),
        Some("register") => UciCommand::Register,
        Some("ucinewgame") => UciCommand::UciNewGame,
        Some("position") => parse_position(&mut parts),
        Some("go") => parse_go(&mut parts),
        Some("stop") => UciCommand::Stop,
        Some("ponderhit") => UciCommand::PonderHit,
        Some("quit") => UciCommand::Quit,
        Some("d") => UciCommand::Display,
        _ => UciCommand::Unknown(input.to_string()),
    }
}

fn parse_setoption<'a>(parts: &mut impl Iterator<Item = &'a str>) -> UciCommand {
    let mut name = String::new();
    let mut value = None;
    let mut parsing_name = false;
    let mut parsing_value = false;

    for token in parts {
        match token {
            "name" => {
                parsing_name = true;
                parsing_value = false;
            }
            "value" => {
                parsing_name = false;
                parsing_value = true;
            }
            _ => {
                if parsing_name {
                    if !name.is_empty() {
                        name.push(' ');
                    }
                    name.push_str(token);
                } else if parsing_value {
                    let v = value.get_or_insert(String::new());
                    if !v.is_empty() {
                        v.push(' ');
                    }
                    v.push_str(token);
                }
            }
        }
    }

    UciCommand::SetOption { name, value }
}

fn parse_position<'a>(parts: &mut impl Iterator<Item = &'a str>) -> UciCommand {
    let mut fen = None;
    let mut moves = Vec::new();
    let mut parsing_moves = false;

    while let Some(token) = parts.next() {
        match token {
            "startpos" => {
                fen = None; // Use default start position
            }
            "fen" => {
                // Collect FEN string (6 parts)
                let mut fen_parts = Vec::new();
                for _ in 0..6 {
                    if let Some(part) = parts.next() {
                        if part == "moves" {
                            parsing_moves = true;
                            break;
                        }
                        fen_parts.push(part);
                    }
                }
                if !fen_parts.is_empty() {
                    fen = Some(fen_parts.join(" "));
                }
            }
            "moves" => {
                parsing_moves = true;
            }
            _ if parsing_moves => {
                moves.push(token.to_string());
            }
            _ => {}
        }
    }

    UciCommand::Position { fen, moves }
}

fn parse_go<'a>(parts: &mut impl Iterator<Item = &'a str>) -> UciCommand {
    let mut params = SearchParams::new();
    
    let tokens: Vec<&str> = parts.collect();
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i] {
            "infinite" => params.infinite = true,
            "ponder" => params.ponder = true,
            "depth" => {
                i += 1;
                if i < tokens.len() {
                    if let Ok(d) = tokens[i].parse::<i32>() {
                        params.depth = Some(Depth::new(d));
                    }
                }
            }
            "movetime" => {
                i += 1;
                if i < tokens.len() {
                    params.movetime = tokens[i].parse().ok();
                }
            }
            "wtime" => {
                i += 1;
                if i < tokens.len() {
                    params.wtime = tokens[i].parse().ok();
                }
            }
            "btime" => {
                i += 1;
                if i < tokens.len() {
                    params.btime = tokens[i].parse().ok();
                }
            }
            "winc" => {
                i += 1;
                if i < tokens.len() {
                    params.winc = tokens[i].parse().ok();
                }
            }
            "binc" => {
                i += 1;
                if i < tokens.len() {
                    params.binc = tokens[i].parse().ok();
                }
            }
            "movestogo" => {
                i += 1;
                if i < tokens.len() {
                    params.movestogo = tokens[i].parse().ok();
                }
            }
            "mate" => {
                i += 1;
                if i < tokens.len() {
                    params.mate = tokens[i].parse().ok();
                }
            }
            "nodes" => {
                i += 1;
                if i < tokens.len() {
                    params.nodes = tokens[i].parse().ok();
                }
            }
            "searchmoves" => {
                // Remaining tokens are moves
                // We'll parse them later when we have the board
                i += 1;
                while i < tokens.len() {
                    // Store as strings for now, will be parsed with board context
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    UciCommand::Go(params)
}
