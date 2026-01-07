//! # raptar 🦖
//!
//! A smart archive tool that respects `.gitignore` and friends.
//!
//! ## Features
//!
//! - Respects `.gitignore` and `.ignore` by default
//! - Opt-in support for `.dockerignore`, `.npmignore`, and other ignore files
//! - Configuration file at `~/.config/raptar/config.toml`
//! - Multiple output formats: tar, tar.gz, tar.bz2, tar.zst, zip
//! - Reproducible builds with deterministic ordering and timestamps
//! - Symlink preservation
//! - Ownership and permission preservation
//! - Preview mode and size estimation
//!
//! ## Rule Precedence
//!
//! All patterns from all sources are combined and sorted by specificity
//! (fewer wildcards first). First match wins.

mod archive;
mod config;
mod ecosystem;
mod rules;
mod walk;

use anyhow::Result;
use bytesize::ByteSize;
use clap::{Parser, ValueEnum};
use colored::Colorize;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub use config::Config;
pub use walk::{EntryType, ExcludedFile, FileEntry};

/// Supported archive formats.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Format {
    /// Plain tar archive
    Tar,
    /// Gzip-compressed tar archive
    #[value(name = "tar.gz", alias = "tgz")]
    TarGz,
    /// Bzip2-compressed tar archive
    #[value(name = "tar.bz2", alias = "tbz2")]
    TarBz2,
    /// Zstandard-compressed tar archive
    #[value(name = "tar.zst", alias = "tzst")]
    TarZst,
    /// Zip archive
    Zip,
}

impl Format {
    /// Returns the file extension for this format.
    const fn extension(self) -> &'static str {
        match self {
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::TarBz2 => "tar.bz2",
            Self::TarZst => "tar.zst",
            Self::Zip => "zip",
        }
    }
}

/// 🦖 raptar - A smart archive tool that respects .gitignore and friends
#[derive(Parser, Debug)]
#[command(name = "raptar")]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Directory to archive (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Output file (defaults to directory name with appropriate extension)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(short, long, value_enum, default_value = "tar.gz")]
    pub format: Format,

    /// Preview mode - show what would be included without creating archive
    #[arg(short, long)]
    pub preview: bool,

    /// Show size estimation
    #[arg(short, long)]
    pub size: bool,

    // ========================================================================
    // Include/Exclude patterns
    // ========================================================================
    /// Add exclude pattern (can be repeated, gitignore syntax)
    #[arg(long = "with-exclude", action = clap::ArgAction::Append, value_name = "PATTERN")]
    pub with_exclude: Vec<String>,

    /// Add include pattern, overrides exclusions (can be repeated)
    #[arg(long = "with-include", action = clap::ArgAction::Append, value_name = "PATTERN")]
    pub with_include: Vec<String>,

    /// Add ignore file to use (can be repeated, gitignore-format files)
    #[arg(long = "with-ignorefile", action = clap::ArgAction::Append, value_name = "FILE")]
    pub with_ignorefile: Vec<String>,

    /// Disable config `always_exclude` patterns
    #[arg(long)]
    pub without_exclude_always: bool,

    /// Disable config `always_include` patterns
    #[arg(long)]
    pub without_include_always: bool,

    /// Disable all ignore files (.gitignore, .ignore, etc.)
    #[arg(long)]
    pub without_ignorefiles: bool,

    /// Disable specific ignore file (can be repeated)
    #[arg(long = "without-ignorefile", action = clap::ArgAction::Append, value_name = "FILE")]
    pub without_ignorefile: Vec<String>,

    // ========================================================================
    // Ecosystem templates
    // ========================================================================
    /// Use ecosystem-specific gitignore template (e.g., Rust, Python, Node)
    #[arg(long = "with-ecosystem", action = clap::ArgAction::Append, value_name = "NAME")]
    pub with_ecosystem: Vec<String>,

    /// List available ecosystem templates
    #[arg(long)]
    pub list_ecosystems: bool,

    // ========================================================================
    // Other options
    // ========================================================================
    /// Follow symlinks instead of archiving them as links
    #[arg(long)]
    pub dereference: bool,

    /// Preserve file ownership (uid/gid)
    #[arg(long)]
    pub preserve_owner: bool,

    /// Deterministic ordering and zero timestamps for reproducible archives
    #[arg(short, long)]
    pub reproducible: bool,

    /// Minimal output
    #[arg(short, long)]
    pub quiet: bool,

    /// Verbose output - show rules and exclusion reasons
    #[arg(short, long)]
    pub verbose: bool,

    /// Show config file location and current settings
    #[arg(long)]
    pub show_config: bool,

    /// Initialize config file with defaults
    #[arg(long)]
    pub init_config: bool,

    /// Open config file in $EDITOR (creates if missing)
    #[arg(long)]
    pub edit_config: bool,
}

