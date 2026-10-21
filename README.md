# Reverse Game

`Reverse Game` is a hot-seat terminal party game. One player records a sound. The game reverses it, the next player imitates the reversal, and everyone compares the re-reversed imitation with the original.

Audio stays in memory. Nothing is written to disk.

## Run

Start the game:

```sh
cargo run
```

List microphones and speakers:

```sh
cargo run -- --list-devices
```

Use `--help` or `-h` for usage.

## Play

Each round:

1. Player A records an original sound.
2. Player A reviews it, re-records it if needed, then confirms it.
3. The reversed original plays automatically.
4. Player B records an imitation of the reversed sound.
5. Compare the original, reversed original, attempt, and re-reversed attempt. Start a new round when ready.

| Step | Keys |
| --- | --- |
| Record original | `Space` or `Enter` starts and stops. `q` quits when not recording. |
| Review original | `o` or `1` replays the original. `r` re-records. `Enter` reverses it. `q` quits. |
| Play reversed | `v` or `2` replays the reversed clip. `1` replays the original. `Enter` begins the imitation. `q` quits. |
| Record attempt | `Space` or `Enter` starts and stops. `1` replays the original and `2` the reversed clip before recording. `q` quits when not recording. |
| Results | `1` original, `2` reversed, `3` attempt, `4` re-reversed. `n` starts a new round. `q` quits. |

During recording, replay keys and `q` are ignored. Ctrl+C exits. Recordings stop at the configured maximum length.

## Configuration

The optional local config file is `./config/config.toml`. Use `config/config-example.toml` as a starting point:

```toml
# Maximum length of one recording, in seconds.
max_seconds = 10

# Optional exact names from `reverse-game --list-devices`.
# input_device = "MacBook Pro Microphone"
# output_device = "MacBook Pro Speakers"
```

Without the file, the game uses a 10-second cap and the system default microphone and speaker. `max_seconds` must be at least 1. Configured device names must exactly match `--list-devices`.

## Platform notes

- macOS: Allow microphone access when prompted. If recording fails or is silent, check the terminal app's microphone permission.
- Linux: Install ALSA development headers before building, such as `libasound2-dev` on Debian or Ubuntu.
- Windows: No additional setup is required.