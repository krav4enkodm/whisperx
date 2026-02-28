# whisperx

A DX-first CLI wrapper around [whisper.cpp](https://github.com/ggml-org/whisper.cpp) — the open-source speech-to-text engine by [ggml-org](https://github.com/ggml-org).

whisperx handles the tedious parts so you can focus on transcription:

- Sane defaults via config file
- Model registry with local cache management
- Automatic model download on first transcription
- ffmpeg normalization to 16kHz mono WAV
- Full whisper.cpp flag passthrough after `--`
- Microphone daemon with toggle/start/stop helpers and hotkey binding

## Requirements

- Linux (focused on Ubuntu, X11 session for mic text injection)
- Runtime dependencies:

```bash
sudo apt update
sudo apt install -y ffmpeg xdotool xclip
```

`xclip` is optional — only needed when `mic_output = "clipboard"`.

## Install

Everything installs to `~/.local/bin`. Pick one of the options below.

### Option A: CPU-only (default)

Works on any Linux x86_64 machine. Good for smaller models (tiny, base, small).

```bash
curl -fsSL https://raw.githubusercontent.com/krav4enkodm/whisperx/main/scripts/install.sh | bash
```

### Option B: CUDA (NVIDIA GPU)

Recommended if you have an NVIDIA GPU. Gives a dramatic speedup with larger
models (medium, large) — inference becomes near-instant, fully offloaded to GPU.

First, make sure NVIDIA drivers and CUDA runtime are installed:

```bash
sudo apt install -y nvidia-cuda-toolkit
```

Then install with the `--cuda` flag:

```bash
curl -fsSL https://raw.githubusercontent.com/krav4enkodm/whisperx/main/scripts/install.sh | bash -s -- --cuda
```

| Backend | Transcription time (medium model, ~10s audio) | CPU usage |
|---------|-----------------------------------------------|-----------|
| CPU-only | Several seconds | High |
| CUDA | Near-instant | Minimal |

### Option C: Custom whisper.cpp build (advanced)

Both options above bundle a prebuilt `whisper-cli`. If you need a custom build
(different CUDA version, other accelerator backends, special flags), build
[whisper.cpp](https://github.com/ggml-org/whisper.cpp) yourself:

```bash
git clone https://github.com/ggml-org/whisper.cpp.git
cd whisper.cpp
cmake -B build -DGGML_CUDA=ON    # or other flags
cmake --build build --config Release -j$(nproc)
```

Then point whisperx at your build in `~/.config/whisperx/config.toml`:

```toml
whisper_bin = "/path/to/whisper.cpp/build/bin/whisper-cli"
```

### Installed files

After any install option, `~/.local/bin` will contain:

- `whisperx` — main CLI
- `whisper-cli` — bundled whisper.cpp binary
- `libwhisper.so*`, `libggml*.so*` — shared libraries
- `whisperx-mic-{daemon,start,stop,toggle,status,shutdown}` — mic helpers

Add `~/.local/bin` to your `PATH` if it isn't already.

### Verify installation

```bash
whisperx --version
whisperx doctor
```

## Getting started

Initialize config and install a model:

```bash
whisperx config init
whisperx models install base.en
```

Transcribe a file:

```bash
whisperx transcribe /path/to/audio.wav
```

`whisperx models list` shows the full upstream whisper.cpp GGML model set
(including quantized variants). Source list:
[models/download-ggml-model.sh](https://github.com/ggml-org/whisper.cpp/blob/master/models/download-ggml-model.sh).

## Microphone mode

The mic daemon listens for toggle commands and transcribes on stop.

Start the daemon:

```bash
whisperx-mic-daemon
```

In another terminal:

```bash
whisperx-mic-toggle    # starts recording
whisperx-mic-toggle    # stops, transcribes, emits text
```

Output modes:
- `mic_output = "type"` (default) — types transcript into the focused window
- `mic_output = "clipboard"` — copies transcript to clipboard

### Autostart on login (systemd)

```bash
mkdir -p ~/.config/systemd/user
cp scripts/whisperx-mic-daemon.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now whisperx-mic-daemon.service
```

Check status:

```bash
systemctl --user status whisperx-mic-daemon.service
journalctl --user -u whisperx-mic-daemon.service -f
```

### Hotkey binding

Bind your desktop shortcut to:

```bash
bash -lc "$HOME/.local/bin/whisperx-mic-toggle"
```

`bash -lc` avoids common desktop environment `PATH` issues.

## Commands

```
whisperx transcribe <file>       Transcribe an audio file
whisperx mic daemon              Start mic daemon
whisperx mic start               Start recording
whisperx mic stop                Stop recording and transcribe
whisperx mic toggle              Toggle recording on/off
whisperx mic status              Show daemon status
whisperx mic shutdown            Stop the daemon
whisperx models list             List available models
whisperx models install <name>   Download a model
whisperx models path             Show model cache directory
whisperx config init             Create default config
whisperx config show             Print current config
whisperx doctor                  Check installation health
```

## Transcribe examples

```bash
# Friendly flags
whisperx transcribe audio.mp3 \
  --model base.en \
  --threads 8 \
  --language en \
  --output txt

# Skip ffmpeg conversion for pre-converted WAV files
whisperx transcribe audio.wav --no-convert

# Pass native whisper.cpp flags after --
whisperx transcribe audio.mp3 -- --beam-size 5 --max-tokens 256
```

Passthrough flags override friendly wrapper flags. See upstream
[whisper.cpp](https://github.com/ggml-org/whisper.cpp) for all available flags.

## Mic daemon flags

```bash
whisperx mic daemon --source default --min-seconds 0.2
whisperx mic daemon --dry-run
whisperx mic daemon -- --beam-size 5
```

## Config

Path: `~/.config/whisperx/config.toml`

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
mic_output = "type"
clipboard_bin = "xclip"
timeout_secs = 3600
```

`mic_socket` defaults to `${XDG_RUNTIME_DIR}/whisperx/mic.sock` when
`XDG_RUNTIME_DIR` exists, otherwise `/tmp/whisperx-$USER/mic.sock`.

## Uninstall

Remove all installed files:

```bash
rm -f ~/.local/bin/whisperx ~/.local/bin/whisper-cli
rm -f ~/.local/bin/whisperx-mic-{daemon,start,stop,toggle,status,shutdown}
rm -f ~/.local/bin/libwhisper.so* ~/.local/bin/libggml*.so*
```

Remove config and cached models (optional):

```bash
rm -rf ~/.config/whisperx
rm -rf ~/.cache/whisperx
```

If you set up the systemd service, disable it first:

```bash
systemctl --user disable --now whisperx-mic-daemon.service
rm -f ~/.config/systemd/user/whisperx-mic-daemon.service
systemctl --user daemon-reload
```

## Install from source

For local development and testing:

```bash
cargo build --release
install -m 0755 target/release/whisperx ~/.local/bin/whisperx
```

If `whisper-cli` is not already available, either run the release installer
once (to get the bundled binary) or set `whisper_bin` in config to your
system `whisper-cli` path.

## Publish a new release

Release automation is in `.github/workflows/release.yml` and runs on `v*` tags.

```bash
cargo test
git add .
git commit -m "release: <summary>"
git tag v0.1.12
git push origin main
git push origin v0.1.12
```

GitHub Actions builds both CPU and CUDA release artifacts, creates checksums,
and publishes the GitHub Release.

Validate after publish:

```bash
scripts/test-clean-install.sh
```
