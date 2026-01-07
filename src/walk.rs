//! Directory walking with rule-based filtering using indexed lookups.

use crate::config::{find_ignore_files, Config};
use crate::rules::{parse_ignore_file, print_rules_verbose, Action, RuleIndex, RuleOrigin};
use crate::Args;

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs::{self, Metadata};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// A file entry to be archived.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub size: u64,
    pub entry_type: EntryType,
    pub link_target: Option<PathBuf>,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub mtime: u64,
}

/// Type of file entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
}

/// A file that was excluded from the archive.
#[derive(Debug, Clone)]
pub struct ExcludedFile {
    pub path: PathBuf,
    pub origin: String,
}

/// Results of walking a directory tree.
pub struct WalkResults {
    pub entries: Vec<FileEntry>,
    pub excluded: Vec<ExcludedFile>,
}

/// Builds the rule index from config and CLI args.
#[allow(clippy::too_many_lines)]
pub fn build_rule_index(args: &Args, config: &Config, root: &Path) -> RuleIndex {
    let mut index = RuleIndex::new(root.to_path_buf());

    // 0. Ecosystem templates (lowest priority, added first)
    // Only used when explicitly requested via --with-ecosystem
    if !args.with_ecosystem.is_empty() {
        let templates =
            crate::ecosystem::load_ecosystem_templates(&args.with_ecosystem, args.verbose, true);
        for path in templates {
            if let Err(e) = parse_ignore_file(&path, &mut index, Action::Exclude) {
                eprintln!("{} {}", "⚠".yellow(), e);
            }
        }
    }

    // 1. Parse ignore files (added after ecosystems, so they take priority)
    if !args.without_ignorefiles {
        // Find .gitignore and .ignore in root
        let default_ignores = [".gitignore", ".ignore"];
        for name in &default_ignores {
            // Skip if explicitly excluded via --without-ignorefile
            if args
                .without_ignorefile
                .iter()
                .any(|f| f == *name || f == &name[1..])
            {
                continue;
            }
            let path = root.join(name);
            if path.exists() {
                if let Err(e) = parse_ignore_file(&path, &mut index, Action::Exclude) {
                    eprintln!("{} {}", "⚠".yellow(), e);
                } else {
                    index.loaded_ignore_files.insert(path);
                }
            }
        }

        // Add configured ignore files
        let search = find_ignore_files(root, &config.ignore.use_files);
        for name in &search.not_found {
            eprintln!(
                "{} Configured ignore file not found: {}",
                "⚠".yellow(),
                name.yellow()
            );
        }
        for path in &search.found {
            // Skip if explicitly excluded
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if args
                .without_ignorefile
                .iter()
                .any(|f| f == name || format!(".{f}") == name)
            {
                continue;
            }
            if let Err(e) = parse_ignore_file(path, &mut index, Action::Exclude) {
                eprintln!("{} {}", "⚠".yellow(), e);
            } else {
                index.loaded_ignore_files.insert(path.clone());
            }
        }

        // Add CLI-specified ignore files (--with-ignorefile)
        if !args.with_ignorefile.is_empty() {
            let cli_search = find_ignore_files(root, &args.with_ignorefile);
            for name in &cli_search.not_found {
                eprintln!("{} Ignore file not found: {}", "⚠".yellow(), name.yellow());
            }
            for path in &cli_search.found {
                if let Err(e) = parse_ignore_file(path, &mut index, Action::Exclude) {
                    eprintln!("{} {}", "⚠".yellow(), e);
                } else {
                    index.loaded_ignore_files.insert(path.clone());
                }
            }
        }
    }

    // 2. Config always_exclude (force exclude)
    if !args.without_exclude_always {
        for pattern in &config.ignore.always_exclude {
            let origin = RuleOrigin {
                source: "config always_exclude".to_string(),
                line: None,
            };
            if let Err(e) = index.add_rule(pattern, Action::Exclude, origin, root) {
                eprintln!("{} {}", "⚠".yellow(), e);
            }
        }
    }

    // 3. Config always_include (force include)
    if !args.without_include_always {
        for pattern in &config.ignore.always_include {
            let origin = RuleOrigin {
                source: "config always_include".to_string(),
                line: None,
            };
            if let Err(e) = index.add_rule(pattern, Action::Include, origin, root) {
                eprintln!("{} {}", "⚠".yellow(), e);
            }
        }
    }

    // 4. CLI --with-exclude
    for pattern in &args.with_exclude {
        let origin = RuleOrigin {
            source: "--with-exclude".to_string(),
            line: None,
        };
        if let Err(e) = index.add_rule(pattern, Action::Exclude, origin, root) {
            eprintln!("{} {}", "⚠".yellow(), e);
        }
    }

    // 5. CLI --with-include (highest priority)
    for pattern in &args.with_include {
        let origin = RuleOrigin {
            source: "--with-include".to_string(),
            line: None,
        };
        if let Err(e) = index.add_rule(pattern, Action::Include, origin, root) {
            eprintln!("{} {}", "⚠".yellow(), e);
        }
    }

    // Build the index
    index.build();

    index
}

