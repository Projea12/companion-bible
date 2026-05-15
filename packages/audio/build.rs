use std::path::Path;
use std::process::Command;

const MODEL_DIR: &str = "models";
const MODEL_FILE: &str = "models/silero_vad.onnx";
// Pinned to v4.0 tag for reproducibility.
const MODEL_URL: &str =
    "https://github.com/snakers4/silero-vad/raw/v4.0/files/silero_vad.onnx";

fn main() {
    println!("cargo:rerun-if-changed={MODEL_FILE}");

    // Only download when the neural-vad feature is requested.
    if std::env::var("CARGO_FEATURE_NEURAL_VAD").is_err() {
        return;
    }

    if Path::new(MODEL_FILE).exists() {
        return;
    }

    std::fs::create_dir_all(MODEL_DIR)
        .expect("failed to create models/ directory");

    // Try curl (macOS / Linux / Windows with Git Bash).
    let ok = Command::new("curl")
        .args(["--fail", "--location", "--silent", "--output", MODEL_FILE, MODEL_URL])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok {
        println!("cargo:warning=Downloaded Silero VAD model to {MODEL_FILE}");
        return;
    }

    // Fall back to wget.
    let ok = Command::new("wget")
        .args(["--quiet", "-O", MODEL_FILE, MODEL_URL])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok {
        println!("cargo:warning=Downloaded Silero VAD model to {MODEL_FILE}");
        return;
    }

    panic!(
        "\n\nFailed to download the Silero VAD model automatically.\n\
         Please download it manually and place it at:\n\
         packages/audio/{MODEL_FILE}\n\n\
         curl -L -o {MODEL_FILE} \"{MODEL_URL}\"\n"
    );
}
