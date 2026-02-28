use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tempfile::TempDir;
use wait_timeout::ChildExt;

use crate::binaries;
use crate::cli::{MicCommand, MicControlArgs, MicDaemonArgs};
use crate::config::AppConfig;
use crate::transcribe;

const RELEASE_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_DELAY: Duration = Duration::from_millis(25);
const WINDOW_RESTORE_DELAY: Duration = Duration::from_millis(50);
const TYPING_CHUNK_SIZE: usize = 2048;

#[derive(Debug, Copy, Clone)]
enum MicAction {
    Start,
    Stop,
    Toggle,
    Status,
    Shutdown,
}

impl MicAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Toggle => "toggle",
            Self::Status => "status",
            Self::Shutdown => "shutdown",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "start" => Some(Self::Start),
            "stop" => Some(Self::Stop),
            "toggle" => Some(Self::Toggle),
            "status" => Some(Self::Status),
            "shutdown" => Some(Self::Shutdown),
            _ => None,
        }
    }
}

struct ActiveRecording {
    child: Child,
    temp_dir: TempDir,
    wav_path: PathBuf,
    started_at: Instant,
    active_window: Option<String>,
}

struct CompletedRecording {
    #[allow(dead_code)]
    temp_dir: TempDir,
    wav_path: PathBuf,
    duration_secs: f32,
    active_window: Option<String>,
}

struct MicDaemonState {
    config: AppConfig,
    ffmpeg_bin: String,
    source: String,
    min_seconds: f32,
    mic_copy_to_clipboard: bool,
    clipboard_bin: String,
    xdotool_bin: String,
    dry_run: bool,
    model: Option<String>,
    threads: Option<usize>,
    language: Option<String>,
    translate: bool,
    passthrough: Vec<String>,
    active: Option<ActiveRecording>,
}

pub fn run(config: &AppConfig, command: &MicCommand) -> Result<()> {
    match command {
        MicCommand::Daemon(args) => run_daemon(config, args),
        MicCommand::Start(args) => run_control(config, args, MicAction::Start),
        MicCommand::Stop(args) => run_control(config, args, MicAction::Stop),
        MicCommand::Toggle(args) => run_control(config, args, MicAction::Toggle),
        MicCommand::Status(args) => run_control(config, args, MicAction::Status),
        MicCommand::Shutdown(args) => run_control(config, args, MicAction::Shutdown),
    }
}

