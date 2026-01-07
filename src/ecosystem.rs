//! Ecosystem-specific gitignore templates.
//!
//! Provides standard gitignore templates for common project types.
//! Templates are baked into the binary from GitHub's gitignore repository.
//!
//! To update templates: `make fetch-ecosystems`

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

// Include the generated ecosystem list
include!(concat!(env!("OUT_DIR"), "/ecosystems_generated.rs"));

/// Returns the cache directory for ecosystem templates.
fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("raptar").join("ecosystems"))
}

/// Gets an ecosystem template by name.
/// Writes the baked-in template to a temp file and returns its path.
pub fn get_template(name: &str) -> Result<PathBuf> {
    // Find the ecosystem (case-insensitive)
    let (_, content) = ECOSYSTEMS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .with_context(|| {
            format!("Unknown ecosystem: {name}. Run --list-ecosystems to see available options.")
        })?;

    // Write to cache directory
    let cache = cache_dir().context("Could not determine cache directory")?;
    fs::create_dir_all(&cache)?;

    let path = cache.join(format!("{name}.gitignore"));
    let mut file = fs::File::create(&path)?;
    file.write_all(content.as_bytes())?;

    Ok(path)
}

/// Loads ecosystem templates and returns their paths.
pub fn load_ecosystem_templates(
    ecosystems: &[String],
    verbose: bool,
    _explicit: bool,
) -> Vec<PathBuf> {
    use colored::Colorize;

    let mut paths = Vec::new();

    for ecosystem in ecosystems {
        match get_template(ecosystem) {
            Ok(path) => {
                if verbose {
                    eprintln!(
                        "{} Using {} ecosystem template",
                        "📦".green(),
                        ecosystem.cyan()
                    );
                }
                paths.push(path);
            }
            Err(e) => {
                eprintln!("{} {}", "⚠".yellow(), e);
            }
        }
    }

    paths
}

/// Prints the list of available ecosystems.
pub fn print_ecosystem_list() {
    use colored::Colorize;

    if ECOSYSTEMS.is_empty() {
        println!("{}", "No ecosystem templates available.".yellow());
        println!(
            "Run {} to download templates.",
            "make fetch-ecosystems".cyan()
        );
        return;
    }

    // Extract date from manifest
    let date = MANIFEST
        .lines()
        .find(|l| l.starts_with("# Downloaded:"))
        .map_or("unknown", |l| l.trim_start_matches("# Downloaded:").trim());

    println!("{}", "Available ecosystem templates:".bold());
    println!(
        "{}",
        format!("(from github/gitignore, downloaded {date})").dimmed()
    );
    println!();

    for (name, _) in ECOSYSTEMS {
        println!("  {name}");
    }

    println!();
    println!(
        "{} ecosystems available. Use {} to apply.",
        ECOSYSTEMS.len(),
        "--with-ecosystem <NAME>".cyan()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecosystems_sorted() {
        let names: Vec<_> = ECOSYSTEMS.iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|a| a.to_lowercase());
        assert_eq!(names, sorted, "ECOSYSTEMS should be sorted alphabetically");
    }

    #[test]
    fn test_ecosystems_not_empty() {
        // This will fail if ecosystems/ directory is missing
        // That's intentional - run `make fetch-ecosystems` first
        assert!(
            !ECOSYSTEMS.is_empty(),
            "No ecosystems found. Run: make fetch-ecosystems"
        );
    }

    #[test]
    fn test_get_template_case_insensitive() {
        if ECOSYSTEMS.is_empty() {
            return; // Skip if no ecosystems
        }
        let first = ECOSYSTEMS[0].0;
        assert!(get_template(first).is_ok());
        assert!(get_template(&first.to_lowercase()).is_ok());
        assert!(get_template(&first.to_uppercase()).is_ok());
    }

    #[test]
    fn test_get_template_unknown() {
        let result = get_template("NotARealEcosystem");
        assert!(result.is_err());
    }
}
