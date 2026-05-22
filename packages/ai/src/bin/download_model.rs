use std::path::PathBuf;

use companion_ai::manager::{LocalAIManager, SetupProgress};

fn main() {
    let models_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // packages/ai
        .unwrap()
        .parent() // packages
        .unwrap()
        .parent() // project root
        .unwrap()
        .join("models");

    let manager = LocalAIManager::new(&models_dir);

    println!("Models directory: {}", models_dir.display());

    manager
        .setup(|progress| match &progress {
            SetupProgress::Checking => print!("Checking… "),
            SetupProgress::AlreadyPresent => println!("already present, skipping download."),
            SetupProgress::Downloading {
                bytes_done,
                bytes_total,
            } => {
                if let Some(total) = bytes_total {
                    let pct = (*bytes_done as f64 / *total as f64 * 100.0) as u8;
                    print!(
                        "\rDownloading… {pct}% ({} / {} MB)   ",
                        bytes_done / 1_048_576,
                        total / 1_048_576
                    );
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                } else {
                    print!("\rDownloading… {} MB", bytes_done / 1_048_576);
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
            }
            SetupProgress::Verifying => {
                println!();
                print!("Verifying SHA-256… ");
            }
            SetupProgress::Loading => println!("ok\nLoading model into memory…"),
            SetupProgress::Ready { model_path } => {
                println!("Ready — model at {}", model_path.display());
            }
        })
        .unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        });
}