fn run_daemon(config: &AppConfig, args: &MicDaemonArgs) -> Result<()> {
    let socket_path = resolve_socket_path(config, args.socket.as_ref());
    let ffmpeg_bin = args
        .ffmpeg_bin
        .clone()
        .unwrap_or_else(|| config.ffmpeg_bin.clone());
    let xdotool_bin = args
        .xdotool_bin
        .clone()
        .unwrap_or_else(|| config.xdotool_bin.clone());
    let source = args
        .source
        .clone()
        .unwrap_or_else(|| config.mic_source.clone());
    let min_seconds = args.min_seconds.unwrap_or(config.mic_min_seconds);

    if !binaries::binary_available(Path::new(&ffmpeg_bin)) {
        bail!("ffmpeg binary '{}' not found", ffmpeg_bin);
    }

    if !args.dry_run {
        let display = std::env::var("DISPLAY").unwrap_or_default();
        if display.trim().is_empty() {
            bail!("DISPLAY is not set. mic typing mode requires an X11 session");
        }

        if !binaries::binary_available(Path::new(&xdotool_bin)) {
            bail!(
                "xdotool binary '{}' not found (required for text injection)",
                xdotool_bin
            );
        }
    }

    let whisper_bin = binaries::resolve_whisper_bin(config);
    if !binaries::binary_available(&whisper_bin) {
        bail!("whisper-cli binary '{}' not found", whisper_bin.display());
    }
    ensure_whisper_runnable(&whisper_bin)?;

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create socket directory {}", parent.display()))?;
    }

    if socket_path.exists() {
        match UnixStream::connect(&socket_path) {
            Ok(_) => {
                bail!(
                    "mic daemon already running at {}",
                    socket_path.to_string_lossy()
                );
            }
            Err(_) => {
                fs::remove_file(&socket_path).with_context(|| {
                    format!(
                        "failed removing stale socket at {}",
                        socket_path.to_string_lossy()
                    )
                })?;
            }
        }
    }

    let listener = UnixListener::bind(&socket_path).with_context(|| {
        format!(
            "failed binding mic daemon socket at {}",
            socket_path.to_string_lossy()
        )
    })?;
    listener
        .set_nonblocking(true)
        .context("failed setting daemon socket nonblocking")?;

    let running = Arc::new(AtomicBool::new(true));
    let handler_flag = Arc::clone(&running);
    ctrlc::set_handler(move || {
        handler_flag.store(false, Ordering::SeqCst);
    })
    .context("failed to install Ctrl+C handler")?;

    eprintln!(
        "whisperx mic daemon listening at {}",
        socket_path.to_string_lossy()
    );
    eprintln!("trigger with: whisperx mic start|stop|toggle|status|shutdown");

    let mut state = MicDaemonState {
        config: config.clone(),
        ffmpeg_bin,
        source,
        min_seconds,
        mic_copy_to_clipboard: config.mic_copy_to_clipboard,
        clipboard_bin: config.clipboard_bin.clone(),
        xdotool_bin,
        dry_run: args.dry_run,
        model: args.model.clone(),
        threads: args.threads,
        language: args.language.clone(),
        translate: args.translate,
        passthrough: args.passthrough.clone(),
        active: None,
    };

    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _addr)) => {
                if let Err(err) = handle_client(stream, &mut state, &running) {
                    eprintln!("mic daemon client handling failed: {err:#}");
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(POLL_DELAY);
            }
            Err(err) => {
                return Err(err).context("mic daemon socket accept failed");
            }
        }
    }

    if let Some(recording) = state.active.take() {
        let _ = stop_recording(recording);
    }

    let _ = fs::remove_file(&socket_path);

    Ok(())
}

fn run_control(config: &AppConfig, args: &MicControlArgs, action: MicAction) -> Result<()> {
    let socket_path = resolve_socket_path(config, args.socket.as_ref());
    let response = send_action(&socket_path, action)?;
    println!("{response}");

    if response.starts_with("error:") {
        bail!("daemon returned error");
    }

    Ok(())
}

fn resolve_socket_path(config: &AppConfig, socket_override: Option<&PathBuf>) -> PathBuf {
    socket_override
        .cloned()
        .unwrap_or_else(|| config.mic_socket.clone())
}

fn send_action(socket_path: &Path, action: MicAction) -> Result<String> {
    let mut stream = UnixStream::connect(socket_path).with_context(|| {
        format!(
            "failed connecting to mic daemon at {} (is `whisperx mic daemon` running?)",
            socket_path.to_string_lossy()
        )
    })?;

    stream
        .write_all(format!("{}\n", action.as_str()).as_bytes())
        .context("failed writing request to mic daemon")?;
    stream.flush().context("failed flushing request")?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .context("failed reading mic daemon response")?;

    Ok(response.trim().to_string())
}

fn handle_client(
    mut stream: UnixStream,
    state: &mut MicDaemonState,
    running: &AtomicBool,
) -> Result<()> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&stream);
        reader
            .read_line(&mut line)
            .context("failed reading daemon request")?;
    }

    let request = line.trim();
    let action = MicAction::parse(request).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported daemon request '{}': use start|stop|toggle|status|shutdown",
            request
        )
    })?;

    let (response, should_shutdown) =
        process_action(state, action).unwrap_or_else(|err| (format!("error: {err:#}"), false));

    stream
        .write_all(format!("{response}\n").as_bytes())
        .context("failed writing daemon response")?;
    stream.flush().context("failed flushing daemon response")?;

    if should_shutdown {
        running.store(false, Ordering::SeqCst);
    }

    Ok(())
}

