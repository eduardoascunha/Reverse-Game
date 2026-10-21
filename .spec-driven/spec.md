# Spec

## What to Build

`reverse-game`: a hot-seat, terminal (TUI) party game written in stable Rust. Players take turns speaking into the mic. The app reverses the recording; the next player imitates the reversed sound; the app reverses that imitation so everyone can compare it with the original by ear.

Scope:
- Single crate with a library (`src/lib.rs`, core logic) and a binary (`src/main.rs`, TUI).
- Runs natively on macOS, Linux and Windows (`cargo run` / release binaries). One machine, no networking.
- Audio capture and playback through `cpal`, using the default devices (or devices named in config). Mono `f32` buffers at the device's native sample rate, held in memory only.
- One persistent TUI screen (`ratatui` + `crossterm`) showing the current step, a clip list, a timer, a volume bar and a key legend.
- Endless rounds. No scoring, names, win tracking, persistence, silence trimming, slow-down playback, tests, Docker, CI or CD.
- Config: `./config/config.toml` (TOML). Commit `config/config-example.toml`. Defaults apply when the file is missing.
- Startup fails with a clear message and non-zero exit if no input or output device is found.

Round flow (the "steps"):
1. `RecordOriginal`: player A records. Space/Enter starts, Space/Enter stops (or the cap auto-stops).
2. `ReviewOriginal`: A can replay the original (`o`), re-record (`r`), or confirm (Enter). Confirming computes the reversed clip.
3. `PlayReversed`: the reversed clip plays automatically once. `v` replays it. Enter moves on to the attempt.
4. `RecordAttempt`: player B records an imitation. Space/Enter starts and stops. The app then computes the re-reversed attempt.
5. `Results`: all four clips are replayable. `n` starts a new round (discards all audio), `q` quits.

## How to Build It

### 1. Project setup
1. `cargo init --name reverse-game` (edition 2021 or newer). Do not add a `tests/` dir or CI files.
2. Dependencies (resolve the latest compatible versions at implementation time; at spec time these were `cpal 0.18`, `ratatui 0.30`, `crossterm 0.29`):
   - `cpal`, `ratatui`, `serde` (feature `derive`), `toml`, `anyhow`.
   - Use the `crossterm` re-exported by ratatui (`ratatui::crossterm`) to avoid a version mismatch. Add `crossterm` directly only if it is not re-exported.