/// Creates a file entry from path and metadata.
pub fn create_file_entry(
    path: &Path,
    relative_path: PathBuf,
    metadata: &Metadata,
    reproducible: bool,
) -> Result<FileEntry> {
    let entry_type = if metadata.file_type().is_symlink() {
        EntryType::Symlink
    } else if metadata.is_dir() {
        EntryType::Directory
    } else {
        EntryType::File
    };

    let link_target = if entry_type == EntryType::Symlink {
        Some(fs::read_link(path).context("Failed to read symlink target")?)
    } else {
        None
    };

    let size = if entry_type == EntryType::Symlink {
        0 // Symlinks don't have content size
    } else {
        metadata.len()
    };

    let mtime = if reproducible {
        0
    } else {
        metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    };

    Ok(FileEntry {
        path: path.to_path_buf(),
        relative_path,
        size,
        entry_type,
        link_target,
        mode: metadata.permissions().mode(),
        uid: metadata.uid(),
        gid: metadata.gid(),
        mtime,
    })
}

/// Recursively walks a directory, using indexed rule lookups.
fn walk_directory(
    dir: &Path,
    root: &Path,
    index: &mut RuleIndex,
    dereference: bool,
    reproducible: bool,
    results: &mut WalkResults,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
        .filter_map(Result::ok)
        .collect();

    // Sort for deterministic ordering
    entries.sort_by_key(std::fs::DirEntry::path);

    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .context("Failed to compute relative path")?;

        // Warn about nested ignore files (not yet supported)
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if (filename == ".gitignore" || filename == ".ignore") && path.parent() != Some(root) {
                // Check if this file was explicitly loaded via CLI
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !index.loaded_ignore_files.contains(&canonical) {
                    eprintln!(
                        "{} Nested ignore file not processed: {}",
                        "⚠".yellow(),
                        relative.display()
                    );
                }
            }
        }

        let metadata = if dereference {
            path.metadata()
        } else {
            path.symlink_metadata()
        }
        .with_context(|| format!("Failed to read metadata: {}", path.display()))?;

        let is_dir = metadata.is_dir() && !metadata.file_type().is_symlink();

        // Check rules using the index (uses absolute path)
        if let Some((action, origin)) = index.find_match(&path) {
            match action {
                Action::Exclude => {
                    results.excluded.push(ExcludedFile {
                        path: relative.to_path_buf(),
                        origin,
                    });

                    // But check if we need to recurse anyway for nested includes
                    if is_dir && index.has_include_rules(&path) {
                        walk_directory(&path, root, index, dereference, reproducible, results)?;
                    }
                    continue;
                }
                Action::Include => {
                    // Explicitly included
                    if is_dir {
                        walk_directory(&path, root, index, dereference, reproducible, results)?;
                    } else {
                        let file_entry = create_file_entry(
                            &path,
                            relative.to_path_buf(),
                            &metadata,
                            reproducible,
                        )?;
                        results.entries.push(file_entry);
                    }
                    continue;
                }
            }
        }

        // No rule matched - include by default
        if is_dir {
            walk_directory(&path, root, index, dereference, reproducible, results)?;
        } else {
            let file_entry =
                create_file_entry(&path, relative.to_path_buf(), &metadata, reproducible)?;
            results.entries.push(file_entry);
        }
    }

    Ok(())
}

/// Collects all files to be archived based on ignore rules.
pub fn collect_files(args: &Args, config: &Config) -> Result<(Vec<FileEntry>, Vec<ExcludedFile>)> {
    let root = args.path.canonicalize().context("Failed to resolve path")?;

    // Build the rule index
    let mut index = build_rule_index(args, config, &root);

    // Verbose output
    print_rules_verbose(&index, args.verbose);

    // Walk the tree using indexed lookups
    let mut results = WalkResults {
        entries: Vec::new(),
        excluded: Vec::new(),
    };

    walk_directory(
        &root,
        &root,
        &mut index,
        args.dereference,
        args.reproducible,
        &mut results,
    )?;

    // Sort for reproducibility
    if args.reproducible {
        results
            .entries
            .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        results.excluded.sort_by(|a, b| a.path.cmp(&b.path));
    }

    Ok((results.entries, results.excluded))
}
