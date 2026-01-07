//! Build script to generate ecosystem template list at compile time.
//!
//! Scans the ecosystems/ directory and generates Rust code that includes
//! all .gitignore files found there.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("ecosystems_generated.rs");
    let mut f = File::create(&dest_path).unwrap();

    let ecosystems_dir = Path::new("ecosystems");

    // Tell cargo to rerun if ecosystems directory changes
    println!("cargo:rerun-if-changed=ecosystems");

    if !ecosystems_dir.exists() {
        // No ecosystems directory - generate empty list
        writeln!(f, "pub const ECOSYSTEMS: &[(&str, &str)] = &[];").unwrap();
        writeln!(f, "pub const MANIFEST: &str = \"# No ecosystems downloaded\\n# Run: make fetch-ecosystems\";").unwrap();
        return;
    }

    // Read manifest
    let manifest_path = ecosystems_dir.join("MANIFEST");
    if manifest_path.exists() {
        writeln!(f, "pub const MANIFEST: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/ecosystems/MANIFEST\"));").unwrap();
    } else {
        writeln!(f, "pub const MANIFEST: &str = \"# No manifest\";").unwrap();
    }

    // Collect all .gitignore files
    let mut ecosystems: Vec<String> = Vec::new();

    if let Ok(entries) = fs::read_dir(ecosystems_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "gitignore" {
                    if let Some(stem) = path.file_stem() {
                        let name = stem.to_string_lossy().to_string();
                        ecosystems.push(name);
                        // Tell cargo to rerun if this file changes
                        println!("cargo:rerun-if-changed={}", path.display());
                    }
                }
            }
        }
    }

    // Sort alphabetically (case-insensitive)
    ecosystems.sort_by_key(|a| a.to_lowercase());

    // Generate the ECOSYSTEMS array
    writeln!(f, "pub const ECOSYSTEMS: &[(&str, &str)] = &[").unwrap();
    for name in &ecosystems {
        writeln!(
            f,
            "    (\"{name}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/ecosystems/{name}.gitignore\"))),"
        ).unwrap();
    }
    writeln!(f, "];").unwrap();
}
