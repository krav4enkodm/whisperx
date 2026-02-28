use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use crate::binaries;
use crate::cache;
use crate::config::AppConfig;

pub fn run(config: &AppConfig) -> Result<()> {
    let mut failures = 0_u32;

    let ffmpeg_ok = which::which(&config.ffmpeg_bin).is_ok();
    print_check(
        ffmpeg_ok,
        "ffmpeg",
        &format!(
            "binary '{}'{}",
            config.ffmpeg_bin,
            if ffmpeg_ok {
                " found"
            } else {
                " not found in PATH"
            }
        ),
    );
    if !ffmpeg_ok {
        failures += 1;
    }

    let whisper_bin = binaries::resolve_whisper_bin(config);
    let whisper_found = binaries::binary_available(&whisper_bin);
    let whisper_runnable = if whisper_found {
        check_whisper_runnable(&whisper_bin)
    } else {
        false
    };
    let whisper_ok = whisper_found && whisper_runnable;
    let whisper_details = if !whisper_found {
        format!(
            "resolved to '{}' (not found). Set whisper_bin in config if needed",
            whisper_bin.display()
        )
    } else if !whisper_runnable {
        format!(
            "resolved to '{}' but failed to start (likely missing shared libraries)",
            whisper_bin.display()
        )
    } else {
        format!("resolved to '{}'", whisper_bin.display())
    };
    print_check(whisper_ok, "whisper-cli", &whisper_details);
    if !whisper_ok {
        failures += 1;
    }

    let model_dir_ok = check_writable_dir(&config.model_dir)?;
    print_check(
        model_dir_ok,
        "model dir",
        &format!(
            "{}{}",
            config.model_dir.display(),
            if model_dir_ok {
                " is writable"
            } else {
                " is not writable"
            }
        ),
    );
    if !model_dir_ok {
        failures += 1;
    }

    let display_set = std::env::var("DISPLAY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    print_info(
        "x11 display",
        if display_set {
            "DISPLAY is set (mic typing mode available on X11)"
        } else {
            "DISPLAY not set (mic typing mode unavailable in this shell)"
        },
    );

    let xdotool_ok = binaries::binary_available(Path::new(&config.xdotool_bin));
    print_info(
        "xdotool",
        if xdotool_ok {
            "found (mic daemon text injection available)"
        } else {
            "not found (install xdotool for `whisperx mic daemon` typing)"
        },
    );
    let clipboard_ok = binaries::binary_available(Path::new(&config.clipboard_bin));
    print_info(
        "clipboard",
        if config.mic_copy_to_clipboard {
            if clipboard_ok {
                "enabled and command available"
            } else {
                "enabled but command not found (copy will be skipped with warning)"
            }
        } else {
            "disabled"
        },
    );
    print_info(
        "mic socket",
        &format!("configured at {}", config.mic_socket.display()),
    );

    println!("\nEffective config:");
    println!("{}", toml::to_string_pretty(config)?);

    if failures > 0 {
        println!("Fixes:");
        if !ffmpeg_ok {
            println!(
                "- Install ffmpeg and ensure '{}' resolves via PATH",
                config.ffmpeg_bin
            );
        }
        if !whisper_ok {
            println!(
                "- Ensure bundled 'whisper-cli' and its shared libraries are present next to 'whisperx', or set whisper_bin to a valid system binary"
            );
        }
        if !model_dir_ok {
            println!(
                "- Set model_dir in config to a writable location (current: {})",
                config.model_dir.display()
            );
        }

        bail!("doctor found {failures} issue(s)");
    }

    Ok(())
}

fn check_whisper_runnable(whisper_bin: &Path) -> bool {
    let mut command = Command::new(whisper_bin);
    command
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    binaries::apply_library_path_env(&mut command, whisper_bin);

    command
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn print_check(ok: bool, name: &str, details: &str) {
    if ok {
        println!("[ok]   {name}: {details}");
    } else {
        println!("[fail] {name}: {details}");
    }
}

fn print_info(name: &str, details: &str) {
    println!("[info] {name}: {details}");
}

fn check_writable_dir(path: &Path) -> Result<bool> {
    cache::ensure_dir(path)?;

    let probe_path = path.join(".whisperx-write-probe");
    match fs::write(&probe_path, b"ok") {
        Ok(_) => {
            let _ = fs::remove_file(&probe_path);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}
