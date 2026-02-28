use std::io::Write;
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
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    ConnectionExt as _, GrabMode, KeyButMask, KeyPressEvent, KeyReleaseEvent, ModMask,
};
use x11rb::rust_connection::RustConnection;

use crate::binaries;
use crate::cli::MicArgs;
use crate::config::AppConfig;
use crate::transcribe;

const RELEASE_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_DELAY: Duration = Duration::from_millis(20);
const TYPING_CHUNK_SIZE: usize = 2048;
const XK_SPACE: u32 = 0x0020;
const XK_RETURN: u32 = 0xff0d;
const XK_TAB: u32 = 0xff09;
const XK_ESCAPE: u32 = 0xff1b;
const XK_BACKSPACE: u32 = 0xff08;
const XK_MINUS: u32 = 0x002d;
const XK_COMMA: u32 = 0x002c;
const XK_PERIOD: u32 = 0x002e;
const XK_SLASH: u32 = 0x002f;
const XK_SEMICOLON: u32 = 0x003b;
const XK_APOSTROPHE: u32 = 0x0027;
const XK_LEFT: u32 = 0xff51;
const XK_UP: u32 = 0xff52;
const XK_RIGHT: u32 = 0xff53;
const XK_DOWN: u32 = 0xff54;
const XK_HOME: u32 = 0xff50;
const XK_END: u32 = 0xff57;
const XK_PAGE_UP: u32 = 0xff55;
const XK_PAGE_DOWN: u32 = 0xff56;
const XK_F1: u32 = 0xffbe;

#[derive(Debug)]
struct HotkeySpec {
    modifiers: u16,
    keysym: u32,
}

struct ActiveRecording {
    child: Child,
    temp_dir: TempDir,
    wav_path: PathBuf,
    started_at: Instant,
}

struct CompletedRecording {
    #[allow(dead_code)]
    temp_dir: TempDir,
    wav_path: PathBuf,
    duration_secs: f32,
}