/// Displays a preview of files that would be archived.
fn preview_files(entries: &[FileEntry], excluded: &[ExcludedFile], args: &Args) {
    let total_size: u64 = entries.iter().map(|e| e.size).sum();
    let symlink_count = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::Symlink)
        .count();

    println!("{}", "Files to be archived:".bold().green());
    println!();

    for entry in entries {
        let type_indicator = match entry.entry_type {
            EntryType::Symlink => " -> ",
            EntryType::File | EntryType::Directory => "",
        };

        if args.size {
            let size_str = if entry.entry_type == EntryType::Symlink {
                "    link".to_string()
            } else {
                format!("{:>10}", ByteSize(entry.size))
            };
            print!("  {} ", size_str.dimmed());
        } else {
            print!("  ");
        }

        print!("{}", entry.relative_path.display());

        if let Some(ref target) = entry.link_target {
            print!(
                "{}{}",
                type_indicator.cyan(),
                target.display().to_string().cyan()
            );
        }
        println!();
    }

    println!();
    println!(
        "{} {} files ({} symlinks), {} total",
        "Summary:".bold(),
        entries.len(),
        symlink_count,
        ByteSize(total_size)
    );

    // Show excluded files in verbose mode
    if args.verbose && !excluded.is_empty() {
        println!();
        println!("{}", "Files excluded:".bold().yellow());
        for file in excluded {
            println!(
                "  {} {}",
                file.path.display().to_string().dimmed(),
                format!("({})", file.origin).dimmed()
            );
        }
    }
}

/// Determines the output path for the archive.
fn get_output_path(args: &Args) -> Result<PathBuf> {
    if let Some(ref output) = args.output {
        return Ok(output.clone());
    }

    let dir_name = args.path.canonicalize()?.file_name().map_or_else(
        || "archive".to_string(),
        |s| s.to_string_lossy().to_string(),
    );

    Ok(PathBuf::from(format!(
        "{}.{}",
        dir_name,
        args.format.extension()
    )))
}

/// Handle config-related commands (--init-config, --edit-config, --show-config).
/// Returns Ok(true) if a command was handled and we should exit.
fn handle_config_commands(args: &Args, config: &Config) -> Result<bool> {
    if args.init_config {
        let path = config::init_config()?;
        println!(
            "Created config file: {}",
            path.display().to_string().green()
        );
        return Ok(true);
    }

    if args.edit_config {
        let path = config::edit_config()?;
        println!("Opened: {}", path.display().to_string().green());
        return Ok(true);
    }

    if args.show_config {
        config::show_config(config);
        return Ok(true);
    }

    Ok(false)
}

/// Apply config defaults to args (CLI args take precedence).
#[allow(clippy::missing_const_for_fn)]
fn apply_config_defaults(args: &mut Args, config: &Config) {
    if config.defaults.reproducible && !args.reproducible {
        args.reproducible = true;
    }
    if config.defaults.dereference && !args.dereference {
        args.dereference = true;
    }
    if config.defaults.preserve_owner && !args.preserve_owner {
        args.preserve_owner = true;
    }
}

/// Exclude the output file from entries to prevent infinite archive growth.
fn exclude_output_file(entries: &mut Vec<FileEntry>, output: &Path, quiet: bool) {
    let abs_output = output.canonicalize().or_else(|_| {
        // File doesn't exist yet, canonicalize parent and append filename
        output
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .map(|p| p.join(output.file_name().unwrap_or_default()))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, ""))
    });

    if let Ok(abs_output) = abs_output {
        let before_count = entries.len();
        entries.retain(|e| e.path != abs_output);
        if entries.len() < before_count && !quiet {
            eprintln!(
                "{} Excluding output file from archive: {}",
                "ℹ".blue(),
                output.display()
            );
        }
    }
}

/// Create the archive in the specified format.
fn create_archive(output: &Path, entries: &[FileEntry], args: &Args) -> Result<()> {
    let file = File::create(output)?;
    let writer = BufWriter::new(file);

    match args.format {
        Format::Tar => {
            archive::create_tar(
                writer,
                entries,
                args.reproducible,
                args.preserve_owner,
                args.quiet,
                args.verbose,
            )?;
        }
        Format::TarGz => {
            archive::create_tar_gz(
                writer,
                entries,
                args.reproducible,
                args.preserve_owner,
                args.quiet,
                args.verbose,
            )?;
        }
        Format::TarBz2 => {
            archive::create_tar_bz2(
                writer,
                entries,
                args.reproducible,
                args.preserve_owner,
                args.quiet,
                args.verbose,
            )?;
        }
        Format::TarZst => {
            archive::create_tar_zst(
                writer,
                entries,
                args.reproducible,
                args.preserve_owner,
                args.quiet,
                args.verbose,
            )?;
        }
        Format::Zip => {
            archive::create_zip(writer, entries, args.reproducible, args.quiet, args.verbose)?;
        }
    }
    Ok(())
}

