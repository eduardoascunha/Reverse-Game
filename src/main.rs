use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use cpal::traits::DeviceTrait;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use reverse_game::audio::{self, Player, Recorder};
use reverse_game::config::Config;
use reverse_game::round::{Clip, Round, Step};

const USAGE: &str = "Usage: reverse-game [--help | -h | --list-devices]";

enum Command {
    Help,
    ListDevices,
    Start,
}

fn main() -> ExitCode {
    match parse_command() {
        Ok(command) => match run(command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error:#}");
                ExitCode::from(1)
            }
        },
        Err(()) => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn parse_command() -> std::result::Result<Command, ()> {
    let mut arguments = std::env::args();
    let _program = arguments.next();

    match (arguments.next().as_deref(), arguments.next()) {
        (None, None) => Ok(Command::Start),
        (Some("--help" | "-h"), None) => Ok(Command::Help),
        (Some("--list-devices"), None) => Ok(Command::ListDevices),
        _ => Err(()),
    }
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::Help => println!("{USAGE}"),
        Command::ListDevices => list_devices()?,
        Command::Start => {
            let config = Config::load()?;
            let input_device = audio::select_input(&config)?;
            let output_device = audio::select_output(&config)?;
            run_game(config, input_device, output_device)?;
        }
    }

    Ok(())
}

fn list_devices() -> Result<()> {
    print_devices("Input devices", audio::input_devices()?);
    print_devices("Output devices", audio::output_devices()?);
    Ok(())
}

fn print_devices(heading: &str, devices: Vec<audio::DeviceInfo>) {
    println!("{heading}:");
    for device in devices {
        let default_marker = if device.is_default { " (default)" } else { "" };
        println!("  {}{default_marker}", device.name);
    }
}

struct App {
    round: Round,
    input_device: cpal::Device,
    input_sample_rate: u32,
    max_samples: usize,
    max_seconds: u32,
    recorder: Option<Recorder>,
    player: Player,
    playing_clip: Option<usize>,
    status: String,
}

impl App {
    fn new(
        input_device: cpal::Device,
        input_sample_rate: u32,
        max_samples: usize,
        max_seconds: u32,
        player: Player,
    ) -> Self {
        Self {
            round: Round::new(),
            input_device,
            input_sample_rate,
            max_samples,
            max_seconds,
            recorder: None,
            player,
            playing_clip: None,
            status: String::new(),
        }
    }

    fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    fn update_audio(&mut self) {
        let recorder_error = self
            .recorder
            .as_ref()
            .and_then(|recorder| take_error(&recorder.stream_error));
        if let Some(error) = recorder_error {
            self.recorder = None;
            self.status = format!(
                "Mic stream error: {error}. Check microphone permission for your terminal."
            );
        }

        let reached_cap = self
            .recorder
            .as_ref()
            .is_some_and(|recorder| recorder.captured.load(Ordering::Acquire) >= self.max_samples);
        if reached_cap {
            self.finish_recording();
        }

        if let Some(error) = take_error(&self.player.stream_error) {
            self.player.stop();
            self.playing_clip = None;
            self.status = format!("Speaker stream error: {error}");
        } else if !self.player.is_playing() {
            self.playing_clip = None;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }

        if self.is_recording() {
            if is_record_key(key.code) {
                self.finish_recording();
            }
            return Ok(false);
        }

        match self.round.step {
            Step::RecordOriginal | Step::RecordAttempt => match key.code {
                KeyCode::Char('q') => return Ok(true),
                code if is_record_key(code) => self.start_recording(),
                KeyCode::Char('1') if self.round.step == Step::RecordAttempt => self.replay(1),
                KeyCode::Char('2') if self.round.step == Step::RecordAttempt => self.replay(2),
                _ => {}
            },
            Step::ReviewOriginal => match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('o' | '1') => self.replay(1),
                KeyCode::Char('r') => {
                    self.player.stop();
                    self.playing_clip = None;
                    self.round.rerecord();
                    self.status.clear();
                }
                KeyCode::Enter => self.confirm_original(),
                _ => {}
            },
            Step::PlayReversed => match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('v' | '2') => self.replay(2),
                KeyCode::Char('1') => self.replay(1),
                KeyCode::Enter => {
                    self.player.stop();
                    self.playing_clip = None;
                    self.round.begin_attempt();
                    self.status.clear();
                }
                _ => {}
            },
            Step::Results => match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('1'..='4') => {
                    if let KeyCode::Char(digit) = key.code {
                        self.replay(digit.to_digit(10).expect("digit key") as usize);
                    }
                }
                KeyCode::Char('n') => {
                    self.player.stop();
                    self.playing_clip = None;
                    self.round = Round::new();
                    self.status.clear();
                }
                _ => {}
            },
        }

        Ok(false)
    }

    fn start_recording(&mut self) {
        self.player.stop();
        self.playing_clip = None;

        match Recorder::start(&self.input_device, self.max_samples) {
            Ok(recorder) => {
                self.recorder = Some(recorder);
                self.status.clear();
            }
            Err(error) => {
                self.status = format!(
                    "Mic stream error: {error:#}. Check microphone permission for your terminal."
                );
            }
        }
    }

    fn finish_recording(&mut self) {
        let Some(recorder) = self.recorder.take() else {
            return;
        };
        let clip = recorder.stop();

        match self.round.step {
            Step::RecordOriginal => self.round.finish_original(clip),
            Step::RecordAttempt => self.round.finish_attempt(clip),
            _ => {}
        }
    }

    fn confirm_original(&mut self) {
        self.round.confirm_original();
        self.replay(2);
    }

    fn replay(&mut self, index: usize) {
        let clip = self.clip(index).cloned();
        let Some(clip) = clip else {
            return;
        };

        match self.player.play(&clip) {
            Ok(()) => {
                self.playing_clip = self.player.is_playing().then_some(index);
                self.status.clear();
            }
            Err(error) => {
                self.playing_clip = None;
                self.status = format!("Speaker stream error: {error:#}");
            }
        }
    }

    fn clip(&self, index: usize) -> Option<&Clip> {
        match index {
            1 => self.round.original.as_ref(),
            2 => self.round.reversed.as_ref(),
            3 => self.round.attempt.as_ref(),
            4 => self.round.rereversed.as_ref(),
            _ => None,
        }
    }
}

