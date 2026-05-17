//! Phi-3 inference worker — runs as a subprocess of companion-desktop.
//!
//! Protocol:
//!   stdin  — one JSON line per request: {"text":"...","book":"...","chapter":3,"recent":"..."}
//!   stdout — one JSON line per response: {"book":"...","chapter":3,"verse":16,"confidence":0.9}
//!
//! Usage: phi3-worker <model_path>

use std::io::{self, BufRead, Write};

use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    sampling::LlamaSampler,
};
use serde::{Deserialize, Serialize};

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Request {
    text: String,
    book: Option<String>,
    chapter: Option<u8>,
    recent: String,
}

#[derive(Serialize, Deserialize)]
struct Response {
    book: Option<String>,
    chapter: Option<u8>,
    verse: Option<u8>,
    confidence: f32,
}

impl Response {
    fn empty() -> Self {
        Self { book: None, chapter: None, verse: None, confidence: 0.0 }
    }
}

// ─── Phi-3 prompt format ──────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = "\
You are a Bible reference assistant for a live sermon transcription system. \
Your ONLY task is to identify the most likely scripture reference being discussed \
and return it as a single JSON object.\n\
\n\
Rules:\n\
1. Respond with ONLY valid JSON — no prose, no explanation, no markdown.\n\
2. If you cannot determine the reference with confidence, set \"book\" to null.\n\
3. NEVER invent or hallucinate verse content. Only identify the reference.\n\
4. Do not repeat or paraphrase the transcript.\n\
5. Output schema: {\"book\":\"<string|null>\",\"chapter\":<int|null>,\"verse\":<int|null>,\"confidence\":<0.0-1.0>}";

fn build_prompt(req: &Request) -> String {
    let mut user = String::new();
    if let Some(book) = &req.book {
        user.push_str(&format!("Current sermon book: {book}"));
        if let Some(ch) = req.chapter {
            user.push_str(&format!(", chapter {ch}"));
        }
        user.push('\n');
    }
    if !req.recent.is_empty() {
        user.push_str("Recent transcript:\n");
        user.push_str(&req.recent);
        user.push('\n');
    }
    user.push_str("Segment: ");
    user.push_str(&req.text);

    format!(
        "<|system|>\n{SYSTEM_PROMPT}\n<|end|>\n<|user|>\n{user}\n<|end|>\n<|assistant|>\n"
    )
}

// ─── Inference ────────────────────────────────────────────────────────────────

const CTX_SIZE: u32 = 4_096;
const MAX_NEW_TOKENS: usize = 128;

fn infer(backend: &LlamaBackend, model: &LlamaModel, prompt: &str) -> Response {
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(std::num::NonZeroU32::new(CTX_SIZE));

    let mut ctx = match model.new_context(backend, ctx_params) {
        Ok(c) => c,
        Err(_) => return Response::empty(),
    };

    let tokens = match model.str_to_token(prompt, AddBos::Always) {
        Ok(t) => t,
        Err(_) => return Response::empty(),
    };

    let n_prompt = tokens.len();
    let mut batch = LlamaBatch::new(CTX_SIZE as usize, 1);
    for (i, &tok) in tokens.iter().enumerate() {
        if batch.add(tok, i as i32, &[0], i == n_prompt - 1).is_err() {
            return Response::empty();
        }
    }
    if ctx.decode(&mut batch).is_err() {
        return Response::empty();
    }

    let mut sampler = LlamaSampler::greedy();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut output = String::new();
    let mut n_cur = n_prompt as i32;

    for _ in 0..MAX_NEW_TOKENS {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        if token == model.token_eos() {
            break;
        }
        if let Ok(piece) = model.token_to_piece(token, &mut decoder, true, None) {
            output.push_str(&piece);
        }
        batch.clear();
        if batch.add(token, n_cur, &[0], true).is_err() {
            break;
        }
        if ctx.decode(&mut batch).is_err() {
            break;
        }
        n_cur += 1;
        if output.trim_end().ends_with('}') {
            break;
        }
    }

    parse_response(&output)
}

fn parse_response(raw: &str) -> Response {
    let start = match raw.find('{') {
        Some(i) => i,
        None => return Response::empty(),
    };
    let end = match raw.rfind('}') {
        Some(i) => i,
        None => return Response::empty(),
    };
    serde_json::from_str(&raw[start..=end]).unwrap_or_else(|_| Response::empty())
}

// ─── Main loop ────────────────────────────────────────────────────────────────

fn main() {
    let model_path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("Usage: phi3-worker <model_path>");
            std::process::exit(1);
        }
    };

    let backend = match LlamaBackend::init() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("phi3-worker: failed to init backend: {e}");
            std::process::exit(1);
        }
    };

    let model = match LlamaModel::load_from_file(&backend, &model_path, &LlamaModelParams::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("phi3-worker: failed to load model from {model_path}: {e}");
            std::process::exit(1);
        }
    };

    eprintln!("phi3-worker: model loaded, ready");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => {
                let _ = writeln!(out, "{}", serde_json::to_string(&Response::empty()).unwrap());
                let _ = out.flush();
                continue;
            }
        };

        let prompt = build_prompt(&req);
        let resp = infer(&backend, &model, &prompt);

        let json = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".into());
        let _ = writeln!(out, "{json}");
        let _ = out.flush();
    }
}
