# Companion Bible

Real-time scripture reference detection for live church sermons. Listens to the preacher via microphone, detects Bible verse citations using a multi-layer AI pipeline, and automatically displays the full KJV text on a congregation screen.

---

## What It Does

A pastor says _"as it is written in Romans eight twenty-eight"_ — within ~400ms the congregation screen shows:

> **Romans 8:28**
> _And we know that all things work together for good to them that love God, to them who are the called according to his purpose._

No operator action required. The system handles it automatically, with a human operator in the loop to confirm or override when confidence is low.

---

## Architecture Overview

```
Microphone
    │
    ▼
Audio Pipeline          (16 kHz mono, noise suppression, VAD)
    │
    ▼
Transcription           (Deepgram streaming  ──or──  Whisper local)
    │
    ▼
Detection Engine
    ├─ Layer 1: Pattern matching      (regex, <5 ms, always runs)
    ├─ Layer 2: Local AI              (Phi-3 Mini, 400 ms budget)
    └─ Layer 3: Cloud AI              (Claude via Anthropic API, 800 ms budget)
    │
    ▼
Arbitration + KJV Validation
    │
    ▼
┌──────────────┐      ┌─────────────────────┐
│ Operator UI  │      │  Congregation Screen │
│ (confirm /   │      │  (full-screen verse) │
│  override)   │      └─────────────────────┘
└──────────────┘
```

Two Tauri windows run simultaneously:

- **Operator** (1280×800) — sermon controls, live transcript, verse queue, manual override
- **Congregation** (1920×1080, secondary display) — full-screen verse display

---

## Monorepo Structure

```
companion-bible/
├── apps/
│   └── desktop/                    # Tauri desktop app
│       ├── src-tauri/              # Rust backend + Tauri config
│       └── src/                    # React frontend
│           ├── operator/           # Operator control panel
│           └── congregation/       # Congregation display
│
├── packages/
│   ├── audio/                      # CPAL capture, RNNoise, VAD, resampling
│   ├── transcription/              # Deepgram WS + Whisper, phonetic correction
│   ├── detection/                  # Regex pattern engine (5 confidence tiers)
│   ├── engine/                     # Pipeline orchestrator, 3-layer fusion
│   ├── context/                    # Sermon state, rolling transcript, context
│   ├── bible/                      # KJV loader, verse lookup, BibleValidator
│   ├── ai/                         # Phi-3 Mini local inference (llama-cpp-2)
│   ├── cloud-ai/                   # Anthropic Claude API client
│   ├── arbitrator/                 # Confidence arbitration + consensus boost
│   ├── calibration/                # Per-church threshold tuning
│   ├── database/                   # SQLite via SQLx, migrations, repositories
│   └── display/                    # Multi-monitor detection, screen management
│
└── shared/
    ├── events/                     # AppEvent enum (Rust + TypeScript)
    ├── errors/                     # Shared error types
    └── config/                     # Audio constants, timeouts
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

### Layer 3 — Cloud AI (Claude)

Anthropic Claude via API. Only invoked when layers 1 and 2 disagree or confidence is below the auto-display threshold. 800 ms total budget. Requires `ANTHROPIC_API_KEY`.

### Arbitration

The `ConfidenceArbitrator` combines all layer results:

- All layers agree → **10% consensus boost**
- Pattern alone ≥ 0.85 → **AutoDisplay**
- 0.65–0.85 → **Amber** (operator queue, shown with warning)
- < 0.65 → **Ignore**

Every detection is validated against the KJV database before display.

### Quotation Detection (FTS5)

If no explicit reference is found (no "John 3:16"), the system searches the KJV full-text index for the passage being _read aloud_. Catches verses the preacher quotes without citing.

---

## Tech Stack

| Layer             | Technology                             |
| ----------------- | -------------------------------------- |
| Desktop framework | Tauri 2                                |
| Frontend          | React 18, TypeScript, Vite 6           |
| Backend           | Rust 2021, Tokio async runtime         |
| Audio capture     | cpal 0.15                              |
| Audio processing  | nnnoiseless (RNNoise port)             |
| STT — primary     | Deepgram Nova-2 (WebSocket streaming)  |
| STT — fallback    | Whisper via whisper-rs (Metal GPU)     |
| Local AI          | Phi-3 Mini 4-bit via llama-cpp-2       |
| Cloud AI          | Anthropic Claude (reqwest)             |
| Database          | SQLite via SQLx (WAL mode, migrations) |
| Pattern matching  | regex crate                            |
| Bible data        | KJV JSON (bundled, ~66 books)          |

---

## Prerequisites

- macOS 12+ (primary platform; Linux/Windows untested)
- Rust 1.75+ via [rustup](https://rustup.rs)
- Node.js 20+ and npm 10+
- Xcode Command Line Tools (`xcode-select --install`)

---

## Setup

```bash
# 1. Clone
git clone https://github.com/your-org/companion-bible.git
cd companion-bible