fn run_game(config: Config, input_device: cpal::Device, output_device: cpal::Device) -> Result<()> {
    let input_sample_rate = input_device
        .default_input_config()
        .context("failed to get microphone configuration")?
        .sample_rate();
    let max_samples = (input_sample_rate as usize)
        .checked_mul(config.max_seconds as usize)
        .context("recording length is too large")?;
    let player = Player::new(output_device)?;
    let mut app = App::new(
        input_device,
        input_sample_rate,
        max_samples,
        config.max_seconds,
        player,
    );
    let mut terminal = ratatui::init();
    install_panic_hook();
    let result = run_event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        app.update_audio();
        terminal.draw(|frame| render(frame, app))?;

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && app.handle_key(key)?
        {
            return Ok(());
        }
    }
}

fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        ratatui::restore();
        previous_hook(panic_info);
    }));
}

fn render(frame: &mut Frame, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new("reverse-game").style(Style::default().fg(Color::Cyan)),
        sections[0],
    );
    render_step(frame, sections[1], app);
    render_clips(frame, sections[2], app);
    frame.render_widget(
        Paragraph::new(app.status.as_str()).style(Style::default().fg(Color::Red)),
        sections[3],
    );
    frame.render_widget(Paragraph::new(key_legend(app)), sections[4]);
}

fn render_step(frame: &mut Frame, area: Rect, app: &App) {
    let (title, instruction) = step_copy(app.round.step);
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(instruction), line_rect(inner, 0));

    if let Some(recorder) = &app.recorder {
        let elapsed =
            recorder.captured.load(Ordering::Acquire) as f32 / app.input_sample_rate as f32;
        let level = f32::from_bits(recorder.level.load(Ordering::Acquire))
            .clamp(0.0, 1.0)
            .sqrt();
        frame.render_widget(
            Paragraph::new(format!(
                "● REC  {} / {}",
                format_duration(elapsed),
                format_duration(app.max_seconds as f32)
            ))
            .style(Style::default().fg(Color::Red)),
            line_rect(inner, 2),
        );
        frame.render_widget(
            Gauge::default()
                .block(Block::default().title("Volume"))
                .gauge_style(Style::default().fg(Color::Green))
                .ratio(level as f64),
            Rect {
                x: inner.x,
                y: inner.y.saturating_add(3),
                width: inner.width,
                height: inner.height.saturating_sub(3).min(2),
            },
        );
    }
}

fn render_clips(frame: &mut Frame, area: Rect, app: &App) {
    let rows = [
        (1, "Original"),
        (2, "Reversed"),
        (3, "Attempt"),
        (4, "Re-reversed"),
    ]
    .into_iter()
    .map(|(index, name)| {
        let duration = app
            .clip(index)
            .map(|clip| format_duration(clip.duration()))
            .unwrap_or_else(|| "—".to_string());
        let playing = if app.playing_clip == Some(index) {
            "▶ "
        } else {
            "  "
        };
        Line::from(format!("{playing}{index} {name}: {duration}"))
    })
    .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(rows).block(Block::default().borders(Borders::ALL).title("Clips")),
        area,
    );
}

fn step_copy(step: Step) -> (&'static str, &'static str) {
    match step {
        Step::RecordOriginal => (
            "Player A: record",
            "Press Space to start, Space again to stop.",
        ),
        Step::ReviewOriginal => (
            "Player A: review",
            "Listen with o, re-record with r, or press Enter to reverse it.",
        ),
        Step::PlayReversed => (
            "Reversed!",
            "Listen with v. Player B: when ready, press Enter to record your imitation.",
        ),
        Step::RecordAttempt => (
            "Player B: imitate",
            "Imitate the reversed sound. Space to start, Space to stop.",
        ),
        Step::Results => (
            "Compare",
            "Play 1-4 to compare the original and the re-reversed attempt. n for a new round.",
        ),
    }
}

fn key_legend(app: &App) -> &'static str {
    if app.is_recording() {
        return "Space/Enter stop";
    }

    match app.round.step {
        Step::RecordOriginal => "Space/Enter start  q quit",
        Step::ReviewOriginal => "o/1 original  r re-record  Enter reverse  q quit",
        Step::PlayReversed => "v/2 reversed  1 original  Enter imitate  q quit",
        Step::RecordAttempt => "Space/Enter start  1 original  2 reversed  q quit",
        Step::Results => "1-4 replay  n new round  q quit",
    }
}

fn is_record_key(code: KeyCode) -> bool {
    matches!(code, KeyCode::Char(' ') | KeyCode::Enter)
}

fn line_rect(area: Rect, offset: u16) -> Rect {
    Rect {
        x: area.x,
        y: area.y.saturating_add(offset),
        width: area.width,
        height: area.height.saturating_sub(offset).min(1),
    }
}

fn format_duration(seconds: f32) -> String {
    let minutes = (seconds / 60.0) as u64;
    let remaining_seconds = seconds - minutes as f32 * 60.0;
    format!("{minutes:02}:{remaining_seconds:04.1}")
}

fn take_error(stream_error: &Mutex<Option<String>>) -> Option<String> {
    stream_error
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
}
