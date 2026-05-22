# Companion Bible

Real-time scripture reference detection and Gospel Hymns & Songs (GHS) display for live church services. Listens to the preacher via microphone, detects Bible verse citations using a multi-layer AI pipeline, and automatically displays the full KJV text on a congregation screen. Also detects GHS hymn numbers from speech and displays stanzas and choruses in sequence.

---

## What It Does

**Bible verse detection** — a pastor says _"as it is written in Romans eight twenty-eight"_ — within ~400ms the congregation screen shows:

> **Romans 8:28**
> _And we know that all things work together for good to them that love God, to them who are the called according to his purpose._

**GHS hymn display** — a worship leader says _"open GHS two hundred and thirty four"_ — the congregation screen immediately shows the first stanza. Each subsequent stanza advances automatically when the song leader reaches the last line of the current section, or the operator can advance manually.

No operator action required for detection. A human operator stays in the loop to confirm, override, or manually load content at any time.

---

## Prerequisites

- macOS 12+ (primary platform; Linux/Windows untested)
- Rust 1.75+ via [rustup](https://rustup.rs)
- Node.js 20+ and npm 10+
- Xcode Command Line Tools (`xcode-select --install`)

---

## Setup

### 1. Install system dependencies

**macOS**

```bash
# Install Xcode Command Line Tools (compiler, linker, Metal SDK)
xcode-select --install

# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Install Node.js 20+ (via nvm or direct download from nodejs.org)
# https://github.com/nvm-sh/nvm
nvm install 20 && nvm use 20
```

> **Linux / Windows** — untested. Tauri's [prerequisites guide](https://tauri.app/start/prerequisites/) lists the packages needed for each platform.

### 2. Clone the repository

```bash
git clone https://github.com/your-org/companion-bible.git
cd companion-bible
```

### 3. Install JavaScript dependencies

```bash
npm install
```

This installs all frontend packages (React, Vite, TypeScript) and the Tauri CLI for the workspace.

### 4. Build all Rust packages

```bash
cargo build --workspace
```

This compiles every crate in the monorepo — `audio`, `transcription`, `detection`, `engine`, `bible`, `hymns`, and the Tauri backend. First build takes several minutes; subsequent builds are incremental.

### 5. (Optional) Download the Phi-3 Mini model

The local AI layer is optional — the system degrades gracefully without it. If you want on-device AI verse detection:

```bash
mkdir -p models/phi3
# Download Phi-3-mini-4k-instruct-q4.gguf (~2.3 GB) from HuggingFace and place it at:
# models/phi3/Phi-3-mini-4k-instruct-q4.gguf
```

### Model Files

| Model            | Size    | Required?                 | Location                                     |
| ---------------- | ------- | ------------------------- | -------------------------------------------- |
| Whisper Medium   | ~1.5 GB | Yes (if no cloud STT key) | Downloaded on first run                      |
| Phi-3 Mini 4-bit | ~2.3 GB | No (degrades gracefully)  | `models/phi3/Phi-3-mini-4k-instruct-q4.gguf` |
| KJV Bible        | ~20 MB  | Yes                       | Bundled — `packages/bible/src/data/kjv.json` |
| GHS Hymns        | <1 MB   | Yes                       | Bundled — `data/Hymns/` (260 .txt files)     |

---

## Running

```bash
cd apps/desktop
npm run tauri dev
```

This starts the Vite dev server and the Tauri app together. The operator window opens immediately; the congregation window is hidden until a session starts.

### First-Run Checklist

1. **Grant microphone permission** — macOS will prompt on first audio capture
2. **Enter API keys** in the operator settings panel (AssemblyAI recommended for best accuracy)
3. **Connect a second monitor** for the congregation display
4. Click **Start Session** to begin

---

## Configuration

All keys are entered through the operator UI and stored locally. No `.env` file needed.

| Setting        | Purpose                       | Required?   |
| -------------- | ----------------------------- | ----------- |
| AssemblyAI key | Cloud streaming transcription | Recommended |
| Deepgram key   | Fallback cloud transcription  | Optional    |
| OpenAI key     | Primary cloud verse detection | Recommended |
| Anthropic key  | Fallback cloud AI (Claude)    | Optional    |

Without any API keys the system runs fully offline: Whisper for transcription, pattern engine + Phi-3 Mini for detection.

---

## Architecture Overview

```
Microphone
    │
    ▼
Audio Pipeline          (48 kHz → RNNoise → 16 kHz mono, VAD)
    │
    ▼
Transcription           (AssemblyAI streaming ──or── Deepgram ──or── Whisper local)
    │
    ├─────────────────────────────────────────────┐
    ▼                                             ▼
Detection Engine                         Hymn Detection
    ├─ Layer 1: Pattern matching          detect_hymn_number()
    ├─ Layer 2: Local AI (Phi-3 Mini)          │
    └─ Layer 3: Cloud AI (OpenAI/Claude)   HymnSession
    │                                     (auto-advance on
    ▼                                      last-line match)
Arbitration + KJV Validation                   │
    │                                          │
    └─────────────┬─────────────────────────────┘
                  ▼
    ┌──────────────────┐      ┌─────────────────────────┐
    │   Operator UI    │      │   Congregation Screen   │
    │ (confirm/override│      │  Bible verse  ──or──    │
    │  mode toggle     │      │  GHS stanza / chorus    │
    │  manual load)    │      └─────────────────────────┘
    └──────────────────┘
```

Two Tauri windows run simultaneously:

- **Operator** (1280×800) — sermon controls, live transcript, verse queue, manual verse override, GHS manual load, mode toggle (Bible ↔ GHS), Next Stanza button
- **Congregation** (1920×1080, secondary display) — full-screen verse or hymn stanza display

---

## Monorepo Structure

```
companion-bible/
├── apps/
│   └── desktop/                    # Tauri desktop app
│       ├── src-tauri/              # Rust backend + Tauri config
│       └── src/                    # TypeScript frontend
│           ├── operator/           # Operator control panel
│           └── congregation/       # Congregation display
│
├── packages/
│   ├── audio/                      # CPAL capture, RNNoise, VAD, resampling
│   ├── transcription/              # AssemblyAI / Deepgram WS + Whisper
│   ├── detection/                  # Regex pattern engine + hymn number detector
│   ├── engine/                     # Pipeline orchestrator, 3-layer fusion, HymnSession
│   ├── hymns/                      # GHS hymn book (260 hymns, compile-time embed)
│   ├── context/                    # Sermon state, rolling transcript, context
│   ├── bible/                      # KJV loader, verse lookup, BibleValidator
│   ├── ai/                         # Phi-3 Mini local inference (llama-cpp-2)
│   ├── cloud-ai/                   # OpenAI + Anthropic Claude API clients
│   ├── arbitrator/                 # Confidence arbitration + consensus boost
│   ├── calibration/                # Per-church threshold tuning
│   ├── database/                   # SQLite via SQLx, migrations, repositories
│   └── display/                    # Multi-monitor detection, screen management
│
├── data/
│   └── Hymns/                      # 260 GHS hymn text files (N Title.txt)
│
└── shared/
    ├── events/                     # AppEvent enum (Rust + TypeScript)
    ├── errors/                     # Shared error types
    ├── config/                     # Audio constants, timeouts
    └── types/                      # TypeScript type definitions
```

---

## Detection Pipeline (Deep Dive)

### Layer 1 — Pattern Engine

Runs synchronously on every transcription segment. Six regex tiers, highest-confidence match wins:

| Tier                 | Example                   | Confidence |
| -------------------- | ------------------------- | ---------- |
| Full canonical       | "John 3:16"               | 1.00       |
| Book chapter verse   | "John chapter 3 verse 16" | 0.95       |
| Space-separated      | "John 3 16"               | 0.90       |
| Book and separator   | "Genesis 1 and 1"         | 0.90       |
| Book + chapter only  | "turning to Romans 8"     | 0.70       |
| Chapter + verse only | "verse 28" (with context) | 0.60       |

Also handles spoken numbers ("three sixteen"), "and" as verse separator, and references fragmented across multiple utterances via a **rolling transcript buffer**.

### Layer 2 — Local AI (Phi-3 Mini)

Phi-3 Mini 4-bit quantized model runs on-device via llama-cpp-2 with Metal GPU acceleration on macOS. Catches references the pattern engine misses — paraphrases, unusual phrasing. 400 ms budget.

### Layer 3 — Cloud AI (OpenAI / Claude)

OpenAI is the primary cloud layer; Anthropic Claude is the fallback. Only invoked when pattern + local AI results need reinforcement or confidence is below the auto-display threshold. 800 ms total budget.

### Arbitration

The `ConfidenceArbitrator` combines all layer results:

- All layers agree → **10% consensus boost**
- Pattern alone ≥ 0.85 → **AutoDisplay**
- 0.65–0.85 → **Amber** (operator queue, shown with warning)
- < 0.65 → **Ignore**

Every detection is validated against the KJV database before display.

### Quotation Detection (FTS5)

If no explicit reference is found, the system searches the KJV full-text index for the passage being _read aloud_. Catches verses the preacher quotes without citing.

---

## GHS Hymn Display

The system supports the **Gospel Hymns and Songs (GHS)** hymn book — 260 hymns embedded at compile time from `data/Hymns/`.

### Detection

Hymn numbers are detected from speech in any of these forms:

| Spoken form                                 | Detected as |
| ------------------------------------------- | ----------- |
| "GHS 234"                                   | Hymn 234    |
| "open GHS 234"                              | Hymn 234    |
| "GHS two hundred and thirty four"           | Hymn 234    |
| "Gospel Hymns and Sound number 234"         | Hymn 234    |
| "Gospel Hymns and Songs number two hundred" | Hymn 200    |

### Stanza Flow

Hymns with a chorus: **Stanza 1 → Chorus → Stanza 2 → Chorus → … → Stanza N → Chorus → Stop**

Hymns without a chorus: **Stanza 1 → Stanza 2 → … → Stanza N → Stop**

### Advancement

- **Auto** — when the song leader sings the last line of the current section (70% word-overlap fuzzy match), the display advances to the next section automatically
- **Manual** — operator presses **Next Stanza** button at any time
- **Manual load** — operator types a hymn number (e.g. `42` or `GHS 42`) in the Load Hymn input and presses Show — works even without an active audio session

### Mode Toggle

The operator can switch between **Bible Mode** and **GHS Mode** at any time. Speech detection for both runs continuously regardless of mode.

---

## Tech Stack

| Layer             | Technology                                     |
| ----------------- | ---------------------------------------------- |
| Desktop framework | Tauri 2                                        |
| Frontend          | React 18, TypeScript, Vite 6                   |
| Backend           | Rust 2021, Tokio async runtime                 |
| Audio capture     | cpal 0.15                                      |
| Audio processing  | nnnoiseless (RNNoise port)                     |
| STT — primary     | AssemblyAI (WebSocket streaming)               |
| STT — secondary   | Deepgram Nova-2 (WebSocket streaming)          |
| STT — fallback    | Whisper via whisper-rs (Metal GPU)             |
| Local AI          | Phi-3 Mini 4-bit via llama-cpp-2               |
| Cloud AI          | OpenAI (primary) + Anthropic Claude (fallback) |
| Database          | SQLite via SQLx (WAL mode, migrations)         |
| Pattern matching  | regex crate                                    |
| Bible data        | KJV JSON (bundled, ~66 books)                  |
| Hymn data         | 260 GHS hymns (compile-time embedded)          |

---

## Audio Setup

For best accuracy — especially with Nigerian/African English accents — audio input quality matters:

- **Ideal**: lapel or headset microphone on the preacher/worship leader, routed through an audio interface
- **Good**: USB microphone positioned close to the pulpit or choir leader
- **Fallback**: built-in device microphone (works, but more susceptible to room echo)

The app captures at 48 kHz through the noise suppression pipeline (RNNoise), then resamples to 16 kHz for transcription.

---

## Transcription Accuracy

The system is optimised for Nigerian and African English accents. AssemblyAI is the recommended backend for best accuracy with diverse accents. Common patterns handled automatically:

- "John three sixteen" → John 3:16
- "Romans eight and twenty-eight" → Romans 8:28
- "Jude one five" → Jude 1:5
- "turning to the book of Genesis chapter one" → Genesis 1
- "GHS two hundred and thirty four" → Hymn 234
- "open Gospel Hymns and Sound number sixty" → Hymn 60

---

## Building for Production

```bash
cd apps/desktop
npm run tauri build
```

Outputs a signed `.app` (macOS), `.exe` (Windows), or `.deb`/`.rpm` (Linux) in `apps/desktop/src-tauri/target/release/bundle/`.

---

## Running Tests

```bash
# Rust unit + integration tests
cargo test --workspace

# Performance tests (requires Phi-3 model)
cargo test -p companion-engine --test performance -- --ignored

# Frontend tests
cd apps/desktop && npm test
```

---

## Database

Schema migrations run automatically on startup. The database stores detection events, sermon sessions, per-church calibration state, and the full KJV verse table (used for FTS5 quotation matching).

---

## Graceful Degradation

| Missing              | Behaviour                                      |
| -------------------- | ---------------------------------------------- |
| No AssemblyAI key    | Falls back to Deepgram, then Whisper           |
| No Deepgram key      | Falls back to Whisper local transcription      |
| No Phi-3 model       | Skips local AI layer, uses pattern + cloud     |
| No OpenAI key        | Skips OpenAI, uses pattern + local AI + Claude |
| No Anthropic key     | Skips Claude fallback, uses pattern + local AI |
| No internet          | Fully offline: Whisper + pattern + Phi-3       |
| No secondary monitor | Congregation window stays hidden               |

---

## Contributing

Contributions are welcome! If you spot a bug, have an idea for improvement, or want to add a feature, please open a pull request.

- Fork the repository and create a branch from `main`
- Make your changes with clear, focused commits
- Ensure `cargo clippy --all -- -D warnings` and `cargo test --workspace` pass
- Ensure `cd apps/desktop && npm test` passes
- Open a PR describing what you changed and why

If you are unsure whether an idea fits the project, open an issue first to discuss it.

---

## Acknowledgements

The GHS hymn text data used in this project is sourced from [gospel-hymns](https://github.com/marvinjude/gospel-hymns) by [@marvinjude](https://github.com/marvinjude). Many thanks for making the Gospel Hymns and Songs text freely available.

---

## Project Status

Active development. Core detection pipeline, audio capture, transcription, dual-window UI, KJV validation, semantic quotation matching (FTS5), and GHS hymn display are complete. Calibration and operator feedback loop are functional.
