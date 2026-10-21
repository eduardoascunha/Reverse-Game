# Implementation Tasks

## Task 1: Create startup and configuration flow

**Status:** done
**Depends on:** none
**Blocked by:** none

### What to implement

Create the stable-Rust `reverse-game` library and binary project with the required module layout and dependencies. The program must load `./config/config.toml` with defaults when absent, reject malformed or zero-length configuration with a clear error, select configured or default input and output devices, and provide the complete `--help`, `--list-devices`, and invalid-argument command-line behavior before the TUI starts.

### How to implement

1. Add the Cargo manifest, library module declarations, binary entry point, `.gitignore`, and committed `config/config-example.toml`.
2. Implement `Config`, its defaults, TOML loading, and validation in `src/config.rs`; preserve serde's default unknown-key behavior.
3. Implement input/output device discovery and exact-name selection in `src/audio.rs`, including the specified missing-device errors and formatting for the device listing.
4. Parse arguments manually in `main.rs`; load config and resolve both devices before starting the interactive application, reporting startup failures to stderr with exit code 1.
5. Check the resolved `cpal` documentation and use its current device enumeration APIs rather than assuming an older stream API.

### Verify

- `cargo build` succeeds.
- `cargo run -- --help` exits 0 and prints usage; an unknown argument exits 2 and prints usage.
- `cargo run -- --list-devices` prints available input and output devices with defaults marked.
- A malformed config, `max_seconds = 0`, and a nonexistent configured device each produce the specified clear startup error and exit code 1.

### Checks

- `cargo build` passed.
- `cargo clippy -- -D warnings` passed.
- `--help`, invalid arguments, and `--list-devices` returned the specified exit codes; device listing succeeded on this machine.
- Malformed TOML, `max_seconds = 0`, and an unknown input device returned clear errors with exit code 1.
- The corrected Rust minimum passed `cargo build` and `cargo clippy -- -D warnings`.

## Task 2: Implement the round state machine

**Status:** done
**Depends on:** Task 1
**Blocked by:** none

### What to implement

Provide the in-memory clip model, sample reversal, and complete round progression from recording an original through comparison results. This logic must have no `cpal` or terminal dependency and must discard all clips when a new round begins.

### How to implement

1. Define `Clip` with its sample rate and duration calculation in `src/round.rs`.
2. Implement `reverse` by reversing samples while retaining the input sample rate.
3. Define `Step` and `Round`, plus the specified finish, re-record, confirm, begin-attempt, and new-round transitions.
4. Re-export or reference the shared clip type consistently so the future audio and TUI layers use the same data model.

### Verify

- `cargo build` succeeds.
- A focused manual or temporary in-crate check confirms every transition sets the intended step and clips, reversal preserves sample rate, and `Round::new()` contains no audio.

### Checks

- `cargo build` passed.
- A temporary in-crate check passed every round transition, clip reversal, duration calculation, and new-round clearing; it was removed to preserve the specified test-free project.
- `cargo clippy -- -D warnings` passed.

## Task 3: Implement microphone capture and playback

**Status:** done
**Depends on:** Task 1
**Blocked by:** none

### What to implement

Capture microphone audio as capped, mono `f32` clips and play clips asynchronously through the selected output device. The implementation must support native device sample formats, resampling, live volume/timer telemetry, stream errors, empty clips, and immediate replacement of active playback.

### How to implement

1. Build `Recorder` from the selected input device's default configuration, converting `f32`, `i16`, and `u16` callbacks to averaged mono samples in a preallocated shared buffer.
2. Expose atomic captured-sample and peak-level state, plus a poison-tolerant shared stream-error message, and return the captured clip when the recorder is stopped or reaches its cap.
3. Build `Player` from the selected output device's default configuration; linearly resample when needed, duplicate mono samples to each channel, convert to the native output format, and output silence after completion.
4. Keep playback non-blocking, expose playing/finished state, make empty clips no-ops, and ensure starting another clip stops the old stream.
5. Surface stream-build and runtime failures in a form that the TUI can report without crashing.

### Verify

- `cargo build` and `cargo clippy` are clean.
- On a machine with audio devices, record a short clip and verify its sample count, timer/level telemetry, and playback complete without blocking the caller.
- Confirm an empty clip is accepted, starting playback replaces existing playback, and a recording stops accepting samples at its configured cap.

### Checks

- `cargo build` passed.
- `cargo clippy -- -D warnings` passed.
- A temporary live-device probe captured 480 microphone samples, started non-blocking playback, and was removed after validation.
- A temporary resampling check confirmed that downsampling preserves a one-sample clip; it was removed after validation.
- A temporary live-device probe confirmed the recording cap, playback replacement, and playback completion; it was removed after validation.

## Task 4: Build the terminal game loop

**Status:** done
**Depends on:** Task 1, Task 2, Task 3
**Blocked by:** none

### What to implement

Deliver the single-screen hot-seat game: all five round steps, automatic reversed playback, replay keys, recording controls, live timer and volume bar, clip status rows, status messages, and reliable terminal restoration on every exit path.

### How to implement

1. Add RAII-equivalent terminal lifecycle handling in `main.rs`: raw mode, alternate screen, panic restoration, and restoration after normal quit or returned errors, using the resolved ratatui/crossterm APIs.
2. Run a 50 ms polling/redraw event loop that accepts only `KeyEventKind::Press`, handles Ctrl+C, and coordinates the `Round`, `Recorder`, and `Player` state.
3. Implement the specified recording start/stop and auto-stop paths, including stopping playback before recording, ignoring replay and `q` while recording, and reporting mic errors without changing the current step.
4. Render the title, bordered step panel, recording meter, four-row clip list with active playback indicator, status line, and step-specific legend.
5. Implement every listed key binding and alias, including automatic play on confirmation, universally numbered replay keys when clips exist, results reset with `n`, and quit with `q` outside recordings.

### Verify

- `cargo build` and `cargo clippy` are clean.
- Complete one full round manually: record and replay the original, re-record it, confirm and hear its reversal, record an attempt, replay all four clips, begin a new round, then quit.
- Confirm the 10-second cap auto-stops recording, a changed `max_seconds` changes that cap, and the terminal is restored after `q`, Ctrl+C, and a forced returned error.

### Checks

- `cargo fmt --check`, `cargo build`, and `cargo clippy -- -D warnings` passed.
- `cargo run -- --list-devices` listed the machine's input and output devices.
- An interactive full round recorded, replayed, re-recorded, reversed, attempted, compared all four clips, reset with `n`, and quit with `q`; terminal restoration was confirmed.
- Default recordings auto-stopped at the 10-second cap.

## Task 5: Document installation and play

**Status:** done
**Depends on:** Task 4
**Blocked by:** none

### What to implement

Provide a concise README that lets a player build, configure, operate, and troubleshoot the finished game without implying recordings are persisted.

### How to implement

1. Describe the game and its original/reversed/imitation/re-reversed round flow.
2. Document `cargo run`, `--list-devices`, configuration location and keys, and the complete key table.
3. Include macOS microphone permission, Linux ALSA development-header, and Windows platform notes.
4. State clearly that no audio is written to disk.

### Verify

- README commands, paths, config keys, and key bindings match the implemented application.
- Re-run `cargo build`, `cargo clippy`, device listing, and the full manual round checklist after documentation is complete.

### Checks

- README markdown diagnostics passed.
- `cargo build` and `cargo clippy -- -D warnings` passed.
- `cargo run -- --list-devices` listed the machine's input and output devices.
- README flow, key bindings, configuration, and platform notes were checked against the implemented application and the Task 4 interactive round verification.