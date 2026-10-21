# Grill-Me Result
## Questions and Answers

**Q1 - Platform/form factor**: CLI-style game, with a TUI (see Q18). Runs on one machine.
**Q2 - Player setup**: Hot-seat on one computer, passing the mic. No networking.
**Q3 - Target OSes**: macOS, Linux and Windows, built and run natively (see Q5/Q11).
**Q4 - Scoring**: No automatic scoring. Players judge by listening.
**Q5 / Q11 - Docker**: Dropped after the user learned Docker cannot access host audio on macOS/Windows without bridges. The game runs natively via `cargo run` / release binaries. No Docker, CI or CD.
**Q6 - Turn flow**: Player A records, the app reverses the clip, and the clip is replayable. Player B records an imitation of the reversed clip, the app reverses that attempt, and everyone listens to the original and the re-reversed attempt. Any clip can be replayed on demand.
**Q7 - Rounds/roles**: Endless rounds. The app does not enforce roles; players decide who plays and when to swap. `q` quits.
**Q8 - Recording control**: Start and stop with a key press (Enter/Space), with a max length cap.
**Q9 - Playback options**: Replay the original, the reversed clip, the attempt, and the re-reversed attempt. No slow-down.
**Q10 - Persistence**: None. Audio lives in memory only for the current round and is discarded afterwards. No save flag.
**Q12 - Audio library**: No answer from the user; decided as `cpal` for input and output. Reversal is done on in-memory `f32` buffers.
**Q13 - Audio format/devices**: Default devices, mono, native sample rate, f32 internally. Optional device selection via config (and a list-devices option).
**Q14 - Project shape**: A single crate with `lib.rs` (core logic) and `main.rs` (TUI/CLI), stable Rust.
**Q15 - Testing**: Not needed. No automated tests, CI or CD.
**Q16 - Recording edge cases**: Accept every recording. Players listen and can re-record.
**Q17 - Max length**: Default 10 seconds, defined in the config file.
**Q18 - UI**: TUI. Interpreted as `ratatui` + `crossterm`.
**Q19 - Players/names**: No names. Flow supports 1 or 2+ players, with no enforced turn rules.
**Q20 - Win tracking**: None.
**Q21 - Config**: `./config/config.toml` (TOML), with a committed `config-example.toml`.
**Q22 - Config contents**: Left to my discretion: `max_seconds`, `input_device`, `output_device`.
**Q23 - Silence trimming**: None. Audio is used exactly as recorded.
**Q24 - TUI flow**: Accepted as proposed: a single persistent screen showing the current step, a clip list and a key legend. Steps: A records (Space/Enter start and stop, timer and meter), A can replay the original (`o`) and re-record (`r`), Enter confirms, the reversed clip plays (replay with `v`), B records an attempt, the attempt is reversed, all four clips are replayable, and `n` starts a new round.
**Q25 - Level display**: A volume bar plus timer. A scrolling waveform (e.g. ratatui `Sparkline`) is used only if it turns out to be simple.
**Q26 - Device errors/name**: A clear error message and exit when no mic or speaker is found. Project name is `reverse-game`.

## Implementation Summary

- Rust project `reverse-game`: a single crate with a library (core logic) and a binary (TUI).
- Audio uses `cpal` for capture and playback. Recordings are mono `f32` buffers at the device's native sample rate, held in memory only.
- The round flow is: record original, reverse, play reversed, record attempt, reverse the attempt, then replay any of the four clips. `n` starts a new round and `q` quits.
- Recording starts and stops with a key press and is capped at `max_seconds`.
- The TUI uses `ratatui` and `crossterm`: one screen with the current step, clip list, key legend, timer and a volume bar.
- Config is TOML at `./config/config.toml` with a `config-example.toml`. Keys: `max_seconds` (default 10), `input_device` and `output_device` (optional names). Defaults apply when the file is missing.
- No scoring, names, persistence, trimming, tests, Docker, CI or CD.
- Platform notes: macOS asks for microphone permission for the terminal on first run. Linux builds need ALSA dev headers (`libasound2-dev`). Windows needs no extra setup.
- Startup fails with a clear message if no input or output device is available.

## Open Decisions
- **Scrolling waveform vs volume bar (Q25)**: Default is volume bar plus timer. Consequence: the waveform is a stretch goal, decided during implementation if it proves trivial.
- **Exact key bindings (Q24)**: Tentative (`o`, `r`, `v`, `n`, `q`, Enter/Space). The spec should finalize the list of keys for the four replay actions (the attempt and re-reversed clips need keys too).
