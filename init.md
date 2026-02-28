Below is a **detailed, Codex-ready implementation plan** to “productize” whisper.cpp into a **best-DX tool** (easy install, auto model management, full advanced flags via passthrough). It’s written so you can hand it directly to **Codex CLI** as a sequence of tasks/prompts.

I’m assuming you want something like:

* `whisperx transcribe audio.mp3` → prints transcript
* `whisperx models install base.en`
* `whisperx --help` exposes friendly options
* **Advanced mode:** pass through *any* whisper.cpp flags (so you don’t lose power)
* Works on **Linux/macOS**, ideally **amd64+arm64**
* Optional: a local HTTP service later, but **start with CLI**

Codex CLI references and behavior (interactive mode, config inheritance, `-c key=value` overrides) are in the official docs. ([OpenAI Developers][1])

---

# Project scope and architecture

## Core idea

Build a **thin wrapper** around whisper.cpp that provides:

1. **Great defaults** (auto-download model, sane cache dirs, ffmpeg normalize)
2. **A stable CLI UX** (subcommands + config)
3. **Full passthrough** to whisper.cpp flags (so power users aren’t blocked)

### Recommended wrapper language

**Rust** is a strong fit because:

* Single static-ish binary per platform (nice DX)
* Easy multi-arch release builds
* Fits well with Codex CLI’s own ecosystem (Codex CLI is Rust) ([OpenAI Developers][1])

(Go would also work. But Rust + `clap` is the most “product-y” here.)

---

# Target UX (define first)

### MVP commands

* `whisperx transcribe <file>`
* `whisperx models list`
* `whisperx models install <name>`
* `whisperx models path`
* `whisperx config init`
* `whisperx doctor` (environment check)

### Friendly defaults

* Default model: `base.en` (fast)
* Cache dir: `~/.cache/whisperx/models` (Linux), `~/Library/Caches/...` (macOS)
* Converts input → `wav 16kHz mono` via ffmpeg if needed (optional flag `--no-convert`)
* Output: stdout (and optional `--output json|txt|srt|vtt`)

### Advanced passthrough

Everything after `--` gets passed to whisper.cpp verbatim:

```bash
whisperx transcribe audio.mp3 -- --beam-size 5 --max-tokens 256
```

---

# Repository plan (Codex task 0)

## Suggested repo structure

```
whisperx/
  README.md
  Cargo.toml
  src/
    main.rs
    cli.rs
    config.rs
    cache.rs
    models.rs
    transcribe.rs
    doctor.rs
  assets/
    models.json         # mapping name -> url, sha256, size, description
  scripts/
    install.sh          # one-liner install for Linux/macOS (optional)
  .github/workflows/
    release.yml         # buildx or Rust cross builds, publish GitHub Releases
```

---

# Codex execution plan (step-by-step prompts)

You can run Codex interactively (`codex`) or pass a prompt. ([OpenAI Developers][2])
Use Git checkpoints before/after each big step (Codex docs recommend this). ([OpenAI Developers][3])

---

## Phase 1 — Bootstrap the CLI skeleton

**Goal:** compile a binary with help text + command routing.

### Codex Prompt 1

> Create a Rust CLI app called `whisperx` using `clap`. Add subcommands: `transcribe`, `models`, `config`, `doctor`. For now, print parsed args for each subcommand. Add `README.md` with the command overview.

Acceptance:

* `cargo run -- --help` works
* `cargo run -- transcribe file.mp3` prints args

---

## Phase 2 — Config system (great DX + reproducibility)

**Goal:** config file + overrides.

### Design

* Config file path:

  * Linux: `~/.config/whisperx/config.toml`
  * macOS: `~/Library/Application Support/whisperx/config.toml`
* Fields:

  * `default_model = "base.en"`
  * `model_dir = "~/.cache/whisperx/models"`
  * `whisper_bin = "whisper-cli"` (or internal)
  * `ffmpeg_bin = "ffmpeg"`
  * `threads = 4`
  * `language = "en"`
  * `convert = true`

### Codex Prompt 2

> Implement config load/merge: defaults → config file → CLI flags. Add `whisperx config init` to write a default config. Add `whisperx config show` to print effective config.

Acceptance:

* Running `whisperx config init` creates config
* `whisperx config show` prints merged config

---

## Phase 3 — Model management (auto-download + caching)

**Goal:** “no manual model steps” for users.

### Design

* `assets/models.json`:

  * name: `base.en`, `small.en`, `medium.en`
  * url(s): official model URLs or your mirror
  * sha256, size
* Commands:

  * `whisperx models list`
  * `whisperx models install base.en`
  * `whisperx models ensure` (internal): download if missing, verify sha256

### Codex Prompt 3

> Implement `models` module:
>
> * Load model registry from `assets/models.json`
> * `models list` shows available models and whether installed
> * `models install <name>` downloads to model_dir with progress
> * Verify sha256
> * Make `transcribe` call `models ensure` automatically.

Acceptance:

* Delete model dir → `whisperx transcribe` downloads model automatically
* `models list` shows installed state

---

## Phase 4 — Transcription pipeline (ffmpeg normalize + whisper.cpp)

**Goal:** reliably accept “any audio file” and return text.