# 2. Install JS dependencies
npm install

# 3. Build all Rust packages
~/.cargo/bin/cargo build --workspace
```

### Model Files

| Model            | Size    | Required?                | Location                                     |
| ---------------- | ------- | ------------------------ | -------------------------------------------- |
| Whisper Medium   | ~1.5 GB | Yes (if no Deepgram key) | Downloaded on first run                      |
| Phi-3 Mini 4-bit | ~2.3 GB | No (degrades gracefully) | `models/phi3/Phi-3-mini-4k-instruct-q4.gguf` |
| KJV Bible        | ~20 MB  | Yes                      | Bundled — `packages/bible/src/data/kjv.json` |

The Whisper model downloads automatically on first start if no Deepgram key is configured.

---

## Running

```bash
cd apps/desktop
npm run tauri dev
```

This starts the Vite dev server and the Tauri app together. The operator window opens immediately; the congregation window is hidden until a sermon session starts.

### First-Run Checklist

1. **Grant microphone permission** — macOS will prompt on first audio capture
2. **Enter your Deepgram API key** in the operator settings panel (recommended for best accuracy)
3. **Connect a second monitor** for the congregation display, or use it on a single screen for testing
4. Click **Start Session** to begin

---

## Configuration

All keys are entered through the operator UI and stored locally. No `.env` file needed.

| Setting           | Purpose                       | Required?   |
| ----------------- | ----------------------------- | ----------- |
| Deepgram API key  | Cloud streaming transcription | Recommended |
| Anthropic API key | Claude cloud AI (Layer 3)     | Optional    |

Without any API keys the system runs fully offline: Whisper for transcription, pattern engine + Phi-3 Mini for detection.

---

## Audio Setup

For best accuracy — especially with Nigerian/African English accents — audio input quality matters more than model choice:

- **Ideal**: lapel or headset microphone on the preacher, routed through an audio interface into the computer's line-in
- **Good**: USB microphone positioned close to the pulpit
- **Fallback**: built-in MacBook microphone (works, but more susceptible to room echo)

The app captures at 16 kHz mono. If the device's native rate differs (e.g. 48 kHz), it is resampled automatically in the capture pipeline.

---

## Transcription Accuracy

The system uses Deepgram keyword boosting for all 66 Bible book names to improve recognition of accented speech. When Deepgram is unavailable, Whisper large-v3 is the fallback — it has broader training data for diverse English accents.

Common transcription patterns handled automatically:

- "John three sixteen" → John 3:16
- "Romans eight and twenty-eight" → Romans 8:28
- "Jude one five" → Jude 1:5
- "turning to the book of Genesis chapter one" → Genesis 1

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
~/.cargo/bin/cargo test --workspace

# Performance tests (requires Phi-3 model)
~/.cargo/bin/cargo test -p companion-engine --test performance -- --ignored

# Frontend tests
cd apps/desktop && npm test
```

---

## Database

Schema migrations run automatically on startup. The database stores detection events, sermon sessions, per-church calibration state, and the full KJV verse table (used for FTS5 quotation matching).

---

## Graceful Degradation

The system is designed to keep working even when components are unavailable:

| Missing              | Behaviour                                     |
| -------------------- | --------------------------------------------- |
| No Deepgram key      | Falls back to Whisper local transcription     |
| No Phi-3 model       | Skips local AI layer, uses pattern + cloud    |
| No Anthropic key     | Skips cloud AI layer, uses pattern + local AI |
| No internet          | Fully offline: Whisper + pattern + Phi-3      |
| No secondary monitor | Congregation window stays hidden              |

---

## Project Status

Active development. Core detection pipeline, audio capture, transcription, dual-window UI, and KJV validation are complete. Semantic quotation matching (FTS5) is implemented. Calibration and operator feedback loop are functional.

Planned: Whisper large-v3 API integration for improved accent handling, semantic embedding search.
