







# Chess Engine (Rust)

A chess engine built in Rust using:
- [`chess`](https://crates.io/crates/chess) for move generation.
- [`nnue`](https://github.com/analog-hors/nnue-rs) for evaluation.

## Setup

1. **Install Rust**: Ensure you have Rust installed.
2. **Download NNUE Network**:
   - The engine requires an NNUE network file to run its evaluation.
   - Download a Stockfish NNUE file (e.g., from [Stockfish NNUE files](https://tests.stockfishchess.org/nns)).
   - Rename the file to `network.nnue` and place it in the root of this project.

## Running

To run the verification tests:

```sh
cargo run
```

This will:
1. Generate moves for the starting position (checking `chess` crate).
2. Attempt to load `network.nnue` (checking `nnue` crate).