fn process_action(state: &mut MicDaemonState, action: MicAction) -> Result<(String, bool)> {
    match action {
        MicAction::Start => {
            if state.active.is_some() {
                return Ok(("ok: already recording".to_string(), false));
            }

            let active_window = if state.dry_run {
                None
            } else {
                capture_active_window(&state.xdotool_bin)
            };
            let recording = start_recording(&state.ffmpeg_bin, &state.source, active_window)
                .with_context(|| {
                    format!(
                        "failed starting microphone capture for source '{}'",
                        state.source
                    )
                })?;
            state.active = Some(recording);
            Ok(("ok: recording started".to_string(), false))
        }
        MicAction::Stop => {
            if let Some(recording) = state.active.take() {
                let message = finalize_recording_cycle(state, recording)?;
                Ok((message, false))
            } else {
                Ok(("ok: not recording".to_string(), false))
            }
        }
        MicAction::Toggle => {
            if let Some(recording) = state.active.take() {
                let message = finalize_recording_cycle(state, recording)?;
                Ok((message, false))
            } else {
                let active_window = if state.dry_run {
                    None
                } else {
                    capture_active_window(&state.xdotool_bin)
                };
                let recording = start_recording(&state.ffmpeg_bin, &state.source, active_window)
                    .with_context(|| {
                        format!(
                            "failed starting microphone capture for source '{}'",
                            state.source
                        )
                    })?;
                state.active = Some(recording);
                Ok(("ok: recording started".to_string(), false))
            }
        }
        MicAction::Status => Ok((
            if state.active.is_some() {
                "ok: recording".to_string()
            } else {
                "ok: idle".to_string()
            },
            false,
        )),
        MicAction::Shutdown => {
            if let Some(recording) = state.active.take() {
                let _ = stop_recording(recording);
            }
            Ok(("ok: daemon shutting down".to_string(), true))
        }
    }
}

fn finalize_recording_cycle(
    state: &mut MicDaemonState,
    recording: ActiveRecording,
) -> Result<String> {
    let completed = stop_recording(recording)?;

    if completed.duration_secs < state.min_seconds {
        return Ok(format!(
            "ok: ignored short capture ({:.2}s < {:.2}s)",
            completed.duration_secs, state.min_seconds
        ));
    }

    let transcript = transcribe::transcribe_path_to_text(
        &state.config,
        &completed.wav_path,
        state.model.clone(),
        state.threads,
        state.language.clone(),
        state.translate,
        state.passthrough.clone(),
    )?;

    let text = normalize_transcript_text(&transcript);
    if text.is_empty() {
        return Ok("ok: empty transcript".to_string());
    }

    if state.dry_run {
        println!("{text}");
    } else {
        if let Some(window_id) = completed.active_window.as_deref()
            && let Err(err) = restore_window_focus(&state.xdotool_bin, window_id)
        {
            eprintln!("warning: failed to restore window focus: {err:#}");
        }

        if state.mic_copy_to_clipboard
            && let Err(err) = copy_to_clipboard(&state.clipboard_bin, &text)
        {
            eprintln!(
                "warning: clipboard copy failed using '{}': {err:#}",
                state.clipboard_bin
            );
        }

        inject_text(&state.xdotool_bin, &text)?;
    }

    Ok(format!("ok: transcribed {} chars", text.chars().count()))
}

fn ensure_whisper_runnable(whisper_bin: &Path) -> Result<()> {
    let mut command = Command::new(whisper_bin);
    command
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    binaries::apply_library_path_env(&mut command, whisper_bin);

    let status = command
        .status()
        .with_context(|| format!("failed running {} --help", whisper_bin.display()))?;

    if !status.success() {
        bail!(
            "whisper-cli '{}' is not runnable (check bundled shared libraries)",
            whisper_bin.display()
        );
    }

    Ok(())
}

fn start_recording(
    ffmpeg_bin: &str,
    source: &str,
    active_window: Option<String>,
) -> Result<ActiveRecording> {
    let temp_dir =
        tempfile::tempdir().context("failed to create temporary microphone directory")?;
    let wav_path = temp_dir.path().join("capture.wav");

    let mut command = Command::new(ffmpeg_bin);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-f")
        .arg("pulse")
        .arg("-i")
        .arg(source)
        .arg("-ar")
        .arg("16000")
        .arg("-ac")
        .arg("1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(&wav_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let child = command.spawn().with_context(|| {
        format!("failed to launch ffmpeg microphone capture with source '{source}'")
    })?;

    Ok(ActiveRecording {
        child,
        temp_dir,
        wav_path,
        started_at: Instant::now(),
        active_window,
    })
}

