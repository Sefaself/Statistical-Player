# Statistical Player
Desktop music player in Rust (eframe/egui, symphonia, cpal, rusqlite).
Read docs/ROADMAP.md, then the current phase file in docs/phases/. Work only on that phase.

## Commands
- Run: cargo run
- Check: cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test

## Hard rules
- Never allocate, lock, log or do I/O inside the audio callback.
- Never modify the owner's music files.
- No unsafe code. No new dependency without a note in docs/DECISIONS.md.
- Verify crate APIs against docs.rs for the version you add; do not rely on memory.
- Run the app before saying a task is done, and say what you could not verify.
- The owner is new to Rust: keep code simple and explain new concepts briefly.
