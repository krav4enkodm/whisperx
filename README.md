# whisperx

`whisperx` is a DX-first CLI wrapper around `whisper.cpp`.

It adds:
- Config file with sane defaults
- Model registry + local cache management
- Automatic model ensure on transcription
- Optional ffmpeg normalization to 16kHz mono WAV
- Full whisper.cpp passthrough flags after `--`
- Microphone daemon + local trigger script flow for flexible shortcut binding

## Install

Linux x86_64 (no Cargo required):

```bash
curl -fsSL https://raw.githubusercontent.com/krav4enkodm/whisperx/main/scripts/install.sh | bash
```

The installer downloads the latest GitHub Release binaries and installs:
- `~/.local/bin/whisperx`
- `~/.local/bin/whisper-cli` (bundled)
- bundled `libwhisper.so*` / `libggml*.so*` runtime libraries
- helper commands: `whisperx-mic-daemon`, `whisperx-mic-start`, `whisperx-mic-stop`, `whisperx-mic-toggle`, `whisperx-mic-status`, `whisperx-mic-shutdown`

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
- `xdotool` is required when microphone output should be typed into the active window.

## Commands

```bash
whisperx transcribe <file>
whisperx mic daemon
whisperx mic start
whisperx mic stop
whisperx mic toggle
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

## Microphone dictation daemon

Start daemon (keep this running):

```bash
whisperx mic daemon
```

Trigger commands from terminal or shortcut scripts:

```bash
whisperx mic start
whisperx mic stop
whisperx mic toggle
whisperx mic status
whisperx mic shutdown
```

Daemon flags:

```bash
whisperx mic daemon --source default --min-seconds 0.2
whisperx mic daemon --dry-run
whisperx mic daemon -- --beam-size 5
```

Helper commands installed by `scripts/install.sh` (recommended for shortcut bindings):

```bash
whisperx-mic-daemon
whisperx-mic-toggle
whisperx-mic-start
whisperx-mic-stop
whisperx-mic-status
whisperx-mic-shutdown
```

Repo scripts with equivalent behavior:

```bash
scripts/start-mic.sh
scripts/whisperx-mic-toggle.sh
scripts/whisperx-mic-start.sh
scripts/whisperx-mic-stop.sh
scripts/whisperx-mic-status.sh
scripts/whisperx-mic-shutdown.sh
```

Recommended setup:
- keep `whisperx-mic-daemon` running in background/session startup
- bind desktop shortcut to `whisperx-mic-toggle`
- optional hold-style setup: bind key-down -> `whisperx-mic-start`, key-up -> `whisperx-mic-stop` (if your hotkey tool supports key press/release hooks)

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
mic_socket = "/tmp/whisperx-user/mic.sock"
mic_source = "default"
mic_min_seconds = 0.2
timeout_secs = 3600
```

`mic_socket` default:
- Linux: `${XDG_RUNTIME_DIR}/whisperx/mic.sock` when `XDG_RUNTIME_DIR` exists
- fallback: `/tmp/whisperx-$USER/mic.sock`

## Environment checks

Run:

```bash
whisperx doctor
```

This checks:
- `ffmpeg` availability
- `whisper-cli` availability and runnable state (bundled or configured path)
- model cache directory writability
- x11 display/`xdotool` status for mic typing mode
- configured mic daemon socket path
- effective config summary
