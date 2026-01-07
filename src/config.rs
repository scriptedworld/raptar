//! Configuration file handling.

use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration file structure.
///
/// Located at `~/.config/raptar/config.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ignore: IgnoreConfig,
    pub defaults: DefaultsConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct IgnoreConfig {
    /// List of ignore files to honor (e.g., `.dockerignore`, `.npmignore`).
    #[serde(rename = "use")]
    pub use_files: Vec<String>,

    /// Patterns to always exclude, regardless of other ignore files.
    pub always_exclude: Vec<String>,

    /// Patterns to always include (force include).
    pub always_include: Vec<String>,
}

/// Default exclusions that almost nobody wants in an archive.
const DEFAULT_ALWAYS_EXCLUDE: &[&str] = &[".git/**", ".hg/**", ".svn/**"];

impl Default for IgnoreConfig {
    fn default() -> Self {
        Self {
            use_files: Vec::new(),
            always_exclude: DEFAULT_ALWAYS_EXCLUDE
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            always_include: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DefaultsConfig {
    /// Default output format
    pub format: Option<String>,
    /// Default to reproducible archives
    pub reproducible: bool,
    /// Follow symlinks by default
    pub dereference: bool,
    /// Preserve ownership by default
    pub preserve_owner: bool,
}

/// Returns the path to the config file.
/// Always uses ~/.config/raptar/config.toml for consistency across platforms.
pub fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|d| d.join(".config").join("raptar").join("config.toml"))
}

/// Check if config file exists.
pub fn config_exists() -> bool {
    config_path().is_some_and(|p| p.exists())
}

/// Loads the config file, returning defaults if not found.
pub fn load_config() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };

    if !path.exists() {
        return Config::default();
    }

    fs::read_to_string(&path).map_or_else(
        |_| Config::default(),
        |contents| toml::from_str(&contents).unwrap_or_default(),
    )
}

/// Creates a default config file.
pub fn init_config() -> Result<PathBuf> {
    let path = config_path().context("Could not determine config directory")?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let default_config = r#"# raptar configuration
# Location: ~/.config/raptar/config.toml

[ignore]
# Additional ignore files to honor by default (any gitignore-format file)
# use = [".dockerignore", ".npmignore"]

# Patterns to ALWAYS exclude, regardless of other ignore files.
# Uses gitignore syntax. Use ** to match directory contents.
# Can be disabled per-run with --without-exclude-always
always_exclude = [
    # Version control internals
    ".git/**",
    ".hg/**",
    ".svn/**",
    
    # IDE/Editor directories  
    ".idea/**",
    ".vscode/**",
    "*.swp",
    
    # OS files
    ".DS_Store",
    "Thumbs.db",
    
    # Common build artifacts (uncomment if desired)
    # "node_modules/**",
    # "__pycache__/**",
    # "*.pyc",
    # "dist/**",
    # "build/**",
]

# Patterns to ALWAYS include (force include).
# Overrides 'always_exclude' patterns and ignore files.
# CLI --with-include takes highest priority.
# always_include = ["important.log", "dist/release.tar.gz"]

[defaults]
# Default output format (tar, tar.gz, tar.bz2, tar.zst, zip)
# format = "tar.gz"

# Always create reproducible archives
# reproducible = false

# Follow symlinks by default
# dereference = false

# Preserve file ownership by default
# preserve_owner = false
"#;

    fs::write(&path, default_config)?;
    Ok(path)
}

/// Opens config file in $EDITOR, creating it first if needed.
pub fn edit_config() -> Result<PathBuf> {
    let path = config_path().context("Could not determine config directory")?;

    // Create config if it doesn't exist
    if !path.exists() {
        init_config()?;
    }

    // Get editor from environment
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    // Open editor
    std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("Failed to open editor: {editor}"))?;

    Ok(path)
}

