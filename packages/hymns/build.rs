use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let hymns_dir = manifest.join("../../data/Hymns");

    println!("cargo:rerun-if-changed={}", hymns_dir.display());

    let mut entries: Vec<(u16, String, String)> = vec![];

    let dir = fs::read_dir(&hymns_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", hymns_dir.display()));

    for entry in dir {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let filename = path.file_stem().and_then(|s| s.to_str()).expect("filename");

        // Filename format: "234 Title Of Hymn"
        let (num_str, title) = filename
            .split_once(' ')
            .unwrap_or_else(|| panic!("bad filename: {filename}"));

        let number: u16 = num_str
            .parse()
            .unwrap_or_else(|_| panic!("non-numeric prefix: {num_str}"));

        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        entries.push((number, title.to_string(), content));
    }

    // Sort by hymn number.
    entries.sort_by_key(|(n, _, _)| *n);

    // Generate Rust source.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("hymns_data.rs");
    let mut src = String::from("pub static HYMNS_RAW: &[(u16, &str, &str)] = &[\n");

    for (number, title, content) in &entries {
        let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
        let escaped_content = content.replace('\\', "\\\\").replace('"', "\\\"");
        src.push_str(&format!(
            "    ({number}, \"{escaped_title}\", \"{escaped_content}\"),\n"
        ));
    }

    src.push_str("];\n");
    fs::write(&out, src).expect("write hymns_data.rs");
}