### Pipeline

1. Resolve model path (ensured installed)
2. If `convert=true`, run:

   * `ffmpeg -i input -ar 16000 -ac 1 -c:a pcm_s16le tmp.wav`
3. Run whisper.cpp:

   * `whisper-cli -m <model> -f <wav> -l en` plus extra args
4. Capture stdout and print transcript cleanly

### Codex Prompt 4

> Implement `transcribe`:
>
> * If convert enabled, convert to temp WAV (unique file in /tmp)
> * Execute whisper-cli and capture output
> * Extract transcript text robustly (avoid timing logs if present)
> * Print text to stdout
> * Add `--output` option (txt/json later) and `--no-convert`.

Acceptance:

* `whisperx transcribe sample.mp3` prints plain text
* Works with mp3/wav/ogg if ffmpeg installed

---

## Phase 5 — Advanced passthrough (key requirement)

**Goal:** preserve whisper.cpp’s full power.

### Spec

* Everything after `--` is passed as additional args to `whisper-cli`.
* Also support a friendly set of “common knobs”:

  * `--threads`, `--language`, `--translate`, etc.
* Friendly knobs should map to whisper flags unless user overrides in passthrough.

### Codex Prompt 5

> Add passthrough support: parse args after `--` and append them verbatim to the whisper-cli invocation. Document this in README with examples.

Acceptance:

* `whisperx transcribe a.wav -- --beam-size 5 --max-tokens 256` works
* No wrapper limitations

---

## Phase 6 — Packaging (best DX delivery)

You have three distribution layers; implement in this order:

### Layer 1: GitHub Releases (fastest to ship)

* Build and attach binaries for:

  * linux-amd64
  * linux-arm64
  * macos-amd64
  * macos-arm64

### Codex Prompt 6

> Create a GitHub Actions workflow to build and upload release artifacts for linux/macOS amd64/arm64 on tag. Include checksums.

Acceptance:

* Tag push creates release artifacts

---

### Layer 2: One-line installer (Linux/macOS)

* `curl -fsSL https://…/install.sh | bash`
* The script detects OS/arch, downloads the correct release, installs to `~/.local/bin`.

### Codex Prompt 7

> Add `scripts/install.sh` that detects OS/arch, downloads latest release from GitHub Releases, installs to ~/.local/bin, and prints post-install instructions.

Acceptance:

* Fresh machine: install in one command

---

### Layer 3: Package managers (optional but “premium DX”)

* Homebrew formula (macOS/Linux)
* `.deb` package for Ubuntu
* (Later) AUR for Arch

---

## Phase 7 — “Doctor” command (support burden reducer)

### Codex Prompt 8

> Implement `whisperx doctor`:
>
> * check ffmpeg presence
> * check whisper-cli presence (or bundled)
> * check model dir writable
> * print actionable fixes
> * print effective config summary.

Acceptance:

* Users can self-diagnose missing deps quickly

---

# Security & robustness checklist (Codex prompt bundle)

### Codex Prompt 9

> Harden the tool:
>
> * handle spaces/unicode in file paths
> * clean up temp files
> * timeouts for ffmpeg and whisper invocations
> * return non-zero exit codes on failure
> * structured errors (thiserror/anyhow)
> * add unit tests for config/model registry parsing

---

# Codex usage tips (so you actually get good output)

Codex CLI:

* Use interactive mode for large refactors: `codex` ([OpenAI Developers][2])
* Make Git checkpoints before tasks (Codex docs explicitly recommend this). ([OpenAI Developers][3])
* If you need consistent settings, Codex CLI supports config files and per-run overrides. ([OpenAI Developers][4])
* For automation/CI, use API key auth and avoid exposing execution publicly. ([OpenAI Developers][5])

---

# Optional next step: bundle whisper.cpp itself (even less user setup)

Right now the wrapper assumes `whisper-cli` exists (from a whisper.cpp build). To reduce friction further:

* Ship a **bundled whisper.cpp binary** alongside `whisperx` in releases (or statically link it)
* Then users install *one tool* and never compile whisper.cpp

This increases release complexity but improves DX drastically.

If you want this, the Codex plan adds:

* Build whisper.cpp as part of CI
* Package `whisper-cli` into the release
* Wrapper uses the bundled binary by default, falling back to system `whisper-cli`

---

# If you tell me 2 choices, I can lock the plan into concrete files

1. Wrapper language: **Rust** (recommended) or **Go**
2. Distribution target: **Linux only** vs **Linux + macOS**

Then I’ll produce the exact Codex “task prompts” plus the initial `Cargo.toml`, `main.rs`, and a first-pass `assets/models.json` you can drop in and let Codex iterate on.

[1]: https://developers.openai.com/codex/cli/?utm_source=chatgpt.com "Codex CLI"
[2]: https://developers.openai.com/codex/cli/features/?utm_source=chatgpt.com "Codex CLI features"
[3]: https://developers.openai.com/codex/quickstart/?utm_source=chatgpt.com "Quickstart"
[4]: https://developers.openai.com/codex/cli/reference/?utm_source=chatgpt.com "Codex CLI reference"
[5]: https://developers.openai.com/codex/auth/?utm_source=chatgpt.com "Authentication"