/// Shows current config.
pub fn show_config(config: &Config) {
    let path = config_path();

    println!("{}", "raptar configuration".bold());
    println!();

    if let Some(ref p) = path {
        if p.exists() {
            println!("Config file: {}", p.display().to_string().green());
        } else {
            println!("Config file: {} {}", p.display(), "(not created)".dimmed());
            println!("  Run {} to create and edit", "raptar --edit-config".cyan());
        }
    } else {
        println!("Config file: {}", "not available".yellow());
    }

    println!();
    println!("{}", "Current settings:".bold());

    if config.ignore.use_files.is_empty() {
        println!(
            "  ignore.use: {} (only .gitignore and .ignore)",
            "[]".dimmed()
        );
    } else {
        println!("  ignore.use: {:?}", config.ignore.use_files);
    }

    if config.ignore.always_exclude.is_empty() {
        println!(
            "  ignore.always_exclude: {} (no extra exclusions)",
            "[]".dimmed()
        );
    } else {
        println!(
            "  ignore.always_exclude: {} patterns",
            config.ignore.always_exclude.len()
        );
        for pattern in &config.ignore.always_exclude {
            println!("    {}", pattern.dimmed());
        }
    }

    if config.ignore.always_include.is_empty() {
        println!(
            "  ignore.always_include: {} (no force-includes)",
            "[]".dimmed()
        );
    } else {
        println!(
            "  ignore.always_include: {} patterns",
            config.ignore.always_include.len()
        );
        for pattern in &config.ignore.always_include {
            println!("    {}", pattern.dimmed());
        }
    }

    if let Some(ref fmt) = config.defaults.format {
        println!("  defaults.format: {fmt}");
    }
    println!("  defaults.reproducible: {}", config.defaults.reproducible);
    println!("  defaults.dereference: {}", config.defaults.dereference);
    println!(
        "  defaults.preserve_owner: {}",
        config.defaults.preserve_owner
    );

    println!();
    println!("{}", "Usage:".bold());
    println!(
        "  Use {} to honor any gitignore-format file:",
        "--with-ignorefile <FILE>".cyan()
    );
    println!("    raptar --with-ignorefile .dockerignore");
    println!();
    println!(
        "  Use {} to always exclude a pattern:",
        "--with-exclude <PATTERN>".cyan()
    );
    println!("    raptar --with-exclude '*.bak' --with-exclude 'node_modules/**'");
    println!();
    println!(
        "  Use {} to force include (overrides exclusions):",
        "--with-include <PATTERN>".cyan()
    );
    println!("    raptar --with-exclude '*.log' --with-include 'important.log'");
    println!();
    println!(
        "  Use {} to disable config always-exclude patterns:",
        "--without-exclude-always".cyan()
    );
}

/// Result of searching for ignore files.
pub struct IgnoreFileSearch {
    pub found: Vec<PathBuf>,
    pub not_found: Vec<String>,
}

/// Finds requested ignore files relative to a root directory.
pub fn find_ignore_files(root: &Path, requested: &[String]) -> IgnoreFileSearch {
    let mut found = Vec::new();
    let mut not_found = Vec::new();

    for name in requested {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }

        let path = Path::new(name);

        // If it's an absolute path, use it directly
        if path.is_absolute() {
            if path.exists() {
                found.push(path.to_path_buf());
            } else {
                not_found.push(name.to_string());
            }
            continue;
        }

        // If it contains a directory separator, resolve from cwd or root
        if name.contains('/') || name.contains('\\') {
            // Try relative to cwd first
            if path.exists() {
                found.push(path.to_path_buf());
                continue;
            }
            // Try relative to root
            let rooted = root.join(path);
            if rooted.exists() {
                found.push(rooted);
            } else {
                not_found.push(name.to_string());
            }
            continue;
        }

        // Just a filename - look in project root
        // Add leading dot if missing (e.g., "dockerignore" -> ".dockerignore")
        let filename = if name.starts_with('.') {
            name.to_string()
        } else {
            format!(".{name}")
        };

        let file_path = root.join(&filename);
        if file_path.exists() {
            found.push(file_path);
        } else {
            not_found.push(name.to_string());
        }
    }

    IgnoreFileSearch { found, not_found }
}
