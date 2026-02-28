# whisperx

`whisperx` is a DX-first CLI wrapper around `whisper.cpp`.

It adds:
- Config file with sane defaults
- Model registry + local cache management
- Automatic model ensure on transcription
- Optional ffmpeg normalization to 16kHz mono WAV
- Full whisper.cpp passthrough flags after `--`

## Install

Linux/macOS (no Cargo required):

```bash
curl -fsSL https://raw.githubusercontent.com/krav4enkodm/whisperx/main/scripts/install.sh | bash
```

The installer downloads the latest GitHub Release binary for your OS/arch and installs it to `~/.local/bin/whisperx`.
If needed, add `~/.local/bin` to `PATH`.

## Quickstart

```bash
whisperx config init
whisperx models list
whisperx models install base.en
whisperx transcribe audio.mp3
```

## Commands

```bash
whisperx transcribe <file>
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

## Config file

Default path:
- Linux: `~/.config/whisperx/config.toml`
- macOS: `~/Library/Application Support/whisperx/config.toml`

Default contents:

```toml
default_model = "base.en"
model_dir = "~/.cache/whisperx/models"
whisper_bin = "whisper-cli"
ffmpeg_bin = "ffmpeg"
threads = 4
language = "en"
convert = true
timeout_secs = 3600
```

## Environment checks

Run:

```bash
whisperx doctor
```

This checks:
- `ffmpeg` availability
- `whisper-cli` availability
- model cache directory writability
- effective config summary