fn stop_recording(mut recording: ActiveRecording) -> Result<CompletedRecording> {
    if let Some(mut stdin) = recording.child.stdin.take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
    }

    let status = match recording
        .child
        .wait_timeout(RELEASE_TIMEOUT)
        .context("failed waiting for ffmpeg capture process")?
    {
        Some(status) => status,
        None => {
            let _ = recording.child.kill();
            let _ = recording.child.wait();
            bail!("ffmpeg capture did not stop in time");
        }
    };

    let mut stderr_bytes = Vec::new();
    if let Some(mut stderr) = recording.child.stderr.take() {
        stderr
            .read_to_end(&mut stderr_bytes)
            .context("failed reading ffmpeg stderr")?;
    }

    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        bail!("ffmpeg capture failed: {stderr}");
    }

    if !recording.wav_path.exists() {
        bail!("ffmpeg capture finished but no WAV was produced");
    }

    Ok(CompletedRecording {
        temp_dir: recording.temp_dir,
        wav_path: recording.wav_path,
        duration_secs: recording.started_at.elapsed().as_secs_f32(),
        active_window: recording.active_window,
    })
}

fn capture_active_window(xdotool_bin: &str) -> Option<String> {
    let output = Command::new(xdotool_bin)
        .arg("getactivewindow")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let window = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if window.is_empty() {
        return None;
    }

    Some(window)
}

fn restore_window_focus(xdotool_bin: &str, window_id: &str) -> Result<()> {
    let status = Command::new(xdotool_bin)
        .arg("windowactivate")
        .arg(window_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .with_context(|| format!("failed running '{}'", xdotool_bin))?;

    if !status.success() {
        bail!("xdotool failed to activate window {window_id}");
    }

    thread::sleep(WINDOW_RESTORE_DELAY);
    Ok(())
}

fn copy_to_clipboard(clipboard_bin: &str, text: &str) -> Result<()> {
    let mut child = Command::new(clipboard_bin)
        .arg("-selection")
        .arg("clipboard")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed starting '{}'", clipboard_bin))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .with_context(|| format!("failed writing to '{}'", clipboard_bin))?;
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

fn normalize_transcript_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn inject_text(xdotool_bin: &str, text: &str) -> Result<()> {
    for chunk in split_typing_chunks(text, TYPING_CHUNK_SIZE) {
        let status = Command::new(xdotool_bin)
            .arg("type")
            .arg("--clearmodifiers")
            .arg("--delay")
            .arg("1")
            .arg("--")
            .arg(chunk)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .with_context(|| format!("failed running '{}'", xdotool_bin))?;

        if !status.success() {
            bail!("xdotool failed while typing transcript");
        }
    }

    Ok(())
}

fn split_typing_chunks(text: &str, max_chars: usize) -> Vec<&str> {
    if text.chars().count() <= max_chars {
        return vec![text];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let mut char_count = 0;

    for (idx, _) in text.char_indices() {
        if char_count >= max_chars {
            chunks.push(&text[start..idx]);
            start = idx;
            char_count = 0;
        }
        char_count += 1;
    }

    if start < text.len() {
        chunks.push(&text[start..]);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::{MicAction, normalize_transcript_text, split_typing_chunks};

    #[test]
    fn parses_supported_actions() {
        assert!(matches!(MicAction::parse("start"), Some(MicAction::Start)));
        assert!(matches!(MicAction::parse("stop"), Some(MicAction::Stop)));
        assert!(matches!(
            MicAction::parse("toggle"),
            Some(MicAction::Toggle)
        ));
        assert!(matches!(
            MicAction::parse("status"),
            Some(MicAction::Status)
        ));
        assert!(matches!(
            MicAction::parse("shutdown"),
            Some(MicAction::Shutdown)
        ));
        assert!(MicAction::parse("invalid").is_none());
    }

    #[test]
    fn splits_long_typing_chunks() {
        let chunks = split_typing_chunks("abcdefghij", 4);
        assert_eq!(chunks, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn normalizes_whitespace_for_injection() {
        let text = " please   do\nthis \t now ";
        assert_eq!(normalize_transcript_text(text), "please do this now");
    }
}
