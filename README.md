# whisperx

`whisperx` is a DX-first CLI wrapper around `whisper.cpp`.

It provides:
- Config file with sane defaults
- Model registry + local cache management
- Automatic model ensure on transcription
- Optional ffmpeg normalization to 16kHz mono WAV
- Full whisper.cpp passthrough flags after `--`
- Microphone daemon + toggle/start/stop helper commands

## Supported platform

- Linux (focused on Ubuntu)
- X11 session required for mic text injection (`xdotool` typing)

## Dependencies (Ubuntu)

Install runtime dependencies:

```bash
sudo apt update
sudo apt install -y ffmpeg xdotool xclip
```

Notes:
- `xclip` is optional, but recommended (clipboard copy before typing).
- On non-Ubuntu Linux distributions, install equivalent packages.

## Install (latest GitHub release)

This installs prebuilt binaries to `~/.local/bin`:

```bash
curl -fsSL https://raw.githubusercontent.com/krav4enkodm/whisperx/main/scripts/install.sh | bash
```

Installed files:
- `~/.local/bin/whisperx`
- `~/.local/bin/whisper-cli` (bundled)
- `~/.local/bin/libwhisper.so*` / `~/.local/bin/libggml*.so*`
- `~/.local/bin/whisperx-mic-{daemon,start,stop,toggle,status,shutdown}`

If needed, add `~/.local/bin` to `PATH`.

## Install from source (local development)

Use this when testing local changes before publishing:

```bash
cargo build --release
install -m 0755 target/release/whisperx ~/.local/bin/whisperx
install -m 0755 scripts/whisperx-mic-daemon.sh ~/.local/bin/whisperx-mic-daemon
install -m 0755 scripts/whisperx-mic-toggle.sh ~/.local/bin/whisperx-mic-toggle
install -m 0755 scripts/whisperx-mic-start.sh ~/.local/bin/whisperx-mic-start
install -m 0755 scripts/whisperx-mic-stop.sh ~/.local/bin/whisperx-mic-stop
install -m 0755 scripts/whisperx-mic-status.sh ~/.local/bin/whisperx-mic-status
install -m 0755 scripts/whisperx-mic-shutdown.sh ~/.local/bin/whisperx-mic-shutdown
```

If `whisper-cli` is not already available on your machine, either:
- run release installer once (installs bundled `whisper-cli`), or
- set `whisper_bin` in config to your system `whisper-cli` path.

## Clean install from scratch (local machine)

Optional cleanup of previous local install:

```bash
rm -f ~/.local/bin/whisperx ~/.local/bin/whisper-cli
rm -f ~/.local/bin/whisperx-mic-daemon ~/.local/bin/whisperx-mic-toggle ~/.local/bin/whisperx-mic-start
rm -f ~/.local/bin/whisperx-mic-stop ~/.local/bin/whisperx-mic-status ~/.local/bin/whisperx-mic-shutdown
rm -f ~/.local/bin/libwhisper.so* ~/.local/bin/libggml*.so*
```

Fresh install:

```bash
curl -fsSL https://raw.githubusercontent.com/krav4enkodm/whisperx/main/scripts/install.sh | bash
```

Validate install:

```bash
whisperx --version
whisperx doctor
```

## First-time setup

Initialize config:

```bash
whisperx config init
whisperx config show
```

Install model and test file transcription:

```bash
whisperx models install base.en
whisperx transcribe /path/to/audio.wav
```

## Microphone mode (recommended setup)

`whisperx mic toggle` is a pure control command and expects daemon to already be running.

Start daemon manually (for quick testing):

```bash
whisperx-mic-daemon
```

In another terminal:

```bash
whisperx-mic-toggle
whisperx-mic-toggle
```

Expected behavior:
- first toggle: starts recording
- second toggle: stops, transcribes, copies to clipboard (best-effort), types text into focused window

## Autostart daemon on login (systemd user service)

Create service:

```bash
mkdir -p ~/.config/systemd/user
cp scripts/whisperx-mic-daemon.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now whisperx-mic-daemon.service
```

Check status/logs:

```bash
systemctl --user status whisperx-mic-daemon.service
journalctl --user -u whisperx-mic-daemon.service -f
```

## Hotkey binding

Bind your desktop shortcut to:

```bash
bash -lc "$HOME/.local/bin/whisperx-mic-toggle"
```

Using `bash -lc` avoids common desktop environment `PATH` issues.

## Commands

```bash
whisperx transcribe <file>
whisperx mic daemon
whisperx mic start
whisperx mic stop
whisperx mic toggle
whisperx mic status
whisperx mic shutdown
whisperx models list
whisperx models install <name>
whisperx models path
whisperx config init
whisperx config show
whisperx doctor
```

## Transcribe examples

Friendly flags:

```bash
whisperx transcribe audio.mp3 \
  --model base.en \
  --threads 8 \
  --language en \
  --output txt
```

Disable conversion:

```bash
whisperx transcribe audio.wav --no-convert
```

Pass native whisper.cpp args after `--`:

```bash
whisperx transcribe audio.mp3 -- --beam-size 5 --max-tokens 256
```

If a flag appears in passthrough, it overrides friendly wrapper flags.

## Mic daemon flags

```bash
whisperx mic daemon --source default --min-seconds 0.2
whisperx mic daemon --dry-run
whisperx mic daemon -- --beam-size 5
```

## Default config file

Path:
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
mic_copy_to_clipboard = true
clipboard_bin = "xclip"
timeout_secs = 3600
```

`mic_socket` default behavior:
- Linux: `${XDG_RUNTIME_DIR}/whisperx/mic.sock` when `XDG_RUNTIME_DIR` exists
- fallback: `/tmp/whisperx-$USER/mic.sock`

## Publish a new release

Release automation is in `.github/workflows/release.yml` and runs on `v*` tags.

Typical publish flow:

```bash
# 1) run checks locally
cargo test

# 2) commit changes
git add .
git commit -m "release: <summary>"

# 3) create and push tag
git tag v0.1.6
git push origin main
git push origin v0.1.6
```

GitHub Actions will:
- build release artifacts
- create checksums
- publish GitHub Release assets

After publish, validate from scratch using:

```bash
scripts/test-clean-install.sh
```