/// Print the final summary after creating an archive.
fn print_summary(output: &Path, entries: &[FileEntry]) -> Result<()> {
    let output_size = std::fs::metadata(output)?.len();
    let input_size: u64 = entries.iter().map(|e| e.size).sum();
    let ratio = if input_size > 0 {
        (output_size as f64 / input_size as f64) * 100.0
    } else {
        100.0
    };

    println!(
        "🦖 Done! {} → {} ({:.1}% of original)",
        ByteSize(input_size),
        ByteSize(output_size),
        ratio
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = Args::parse();
    let config = config::load_config();

    // Handle config commands first
    if handle_config_commands(&args, &config)? {
        return Ok(());
    }

    // Handle --list-ecosystems
    if args.list_ecosystems {
        ecosystem::print_ecosystem_list();
        return Ok(());
    }

    // Warn if no config file exists (only when actually archiving, not in quiet mode)
    if !args.quiet && !args.preview && !config::config_exists() {
        eprintln!(
            "{} Running with defaults. Use {} to customize.",
            "ℹ".blue(),
            "--edit-config".cyan()
        );
    }

    apply_config_defaults(&mut args, &config);

    if !args.path.exists() {
        anyhow::bail!("Path does not exist: {}", args.path.display());
    }

    if !args.quiet {
        println!("🦖 Scanning files...");
    }

    // Determine output path early so we can exclude it from the archive
    let output = if args.preview {
        None
    } else {
        Some(get_output_path(&args)?)
    };

    let (mut entries, excluded) = walk::collect_files(&args, &config)?;

    // Exclude the output file itself to prevent infinite growth
    if let Some(ref out) = output {
        exclude_output_file(&mut entries, out, args.quiet);
    }

    if entries.is_empty() {
        println!("{}", "No files to archive!".yellow());
        return Ok(());
    }

    // Preview or size estimation mode
    if args.preview || args.size {
        preview_files(&entries, &excluded, &args);
        if args.preview {
            return Ok(());
        }
    }

    // Create archive
    let output = output.expect("output path should be set for non-preview mode");

    if !args.quiet {
        println!(
            "🦖 Creating {} with {} files...",
            output.display().to_string().cyan(),
            entries.len()
        );
    }

    create_archive(&output, &entries, &args)?;

    if !args.quiet {
        print_summary(&output, &entries)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn default_args(path: PathBuf) -> Args {
        Args {
            path,
            output: None,
            format: Format::TarGz,
            preview: false,
            size: false,
            with_exclude: vec![],
            with_include: vec![],
            with_ignorefile: vec![],
            without_exclude_always: false,
            without_include_always: false,
            without_ignorefiles: false,
            without_ignorefile: vec![],
            with_ecosystem: vec![],
            list_ecosystems: false,
            reproducible: false,
            dereference: false,
            preserve_owner: false,
            quiet: true,
            verbose: false,
            show_config: false,
            init_config: false,
            edit_config: false,
        }
    }

    fn default_config() -> Config {
        Config::default()
    }

    #[test]
    fn test_format_extension_tar() {
        assert_eq!(Format::Tar.extension(), "tar");
    }

    #[test]
    fn test_format_extension_tar_gz() {
        assert_eq!(Format::TarGz.extension(), "tar.gz");
    }

    #[test]
    fn test_format_extension_tar_bz2() {
        assert_eq!(Format::TarBz2.extension(), "tar.bz2");
    }

    #[test]
    fn test_format_extension_tar_zst() {
        assert_eq!(Format::TarZst.extension(), "tar.zst");
    }

    #[test]
    fn test_format_extension_zip() {
        assert_eq!(Format::Zip.extension(), "zip");
    }

    #[test]
    fn test_collect_files_basic() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "content").unwrap();

        let args = default_args(tmp.path().to_path_buf());
        let (entries, _) = walk::collect_files(&args, &default_config()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].relative_path.to_string_lossy(), "file.txt");
    }

    #[test]
    fn test_collect_files_respects_gitignore() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(tmp.path().join("ignored.txt"), "should be ignored").unwrap();
        fs::write(tmp.path().join("included.txt"), "should be included").unwrap();

        let args = default_args(tmp.path().to_path_buf());

        let (entries, _) = walk::collect_files(&args, &default_config()).unwrap();
        let paths: Vec<_> = entries
            .iter()
            .map(|e| e.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(paths.contains(&"included.txt".to_string()));
        assert!(paths.contains(&".gitignore".to_string()));
        assert!(!paths.contains(&"ignored.txt".to_string()));
    }

    #[test]
    fn test_without_ignorefiles_includes_all() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(tmp.path().join("ignored.txt"), "should be included now").unwrap();
        fs::write(tmp.path().join("other.txt"), "other").unwrap();

        let mut args = default_args(tmp.path().to_path_buf());
        args.without_ignorefiles = true;

        let (entries, _) = walk::collect_files(&args, &default_config()).unwrap();
        let paths: Vec<_> = entries
            .iter()
            .map(|e| e.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(paths.contains(&"ignored.txt".to_string()));
        assert!(paths.contains(&"other.txt".to_string()));
    }

    #[test]
    fn test_without_ignorefile_specific() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".gitignore"), "*.log\n").unwrap();
        fs::write(tmp.path().join(".ignore"), "*.tmp\n").unwrap();
        fs::write(tmp.path().join("test.log"), "log").unwrap();
        fs::write(tmp.path().join("test.tmp"), "tmp").unwrap();
        fs::write(tmp.path().join("test.txt"), "txt").unwrap();

        let mut args = default_args(tmp.path().to_path_buf());
        args.without_ignorefile = vec!["gitignore".to_string()]; // Disable only .gitignore

        let (entries, _) = walk::collect_files(&args, &default_config()).unwrap();
        let paths: Vec<_> = entries
            .iter()
            .map(|e| e.relative_path.to_string_lossy().to_string())
            .collect();

        // .gitignore disabled, so .log files included
        assert!(paths.contains(&"test.log".to_string()));
        // .ignore still active, so .tmp files excluded
        assert!(!paths.contains(&"test.tmp".to_string()));
        assert!(paths.contains(&"test.txt".to_string()));
    }

    #[test]
    fn test_with_exclude_pattern() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("keep.txt"), "keep").unwrap();
        fs::write(tmp.path().join("remove.bak"), "remove").unwrap();

        let mut args = default_args(tmp.path().to_path_buf());
        args.with_exclude = vec!["*.bak".to_string()];

        let (entries, excluded) = walk::collect_files(&args, &default_config()).unwrap();
        let paths: Vec<_> = entries
            .iter()
            .map(|e| e.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(paths.contains(&"keep.txt".to_string()));
        assert!(!paths.contains(&"remove.bak".to_string()));
        assert!(excluded
            .iter()
            .any(|e| e.path.to_string_lossy() == "remove.bak"));
    }

    #[test]
    fn test_with_include_overrides_exclude() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("keep.log"), "keep").unwrap();
        fs::write(tmp.path().join("remove.log"), "remove").unwrap();

        let mut args = default_args(tmp.path().to_path_buf());
        args.with_exclude = vec!["*.log".to_string()];
        args.with_include = vec!["keep.log".to_string()];

        let (entries, excluded) = walk::collect_files(&args, &default_config()).unwrap();
        let paths: Vec<_> = entries
            .iter()
            .map(|e| e.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(paths.contains(&"keep.log".to_string()));
        assert!(!paths.contains(&"remove.log".to_string()));
        assert!(excluded
            .iter()
            .any(|e| e.path.to_string_lossy() == "remove.log"));
    }

    #[test]
    fn test_excluded_files_have_origin() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.bak"), "content").unwrap();

        let mut args = default_args(tmp.path().to_path_buf());
        args.with_exclude = vec!["*.bak".to_string()];

        let (_, excluded) = walk::collect_files(&args, &default_config()).unwrap();

        assert_eq!(excluded.len(), 1);
        assert!(excluded[0].origin.contains("--with-exclude"));
    }

    #[test]
    fn test_gitignore_line_numbers_in_origin() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".gitignore"), "# comment\n*.log\n").unwrap();
        fs::write(tmp.path().join("test.log"), "log").unwrap();

        let args = default_args(tmp.path().to_path_buf());

        let (_, excluded) = walk::collect_files(&args, &default_config()).unwrap();

        let log_excluded = excluded
            .iter()
            .find(|e| e.path.to_string_lossy() == "test.log");
        assert!(log_excluded.is_some());
        // Should show line 2 (the *.log line)
        assert!(log_excluded.unwrap().origin.contains(":2"));
    }
}