3. Check the `cpal` docs for the resolved version before writing the stream code. The device and stream-builder APIs changed between releases. Do not assume the 0.15 API.
4. Add `.gitignore` with `/target` and `/config/config.toml` (the user's local copy). Keep `config/config-example.toml` committed.

### 2. Module layout
```
src/lib.rs        pub mod config; pub mod audio; pub mod round;
src/config.rs     Config + loader
src/audio.rs      device lookup, Recorder, Player
src/round.rs      Clip, Step, Round state machine, reverse()
src/main.rs       terminal setup, event loop, rendering
```
Keep `lib.rs` free of terminal code. `round.rs` must not depend on `cpal`, so it can be reasoned about on its own.

### 3. Config (`src/config.rs`)
```rust
#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    pub max_seconds: u32,            // default 10, must be >= 1
    pub input_device: Option<String>,
    pub output_device: Option<String>,
}
```
- `Config::load()` reads `./config/config.toml`. Missing file gives `Config::default()`. A file that exists but fails to parse gives an error naming the path and the parse problem, and the program exits. Do not silently fall back.
- `max_seconds == 0` gives a config error.
- Unknown keys are ignored (serde default behaviour).
- `config/config-example.toml`:
  ```toml
  # Maximum length of one recording, in seconds.
  max_seconds = 10

  # Optional. Exact device names as printed by `reverse-game --list-devices`.
  # When omitted, the system default device is used.
  # input_device = "MacBook Pro Microphone"
  # output_device = "MacBook Pro Speakers"
  ```

### 4. CLI arguments (`src/main.rs`)
Parse `std::env::args` by hand, with no `clap`:
- `--list-devices`: print the input and output device names (marking the defaults) and exit 0.
- `--help` / `-h`: print usage and exit 0.
- Any other argument: print usage and exit 2.

### 5. Audio (`src/audio.rs`)
**Device selection.** `select_input(&Config)` and `select_output(&Config)` use the host's default device, or the device whose name matches the config exactly.
- Named device not found: error `input device "X" not found; run with --list-devices`.
- No default device and none configured: error `no microphone found` / `no speaker found`.
- Run both lookups before entering the TUI, so errors print to a normal terminal and the process exits with code 1.

**Clip type.**
```rust
pub struct Clip { pub samples: Vec<f32>, pub sample_rate: u32 }
```
`Clip::duration()` returns seconds as `f32`.

**Recording.**
- Use the input device's default input config. Whatever the native sample format (`f32`, `i16`, `u16`), convert to `f32` in the callback.
- Downmix to mono by averaging the frames' channels.
- `Recorder::start(device, max_samples) -> Recorder` builds and plays the input stream. Samples go into an `Arc<Mutex<Vec<f32>>>` (pre-allocated to `max_samples`) and stop being pushed once the cap is reached.
- Share live state with the UI through atomics. The callback writes `level` (peak of the latest buffer, stored as `f32` bits in an `AtomicU32`) and `captured` (`AtomicUsize` sample count). The UI reads them each frame for the volume bar and timer.
- `Recorder::stop(self) -> Clip` drops the stream and returns the buffer.
- The UI treats `captured >= max_samples` as auto-stop and finishes the recording exactly as a key press would.
- The stream error callback stores the message in a shared `Mutex<Option<String>>`. The event loop shows it in the status line and returns to the previous step.
- An empty clip (zero samples) is still accepted, per "accept every recording". Playback of it is a no-op.

**Playback.**
- Use the output device's default output config. Resample the clip to the output rate with linear interpolation if `clip.sample_rate != out_rate`. Write each mono sample to every output channel. Convert to the output sample format.
- `Player::play(&mut self, clip: &Clip)` stops any current playback, then builds an output stream that reads from an `Arc<Vec<f32>>` with a position counter. Past the end it writes silence and sets a `finished` `AtomicBool`.
- `Player::is_playing()` reads that flag, so the UI can show "playing" and the legend can stay accurate. Starting a new clip while one plays cuts the old one off.
- Playback is non-blocking. The event loop never waits for audio.
- Dropping the stream stops playback.

### 6. Round logic (`src/round.rs`)
```rust
pub fn reverse(clip: &Clip) -> Clip   // samples.iter().rev().copied().collect(), same sample_rate

pub enum Step { RecordOriginal, ReviewOriginal, PlayReversed, RecordAttempt, Results }

pub struct Round {
    pub step: Step,
    pub original: Option<Clip>,
    pub reversed: Option<Clip>,
    pub attempt: Option<Clip>,
    pub rereversed: Option<Clip>,
}
```
Transitions (`Round` methods; the audio layer and UI call them):
- `finish_original(clip)`: sets `original`, step becomes `ReviewOriginal`.
- `rerecord()`: clears `original`, step becomes `RecordOriginal`.
- `confirm_original()`: sets `reversed = reverse(original)`, step becomes `PlayReversed`. The caller plays `reversed` right away.
- `begin_attempt()`: step becomes `RecordAttempt`.
- `finish_attempt(clip)`: sets `attempt` and `rereversed = reverse(attempt)`, step becomes `Results`.
- A new round is `Round::new()` (all clips dropped, step is `RecordOriginal`).

Because the audio is in memory only, nothing is written to disk anywhere.

### 7. TUI (`src/main.rs`)
**Terminal lifecycle.**
- Enable raw mode and the alternate screen (`ratatui::init()` / `ratatui::restore()` if available in the resolved version, otherwise the crossterm equivalents).
- Install a panic hook that restores the terminal first, so a panic does not leave the shell broken.
- Always restore on exit, including on error return.

**Event loop.** Poll input with `event::poll(Duration::from_millis(50))`, redraw each iteration so the timer and bar are live. Only handle `KeyEventKind::Press` (Windows reports press and release). Ctrl+C also quits.

**Layout** (vertical, top to bottom):
1. Title bar: `reverse-game`.
2. Step panel: a bordered block with the step title and one line of instructions (see the table below). While recording, it also shows `● REC  00:03.2 / 00:10.0` and a `Gauge` volume bar bound to `level` (clamp to 0..1; apply a mild boost such as `sqrt(level)` so quiet speech is visible).
3. Clip list: a bordered block with four rows: `1 Original`, `2 Reversed`, `3 Attempt`, `4 Re-reversed`. Each row shows the duration if the clip exists, or `—` if not. The row currently playing shows `▶`.
4. Status line: the last error or notice (for example "Mic stream error: …"), blank otherwise.
5. Key legend: only keys valid in the current step.

**Key bindings (final)** (AI decision: the open decision asked for the replay keys of the attempt and re-reversed clips. Use the digits `1`-`4`, which match the clip list rows and are valid in every step where that clip exists. Keep the letter mnemonics `o`, `v`, `r`, `n`, `q` from the grill-me flow as aliases for the steps that call for them, since they were already agreed. Digits give a uniform scheme and need no new mnemonics for the attempt and re-reversed clips):

| Step | Keys |
|---|---|
| RecordOriginal | `Space`/`Enter` start, then `Space`/`Enter` stop. `q` quits (when not recording). |
| ReviewOriginal | `o` or `1` replay original. `r` re-record. `Enter` confirm (reverse). `q` quits. |
| PlayReversed | `v` or `2` replay reversed. `1` replay original. `Enter` continue to attempt. `q` quits. |
| RecordAttempt | `Space`/`Enter` start, then `Space`/`Enter` stop. `1` and `2` replay the original and reversed clips (when not recording) so B can listen again before recording. `q` quits (when not recording). |
| Results | `1` original, `2` reversed, `3` attempt, `4` re-reversed. `n` new round. `q` quits. |

Rules:
- `q` during a recording is ignored, and the legend does not list it (Ctrl+C still exits). This avoids an accidental quit mid-take.
- Pressing a replay key while recording does nothing.
- Pressing a replay key for a clip that does not exist does nothing.
- Starting any playback cuts off the current playback.
- Starting a recording stops any playback first, so the mic does not capture speaker output from the app.

**Step panel text.**

| Step | Title | Instruction |
|---|---|---|
| RecordOriginal | Player A: record | Press Space to start, Space again to stop. |
| ReviewOriginal | Player A: review | Listen with `o`, re-record with `r`, or press Enter to reverse it. |
| PlayReversed | Reversed! | Listen with `v`. Player B: when ready, press Enter to record your imitation. |
| RecordAttempt | Player B: imitate | Imitate the reversed sound. Space to start, Space to stop. |
| Results | Compare | Play 1–4 to compare the original and the re-reversed attempt. `n` for a new round. |

Players are not named. The panel text "Player A/B" is a hint only and the app enforces no turns. Any number of players can rotate between rounds.

**Scrolling waveform.** Not part of the baseline. (AI decision: the grill-me result made the volume bar plus timer the default and the waveform a stretch goal only "if it proves trivial". A `Sparkline` needs a ring buffer of recent peaks fed by the audio callback, plus another shared structure. It is not required to play the game, so the spec leaves it out. It can be added later.)

### 8. Error handling
- Config parse error, missing device, or failure to build a stream at startup: print `error: <message>` to stderr, exit code 1, before the TUI starts.
- Stream failure when a recording starts (for example the mic was unplugged or permission denied): show a status message and stay in the same step. Do not crash.
- On macOS, the first mic use triggers a permission prompt for the terminal app. If the stream yields only silence or fails to start, the status line should suggest: "Check microphone permission for your terminal." (README covers this too.)
- Mutex poisoning in the audio buffers: use `lock().unwrap_or_else(|e| e.into_inner())` so a callback panic does not cascade.

### 9. Compatibility and platform notes
- Linux: needs ALSA dev headers (`libasound2-dev` on Debian/Ubuntu) to build.
- macOS: terminal needs microphone permission.
- Windows: nothing extra. Use the `KeyEventKind::Press` filter noted above.
- Keep all code on stable Rust. Do not use `unsafe`, nightly features, or platform-specific crates.

### 10. Documentation
Write a short `README.md`: what the game is, how to play (the round flow and key table), `cargo run`, `--list-devices`, config file location and keys, platform notes from section 9, and a line saying nothing is recorded to disk.

### 11. Manual verification (no automated tests)
After implementing, check these by hand:
- `cargo build` and `cargo clippy` are clean.
- `cargo run -- --list-devices` lists devices.
- Complete one full round: record, replay, re-record, confirm, hear the reversed clip, record an attempt, play all four clips, `n`, then `q`.
- A 10 s recording auto-stops at the cap. Changing `max_seconds` in `config/config.toml` changes the cap.
- A bad `input_device` name gives a clear error and exit code 1.
- The terminal is restored after `q`, Ctrl+C and a forced error.