pub fn run(config: &AppConfig, args: &MicArgs) -> Result<()> {
    ensure_x11_environment()?;

    let hotkey_text = args
        .hotkey
        .clone()
        .unwrap_or_else(|| config.mic_hotkey.clone());
    let source = args
        .source
        .clone()
        .unwrap_or_else(|| config.mic_source.clone());
    let min_seconds = args.min_seconds.unwrap_or(config.mic_min_seconds);
    let ffmpeg_bin = args
        .ffmpeg_bin
        .clone()
        .unwrap_or_else(|| config.ffmpeg_bin.clone());
    let xdotool_bin = args
        .xdotool_bin
        .clone()
        .unwrap_or_else(|| config.xdotool_bin.clone());

    if !binaries::binary_available(Path::new(&ffmpeg_bin)) {
        bail!("ffmpeg binary '{}' not found", ffmpeg_bin);
    }

    if !args.dry_run && !binaries::binary_available(Path::new(&xdotool_bin)) {
        bail!(
            "xdotool binary '{}' not found (required for text injection)",
            xdotool_bin
        );
    }

    let whisper_bin = binaries::resolve_whisper_bin(config);
    if !binaries::binary_available(&whisper_bin) {
        bail!("whisper-cli binary '{}' not found", whisper_bin.display());
    }
    ensure_whisper_runnable(&whisper_bin)?;

    let hotkey = parse_hotkey(&hotkey_text)?;

    let (conn, screen_num) =
        RustConnection::connect(None).context("failed to connect to X11 server")?;
    let root = conn.setup().roots[screen_num].root;
    let keycode = find_keycode_for_keysym(&conn, hotkey.keysym)?;

    grab_hotkey(&conn, root, keycode, hotkey.modifiers)?;
    conn.flush().context("failed to flush X11 key grabs")?;

    let running = Arc::new(AtomicBool::new(true));
    let handler_flag = Arc::clone(&running);
    ctrlc::set_handler(move || {
        handler_flag.store(false, Ordering::SeqCst);
    })
    .context("failed to install Ctrl+C handler")?;

    eprintln!(
        "whisperx mic listening on '{}' (hold to record, release to transcribe)",
        hotkey_text
    );

    let mut active_recording: Option<ActiveRecording> = None;

    while running.load(Ordering::SeqCst) {
        if let Some(event) = conn.poll_for_event().context("failed reading X11 events")? {
            match event {
                Event::KeyPress(ev) => {
                    if is_hotkey_press(&ev, keycode, hotkey.modifiers) && active_recording.is_none()
                    {
                        match start_recording(&ffmpeg_bin, &source).with_context(|| {
                            format!("failed starting microphone capture for source '{source}'")
                        }) {
                            Ok(recording) => {
                                active_recording = Some(recording);
                            }
                            Err(err) => {
                                eprintln!("mic cycle failed: {err:#}");
                                if args.once {
                                    break;
                                }
                            }
                        }
                    }
                }
                Event::KeyRelease(ev) => {
                    if is_hotkey_release(&ev, keycode, hotkey.modifiers)
                        && let Some(recording) = active_recording.take()
                    {
                        if is_key_still_down(&conn, keycode)? {
                            active_recording = Some(recording);
                            continue;
                        }

                        let cycle_result = stop_recording(recording).and_then(|completed| {
                            handle_completed_recording(
                                config,
                                args,
                                &xdotool_bin,
                                completed,
                                min_seconds,
                            )
                        });

                        match cycle_result {
                            Ok(should_exit) => {
                                if args.once || should_exit {
                                    break;
                                }
                            }
                            Err(err) => {
                                eprintln!("mic cycle failed: {err:#}");
                                if args.once {
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        } else {
            thread::sleep(POLL_DELAY);
        }
    }

    if let Some(recording) = active_recording {
        let _ = stop_recording(recording);
    }

    ungrab_hotkey(&conn, root, keycode, hotkey.modifiers)?;
    conn.flush().ok();

    Ok(())
}

fn handle_completed_recording(
    config: &AppConfig,
    args: &MicArgs,
    xdotool_bin: &str,
    completed: CompletedRecording,
    min_seconds: f32,
) -> Result<bool> {
    if completed.duration_secs < min_seconds {
        return Ok(false);
    }

    let transcript = transcribe::transcribe_path_to_text(
        config,
        &completed.wav_path,
        args.model.clone(),
        args.threads,
        args.language.clone(),
        args.translate,
        args.passthrough.clone(),
    )?;

    let text = transcript.trim();
    if text.is_empty() {
        return Ok(false);
    }

    if args.dry_run {
        println!("{text}");
    } else {
        inject_text(xdotool_bin, text)?;
    }

    Ok(false)
}

fn ensure_x11_environment() -> Result<()> {
    let display = std::env::var("DISPLAY").unwrap_or_default();
    if display.trim().is_empty() {
        bail!("DISPLAY is not set. whisperx mic requires an X11 session");
    }

    Ok(())
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

fn parse_hotkey(input: &str) -> Result<HotkeySpec> {
    let mut modifiers = 0_u16;
    let mut key: Option<&str> = None;

    for token in input.split('+').map(str::trim).filter(|s| !s.is_empty()) {
        let lower = token.to_ascii_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => modifiers |= ModMask::CONTROL.bits(),
            "alt" | "mod1" => modifiers |= ModMask::M1.bits(),
            "shift" => modifiers |= ModMask::SHIFT.bits(),
            "super" | "win" | "meta" | "mod4" => modifiers |= ModMask::M4.bits(),
            _ => {
                if key.is_some() {
                    bail!("hotkey '{input}' must contain exactly one non-modifier key");
                }
                key = Some(token);
            }
        }
    }

    let key_name =
        key.ok_or_else(|| anyhow::anyhow!("hotkey '{input}' is missing the main key"))?;
    let keysym = parse_keysym(key_name)?;

    Ok(HotkeySpec { modifiers, keysym })
}

fn parse_keysym(token: &str) -> Result<u32> {
    let lower = token.to_ascii_lowercase();

    let keysym = match lower.as_str() {
        "space" => XK_SPACE,
        "enter" | "return" => XK_RETURN,
        "tab" => XK_TAB,
        "esc" | "escape" => XK_ESCAPE,
        "backspace" => XK_BACKSPACE,
        "minus" => XK_MINUS,
        "comma" => XK_COMMA,
        "period" | "dot" => XK_PERIOD,
        "slash" => XK_SLASH,
        "semicolon" => XK_SEMICOLON,
        "apostrophe" | "quote" => XK_APOSTROPHE,
        "left" => XK_LEFT,
        "right" => XK_RIGHT,
        "up" => XK_UP,
        "down" => XK_DOWN,
        "home" => XK_HOME,
        "end" => XK_END,
        "pageup" => XK_PAGE_UP,
        "pagedown" => XK_PAGE_DOWN,
        _ if lower.len() == 1 => lower.as_bytes()[0] as u32,
        _ if lower.starts_with('f') => {
            let number = lower
                .trim_start_matches('f')
                .parse::<u32>()
                .with_context(|| format!("unsupported function key token '{token}'"))?;
            if !(1..=35).contains(&number) {
                bail!("unsupported function key token '{token}'");
            }
            XK_F1 + (number - 1)
        }
        _ => bail!("unsupported hotkey key token '{token}'"),
    };

    Ok(keysym)
}

fn find_keycode_for_keysym(conn: &RustConnection, target_keysym: u32) -> Result<u8> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let max = setup.max_keycode;
    let count = max.saturating_sub(min).saturating_add(1);

    let reply = conn
        .get_keyboard_mapping(min, count)?
        .reply()
        .context("failed reading X11 keyboard mapping")?;

    let per_keycode = usize::from(reply.keysyms_per_keycode);
    if per_keycode == 0 {
        bail!("X11 keyboard mapping returned zero keysyms per keycode");
    }

    for (index, keysyms) in reply.keysyms.chunks(per_keycode).enumerate() {
        if keysyms.contains(&target_keysym) {
            let keycode = min.saturating_add(index as u8);
            return Ok(keycode);
        }
    }

    bail!("failed to resolve keysym {target_keysym} to a keycode")
}

fn grab_hotkey(conn: &RustConnection, root: u32, keycode: u8, modifiers: u16) -> Result<()> {
    for lock_variant in lock_variants() {
        conn.grab_key(
            false,
            root,
            ModMask::from(modifiers | lock_variant),
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?;
    }

    Ok(())
}

fn ungrab_hotkey(conn: &RustConnection, root: u32, keycode: u8, modifiers: u16) -> Result<()> {
    for lock_variant in lock_variants() {
        conn.ungrab_key(keycode, root, ModMask::from(modifiers | lock_variant))?;
    }

    Ok(())
}

fn lock_variants() -> [u16; 4] {
    [
        0,
        ModMask::LOCK.bits(),
        ModMask::M2.bits(),
        ModMask::LOCK.bits() | ModMask::M2.bits(),
    ]
}

fn is_hotkey_press(event: &KeyPressEvent, keycode: u8, modifiers: u16) -> bool {
    event.detail == keycode && normalized_modifiers(event.state) == modifiers
}

fn is_hotkey_release(event: &KeyReleaseEvent, keycode: u8, modifiers: u16) -> bool {
    event.detail == keycode && normalized_modifiers(event.state) == modifiers
}

fn normalized_modifiers(state: KeyButMask) -> u16 {
    state.bits() & !(ModMask::LOCK.bits() | ModMask::M2.bits())
}

fn is_key_still_down(conn: &RustConnection, keycode: u8) -> Result<bool> {
    let reply = conn
        .query_keymap()?
        .reply()
        .context("failed to query current X11 key state")?;

    let idx = usize::from(keycode / 8);
    let bit = keycode % 8;
    let mask = 1_u8 << bit;

    Ok(reply.keys.get(idx).is_some_and(|value| (value & mask) != 0))
}

fn start_recording(ffmpeg_bin: &str, source: &str) -> Result<ActiveRecording> {
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

    let output = recording
        .child
        .wait_with_output()
        .context("failed collecting ffmpeg capture output")?;

    if !status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("ffmpeg capture failed: {stderr}");
    }

    if !recording.wav_path.exists() {
        bail!("ffmpeg capture finished but no WAV was produced");
    }

    Ok(CompletedRecording {
        temp_dir: recording.temp_dir,
        wav_path: recording.wav_path,
        duration_secs: recording.started_at.elapsed().as_secs_f32(),
    })
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
    use super::{parse_hotkey, split_typing_chunks};

    #[test]
    fn parses_default_hotkey() {
        let parsed = parse_hotkey("Ctrl+Alt+Space").expect("hotkey should parse");
        assert_ne!(parsed.modifiers, 0);
        assert_ne!(parsed.keysym, 0);
    }

    #[test]
    fn rejects_hotkey_without_main_key() {
        let err = parse_hotkey("Ctrl+Alt").expect_err("expected parsing error");
        assert!(err.to_string().contains("missing the main key"));
    }

    #[test]
    fn splits_long_typing_chunks() {
        let chunks = split_typing_chunks("abcdefghij", 4);
        assert_eq!(chunks, vec!["abcd", "efgh", "ij"]);
    }
}
