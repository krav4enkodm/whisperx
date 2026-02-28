# whisperx

`whisperx` is a DX-first CLI wrapper around `whisper.cpp`.

It adds:
- Config file with sane defaults
- Model registry + local cache management
- Automatic model ensure on transcription
- Optional ffmpeg normalization to 16kHz mono WAV
- Full whisper.cpp passthrough flags after `--`
- Push-to-talk microphone dictation mode for Linux X11

## Install

Linux x86_64 (no Cargo required):

```bash
curl -fsSL https://raw.githubusercontent.com/krav4enkodm/whisperx/main/scripts/install.sh | bash
```

The installer downloads the latest GitHub Release binaries and installs:
- `~/.local/bin/whisperx`
- `~/.local/bin/whisper-cli` (bundled)
- bundled `libwhisper.so*` / `libggml*.so*` runtime libraries

If needed, add `~/.local/bin` to `PATH`.

## Quickstart

```bash
whisperx config init
whisperx models list
whisperx models install base.en
whisperx transcribe audio.mp3
```

Dependency:
- `ffmpeg` must be installed on the machine.
- `xdotool` is required for `whisperx mic` text injection.

## Commands

```bash
whisperx transcribe <file>
whisperx mic
whisperx models list
whisperx models install <name>
whisperx models path
whisperx config init
whisperx config show
whisperx doctor
```

## Transcribe flags

```bash
whisperx transcribe audio.mp3 \
  --model base.en \
  --threads 8 \
  --language en \
  --output txt
```

Output options:
- `txt`
- `json`
- `srt`
- `vtt`

Disable conversion:

```bash
whisperx transcribe audio.wav --no-convert
```

## Advanced passthrough

Pass any native whisper.cpp flags after `--`:

```bash
whisperx transcribe audio.mp3 -- --beam-size 5 --max-tokens 256
```

If a flag appears in passthrough, it overrides friendly wrapper flags.

## Microphone dictation (Linux X11)

Start push-to-talk mode:

```bash
whisperx mic
```

Default behavior:
- hold `Ctrl+Alt+Space` to record microphone audio
- release key to transcribe and type text into the active window

Useful flags:

```bash
whisperx mic --hotkey Ctrl+Alt+Space --source default
whisperx mic --once --dry-run
whisperx mic -- --beam-size 5
```

Optional launcher script:

```bash
scripts/start-mic.sh
```

## Config file

Default path:
- Linux: `~/.config/whisperx/config.toml`
- macOS: `~/Library/Application Support/whisperx/config.toml`

Default contents:

```toml
default_model = "base.en"
model_dir = "~/.cache/whisperx/models"
whisper_bin = "auto"
ffmpeg_bin = "ffmpeg"
xdotool_bin = "xdotool"
threads = 4
language = "en"
convert = true
mic_hotkey = "Ctrl+Alt+Space"
mic_source = "default"
mic_min_seconds = 0.2
timeout_secs = 3600
```

## Environment checks

Run:

```bash
whisperx doctor
```

This checks:
- `ffmpeg` availability
- `whisper-cli` availability and runnable state (bundled or configured path)
- model cache directory writability
- x11 display/`xdotool` status for mic mode
- effective config summary
