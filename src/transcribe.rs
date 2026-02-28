use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tempfile::{Builder, TempDir};
use wait_timeout::ChildExt;

use crate::binaries;
use crate::cli::{OutputFormat, TranscribeArgs};
use crate::config::AppConfig;
use crate::models;

#[derive(Debug, Clone)]
pub struct TranscribeRequest {
    pub input: PathBuf,
    pub model: Option<String>,
    pub output: OutputFormat,
    pub no_convert: bool,
    pub threads: Option<usize>,
    pub language: Option<String>,
    pub translate: bool,
    pub passthrough: Vec<String>,
}

impl From<&TranscribeArgs> for TranscribeRequest {
    fn from(args: &TranscribeArgs) -> Self {
        Self {
            input: args.input.clone(),
            model: args.model.clone(),
            output: args.output,
            no_convert: args.no_convert,
            threads: args.threads,
            language: args.language.clone(),
            translate: args.translate,
            passthrough: args.passthrough.clone(),
        }
    }
}

pub fn run(config: &AppConfig, args: &TranscribeArgs) -> Result<()> {
    let request = TranscribeRequest::from(args);
    let content = run_request(config, &request)?;
    print_output(request.output, &content);
    Ok(())
}

pub fn run_request(config: &AppConfig, request: &TranscribeRequest) -> Result<String> {
    if !request.input.exists() {
        bail!("input file does not exist: {}", request.input.display());
    }

    let model_name = request
        .model
        .clone()
        .unwrap_or_else(|| config.default_model.clone());
    let model_path = models::ensure_model(config, &model_name)?;

    let timeout = Duration::from_secs(config.timeout_secs);
    let (prepared_input, _temp_wav) = prepare_input(config, request, timeout)?;

    let output_dir = tempfile::tempdir().context("failed to create temporary output directory")?;
    let output_base = output_dir.path().join("whisperx-output");

    let whisper_bin = binaries::resolve_whisper_bin(config);
    let mut command = Command::new(&whisper_bin);
    command
        .arg("-m")
        .arg(&model_path)
        .arg("-f")
        .arg(&prepared_input)
        .arg("-of")
        .arg(&output_base)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    add_friendly_flags(&mut command, config, request);
    add_output_flag(&mut command, request.output);
    binaries::apply_library_path_env(&mut command, &whisper_bin);

    command.args(&request.passthrough);

    let output = run_with_timeout(
        command,
        timeout,
        &format!("whisper-cli ({})", whisper_bin.display()),
    )?;

    let result_path = output_file_path(&output_dir, request.output);
    if result_path.exists() {
        let content = fs::read_to_string(&result_path)
            .with_context(|| format!("failed reading {}", result_path.display()))?;
        return Ok(content);
    }

    if request.output == OutputFormat::Txt {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let transcript = extract_transcript_from_stdout(&stdout);
        if !transcript.is_empty() {
            return Ok(transcript);
        }
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    bail!(
        "whisper-cli finished without expected output file ({}). stderr: {}",
        result_path.display(),
        stderr
    );
}

pub fn transcribe_path_to_text(
    config: &AppConfig,
    input: &Path,
    model: Option<String>,
    threads: Option<usize>,
    language: Option<String>,
    translate: bool,
    passthrough: Vec<String>,
) -> Result<String> {
    let request = TranscribeRequest {
        input: input.to_path_buf(),
        model,
        output: OutputFormat::Txt,
        no_convert: true,
        threads,
        language,
        translate,
        passthrough,
    };

    run_request(config, &request)
}

fn prepare_input(
    config: &AppConfig,
    request: &TranscribeRequest,
    timeout: Duration,
) -> Result<(PathBuf, Option<tempfile::NamedTempFile>)> {
    let use_conversion = if request.no_convert {
        false
    } else {
        config.convert
    };

    if !use_conversion {
        return Ok((request.input.clone(), None));
    }

    let temp_wav = Builder::new()
        .prefix("whisperx-")
        .suffix(".wav")
        .tempfile()
        .context("failed to create temporary WAV file")?;

    let mut ffmpeg = Command::new(&config.ffmpeg_bin);
    ffmpeg
        .arg("-nostdin")
        .arg("-y")
        .arg("-i")
        .arg(&request.input)
        .arg("-ar")
        .arg("16000")
        .arg("-ac")
        .arg("1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(temp_wav.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    run_with_timeout(ffmpeg, timeout, "ffmpeg")?;

    Ok((temp_wav.path().to_path_buf(), Some(temp_wav)))
}

fn add_friendly_flags(command: &mut Command, config: &AppConfig, request: &TranscribeRequest) {
    if !has_passthrough_flag(&request.passthrough, &["-t", "--threads"]) {
        let threads = request.threads.unwrap_or(config.threads);
        command.arg("-t").arg(threads.to_string());
    }

    if !has_passthrough_flag(&request.passthrough, &["-l", "--language"]) {
        let language = request.language.as_deref().unwrap_or(&config.language);
        if !language.eq_ignore_ascii_case("auto") {
            command.arg("-l").arg(language);
        }
    }

    if request.translate && !has_passthrough_flag(&request.passthrough, &["--translate"]) {
        command.arg("--translate");
    }
}

fn add_output_flag(command: &mut Command, format: OutputFormat) {
    match format {
        OutputFormat::Txt => {
            command.arg("-otxt");
        }
        OutputFormat::Json => {
            command.arg("-oj");
        }
        OutputFormat::Srt => {
            command.arg("-osrt");
        }
        OutputFormat::Vtt => {
            command.arg("-ovtt");
        }
    };
}

fn output_file_path(output_dir: &TempDir, format: OutputFormat) -> PathBuf {
    let ext = match format {
        OutputFormat::Txt => "txt",
        OutputFormat::Json => "json",
        OutputFormat::Srt => "srt",
        OutputFormat::Vtt => "vtt",
    };

    output_dir.path().join(format!("whisperx-output.{ext}"))
}

fn print_output(format: OutputFormat, content: &str) {
    match format {
        OutputFormat::Txt => {
            println!("{}", content.trim_end());
        }
        OutputFormat::Json | OutputFormat::Srt | OutputFormat::Vtt => {
            print!("{content}");
            if !content.ends_with('\n') {
                println!();
            }
        }
    }
}

fn has_passthrough_flag(passthrough: &[String], names: &[&str]) -> bool {
    passthrough.iter().any(|arg| names.contains(&arg.as_str()))
}

fn run_with_timeout(mut command: Command, timeout: Duration, label: &str) -> Result<Output> {
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {label}"))?;

    let status = match child
        .wait_timeout(timeout)
        .with_context(|| format!("failed waiting for {label}"))?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{label} timed out after {} seconds", timeout.as_secs());
        }
    };

    let output = child
        .wait_with_output()
        .with_context(|| format!("failed collecting {label} output"))?;

    if !status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("{label} failed: {stderr}");
    }

    Ok(output)
}

fn extract_transcript_from_stdout(stdout: &str) -> String {
    let mut lines = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("main:") {
            continue;
        }

        if trimmed.starts_with('[')
            && let Some((_, rest)) = trimmed.split_once(']')
        {
            let text = rest.trim();
            if !text.is_empty() {
                lines.push(text.to_string());
            }
            continue;
        }

        if !trimmed.contains("->") {
            lines.push(trimmed.to_string());
        }
    }

    lines.join(" ")
}
