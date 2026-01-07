//! Rule system for pattern matching with path-based indexing.
//!
//! Patterns are absolutized at parse time and indexed by activation path.
//! At walk time, we look up applicable rules by directory path and scan
//! a pre-sorted list for the first match.

use anyhow::{Context, Result};
use colored::Colorize;
use globset::{Glob, GlobMatcher};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Action to take when a rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Include,
    Exclude,
}

/// Origin of a rule for attribution.
#[derive(Debug, Clone)]
pub struct RuleOrigin {
    pub source: String,
    pub line: Option<usize>,
}

impl std::fmt::Display for RuleOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line) = self.line {
            write!(f, "{}:{}", self.source, line)
        } else {
            write!(f, "{}", self.source)
        }
    }
}

/// Priority bucket for rule ordering.
/// Lower bucket = higher priority (checked first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bucket {
    /// Explicit path, no wildcards: /a/b/file.txt
    ExplicitPath = 0,
    /// Fixed path with wildcard filename: /a/b/*.txt
    WildcardFilename = 1,
    /// Double-star at non-root depth: a/b/**/file.txt
    DeepDoubleStar = 2,
    /// Universal (** at root): **/*.log, *.log
    Universal = 3,
}

/// Analyzed pattern information.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PatternInfo {
    /// Original pattern (before absolutizing)
    pub original: String,
    /// Absolute pattern (relative to archive root)
    pub absolute: PathBuf,
    /// Priority bucket
    pub bucket: Bucket,
    /// Path depth (deeper = more specific within bucket)
    pub path_depth: usize,
    /// Where ** appears (for `DeepDoubleStar` bucket)
    pub double_star_depth: usize,
    /// Wildcard count for tiebreaking
    pub wildcard_count: usize,
    /// Whether pattern contains ** (needs re-anchoring on descent)
    pub has_double_star: bool,
    /// Fixed prefix before any wildcards (activation path)
    pub activation_path: PathBuf,
    /// Is this a directory-only pattern (ends with / or /**)
    pub is_dir_pattern: bool,
}

/// Sort key for rules within a path's rule list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleSortKey {
    pub bucket: Bucket,
    /// Negative depth so deeper sorts first
    pub neg_path_depth: i32,
    /// Negative double-star depth so deeper sorts first  
    pub neg_double_star_depth: i32,
    /// Fewer wildcards = more specific
    pub wildcard_count: usize,
}

/// An indexed rule ready for matching.
#[derive(Debug, Clone)]
pub struct IndexedRule {
    /// The absolute glob pattern for this path level
    pub pattern: PathBuf,
    /// Compiled matcher
    pub matcher: GlobMatcher,
    /// Action to take on match
    pub action: Action,
    /// Where this rule came from
    pub origin: RuleOrigin,
    /// Sort key for ordering
    pub sort_key: RuleSortKey,
    /// Original pattern info (for re-anchoring)
    pub info: PatternInfo,
    /// Insertion sequence number (for last-rule-wins ordering)
    pub seq: usize,
}

impl IndexedRule {
    /// Check if this rule matches the given path.
    pub fn matches(&self, path: &Path) -> bool {
        self.matcher.is_match(path)
    }

    /// Re-anchor this rule for a child directory.
    /// Returns None if the rule can't apply in the child.
    pub fn reanchor_for(&self, child_dir: &Path) -> Option<Self> {
        // Check relationship between child_dir and activation_path
        let child_under_activation = child_dir.starts_with(&self.info.activation_path);
        let activation_under_child = self.info.activation_path.starts_with(child_dir);

        // Rule doesn't apply if child and activation are unrelated (siblings)
        if !child_under_activation && !activation_under_child {
            return None;
        }

        // Directory patterns (build/ -> **/build/**) don't need reanchoring
        // The trailing /** already matches contents at any depth
        if self.info.is_dir_pattern {
            return Some(self.clone());
        }

        if !self.info.has_double_star {
            // No **, rule stays as-is if child is under activation path
            if child_under_activation {
                return Some(self.clone());
            }
            return None;
        }

        // Has **: check if ** is "universal" (at/near start) vs "terminal" (at end of specific path)
        // Universal patterns (like **/*.log) have activation_path at the archive root
        // Terminal patterns (like .git/**) have activation_path at the specific dir
        if activation_under_child {
            // Child is an ancestor of activation - rule applies when we descend further
            return Some(self.clone());
        }

        // child_under_activation: re-anchor the pattern
        let new_pattern = reanchor_pattern(&self.pattern, child_dir);
        let glob = Glob::new(&new_pattern.to_string_lossy()).ok()?;

        Some(Self {
            pattern: new_pattern,
            matcher: glob.compile_matcher(),
            action: self.action,
            origin: self.origin.clone(),
            sort_key: self.sort_key.clone(),
            info: self.info.clone(),
            seq: self.seq,
        })
    }
}

/// Entry in the rule index with metadata.
#[derive(Debug, Default, Clone)]
struct IndexEntry {
    /// All rules, sorted by priority
    rules: Vec<IndexedRule>,
    /// Quick flag: does this entry have any Include rules?
    has_includes: bool,
}

/// The rule index - maps directory paths to applicable rules.
#[derive(Debug, Default)]
pub struct RuleIndex {
    /// `archive_root` for reference
    pub archive_root: PathBuf,
    /// Rules indexed by directory path
    index: HashMap<PathBuf, IndexEntry>,
    /// Original parsed rules (for verbose output)
    rules: Vec<IndexedRule>,
    /// Next sequence number for insertion order
    next_seq: usize,
    /// Paths of ignore files that were loaded (to avoid warning about them)
    pub loaded_ignore_files: HashSet<PathBuf>,
}

impl RuleIndex {
    /// Create a new index for the given archive root.
    pub fn new(archive_root: PathBuf) -> Self {
        Self {
            archive_root,
            index: HashMap::new(),
            rules: Vec::new(),
            next_seq: 0,
            loaded_ignore_files: HashSet::new(),
        }
    }

    /// Add a rule from an ignore file.
    pub fn add_rule(
        &mut self,
        pattern: &str,
        action: Action,
        origin: RuleOrigin,
        ignore_file_dir: &Path,
    ) -> Result<()> {
        let pattern = pattern.trim();
        if pattern.is_empty() || pattern.starts_with('#') {
            return Ok(());
        }

        // Handle negation
        #[allow(clippy::option_if_let_else)]
        let (actual_action, clean_pattern) = if let Some(negated) = pattern.strip_prefix('!') {
            let flipped = match action {
                Action::Exclude => Action::Include,
                Action::Include => Action::Exclude,
            };
            (flipped, negated)
        } else {
            (action, pattern)
        };

        // Analyze and absolutize the pattern
        let info = analyze_pattern(clean_pattern, ignore_file_dir, &self.archive_root);

        // Skip patterns that can never match in archive
        if !could_match_in_archive(&info.activation_path, &self.archive_root) {
            return Ok(());
        }

        // Build the indexed rule
        let glob = Glob::new(&info.absolute.to_string_lossy())
            .with_context(|| format!("Invalid pattern: {clean_pattern}"))?;

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let sort_key = RuleSortKey {
            bucket: info.bucket,
            neg_path_depth: -(info.path_depth as i32),
            neg_double_star_depth: -(info.double_star_depth as i32),
            wildcard_count: info.wildcard_count,
        };

        let seq = self.next_seq;
        self.next_seq += 1;

        let rule = IndexedRule {
            pattern: info.absolute.clone(),
            matcher: glob.compile_matcher(),
            action: actual_action,
            origin,
            sort_key,
            info,
            seq,
        };

        self.rules.push(rule);
        Ok(())
    }

    /// Build the index after all rules are added.
    /// Populates index entries from `archive_root` down through all activation paths.
    pub fn build(&mut self) {
        // Clear any existing index
        self.index.clear();

        // Phase 1: Add each rule to its activation path and all ancestors
        for rule in &self.rules {
            let mut path = rule.info.activation_path.clone();

            loop {
                self.index
                    .entry(path.clone())
                    .or_default()
                    .rules
                    .push(rule.clone());

                if path == self.archive_root || !path.starts_with(&self.archive_root) {
                    break;
                }

                match path.parent() {
                    Some(parent) => path = parent.to_path_buf(),
                    None => break,
                }
            }
        }

        // Phase 2: For each path, also include rules from ancestors
        // Collect paths to process (can't mutate while iterating)
        let paths: Vec<PathBuf> = self.index.keys().cloned().collect();

        for path in paths {
            // Collect ancestor rules that should also apply here
            let mut ancestor_rules = Vec::new();
            let mut ancestor = path.parent();

            while let Some(anc) = ancestor {
                if let Some(entry) = self.index.get(anc) {
                    for rule in &entry.rules {
                        // Only include if rule could match in this subtree
                        if rule.info.has_double_star || path.starts_with(&rule.info.activation_path)
                        {
                            ancestor_rules.push(rule.clone());
                        }
                    }
                }

                if anc == self.archive_root || !anc.starts_with(&self.archive_root) {
                    break;
                }
                ancestor = anc.parent();
            }

            // Add ancestor rules to this path's entry (avoiding duplicates)
            if let Some(entry) = self.index.get_mut(&path) {
                for rule in ancestor_rules {
                    // Check if we already have this exact pattern
                    if !entry.rules.iter().any(|r| r.pattern == rule.pattern) {
                        entry.rules.push(rule);
                    }
                }
            }
        }

        // Sort by sequence number and set has_includes flag
        for entry in self.index.values_mut() {
            entry.rules.sort_by_key(|r| r.seq);
            entry.has_includes = entry.rules.iter().any(|r| r.action == Action::Include);
        }
    }

    /// Get rules for a directory, building child index if needed.
    pub fn get_rules_for(&mut self, dir: &Path) -> &[IndexedRule] {
        // If we already have this path, return it
        if self.index.contains_key(dir) {
            return self.index.get(dir).map_or(&[], |e| e.rules.as_slice());
        }

        // Find nearest ancestor with rules
        let mut ancestor = dir.parent();
        let mut ancestor_entry = None;

        while let Some(anc) = ancestor {
            if let Some(entry) = self.index.get(anc) {
                ancestor_entry = Some((anc.to_path_buf(), entry.clone()));
                break;
            }
            if anc == self.archive_root || !anc.starts_with(&self.archive_root) {
                break;
            }
            ancestor = anc.parent();
        }

        // Re-anchor ancestor rules for this directory
        let rules: Vec<IndexedRule> = ancestor_entry
            .map(|(_, entry)| {
                entry
                    .rules
                    .into_iter()
                    .filter_map(|r| r.reanchor_for(dir))
                    .collect()
            })
            .unwrap_or_default();

        // Sort by sequence number to maintain insertion order
        let mut rules = rules;
        rules.sort_by_key(|r| r.seq);
        let has_includes = rules.iter().any(|r| r.action == Action::Include);

        self.index.insert(
            dir.to_path_buf(),
            IndexEntry {
                rules,
                has_includes,
            },
        );

        self.index.get(dir).map_or(&[], |e| e.rules.as_slice())
    }

    /// Check if a directory has any Include rules (meaning we must recurse).
    /// O(1) lookup using precomputed flag.
    pub fn has_include_rules(&mut self, dir: &Path) -> bool {
        // Ensure the entry exists
        let _ = self.get_rules_for(dir);
        self.index.get(dir).is_some_and(|e| e.has_includes)
    }

    /// Find the last matching rule for a file path (gitignore semantics: last rule wins).
    pub fn find_match(&mut self, file_path: &Path) -> Option<(Action, String)> {
        let dir = file_path.parent()?;
        let rules = self.get_rules_for(dir);

        // Last matching rule wins (gitignore semantics)
        rules
            .iter()
            .rev()
            .find(|r| r.matches(file_path))
            .map(|r| (r.action, r.origin.to_string()))
    }

    /// Get all rules for verbose output.
    pub fn all_rules(&self) -> &[IndexedRule] {
        &self.rules
    }
}

/// Analyze a pattern and return its metadata.
fn analyze_pattern(pattern: &str, ignore_file_dir: &Path, _archive_root: &Path) -> PatternInfo {
    let original = pattern.to_string();
    let pattern = pattern.trim();

    // Handle directory patterns
    let (pattern, is_dir_pattern) = if pattern.ends_with('/') {
        (pattern.strip_suffix('/').unwrap(), true)
    } else {
        (pattern, pattern.ends_with("/**"))
    };

    // Check if rooted (has / or starts with /)
    let has_internal_slash = pattern.contains('/') && !pattern.starts_with('/');
    let starts_with_slash = pattern.starts_with('/');
    let is_rooted = has_internal_slash || starts_with_slash;

    // Strip leading slash
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);

    // Check for **
    let has_double_star = pattern.contains("**");
    let double_star_depth = if has_double_star {
        pattern
            .find("**")
            .map_or(0, |pos| pattern[..pos].matches('/').count())
    } else {
        0
    };

    // Count wildcards
    let wildcard_count = count_wildcards(pattern);

    // Determine bucket
    let bucket = if wildcard_count == 0 && !is_dir_pattern {
        Bucket::ExplicitPath
    } else if !has_double_star && is_rooted && !is_dir_pattern {
        // Has wildcards but no **, and is rooted = wildcard in filename only
        Bucket::WildcardFilename
    } else if has_double_star && is_rooted && double_star_depth > 0 {
        Bucket::DeepDoubleStar
    } else {
        Bucket::Universal
    };

    // Build absolute path
    // For directory patterns (trailing /), append /** to match contents only
    let absolute = if is_rooted {
        let base = ignore_file_dir.join(pattern);
        if is_dir_pattern {
            base.join("**")
        } else {
            base
        }
    } else {
        // Universal pattern: prepend **/ and base at ignore file dir
        let base = ignore_file_dir.join(format!("**/{pattern}"));
        if is_dir_pattern {
            base.join("**") // **/build -> **/build/**
        } else {
            base
        }
    };

    // Calculate path depth
    let path_depth = absolute.components().count();

    // Find activation path (fixed prefix before wildcards)
    let activation_path = find_activation_path(&absolute, ignore_file_dir);

    PatternInfo {
        original,
        absolute,
        bucket,
        path_depth,
        double_star_depth,
        wildcard_count,
        has_double_star: has_double_star || !is_rooted || is_dir_pattern, // universal and directory patterns get **
        activation_path,
        is_dir_pattern,
    }
}

/// Find the fixed prefix of a pattern (activation path).
fn find_activation_path(pattern: &Path, base: &Path) -> PathBuf {
    let pattern_str = pattern.to_string_lossy();

    // Find first wildcard
    let first_wild = pattern_str
        .find(['*', '?', '['])
        .unwrap_or(pattern_str.len());

    // Take everything before that, up to last /
    let prefix = &pattern_str[..first_wild];
    let last_slash = prefix.rfind('/').unwrap_or(0);

    if last_slash == 0 {
        base.to_path_buf()
    } else {
        PathBuf::from(&pattern_str[..last_slash])
    }
}

/// Check if a rule could match anything in the archive.
fn could_match_in_archive(activation_path: &Path, archive_root: &Path) -> bool {
    // Rule activates at/above archive root (covers it)
    archive_root.starts_with(activation_path) ||
    // Rule activates within archive
    activation_path.starts_with(archive_root)
}

/// Re-anchor a ** pattern for a child directory.
#[allow(clippy::option_if_let_else)]
fn reanchor_pattern(pattern: &Path, child_dir: &Path) -> PathBuf {
    let pattern_str = pattern.to_string_lossy();

    // Find ** in the pattern
    if let Some(pos) = pattern_str.find("**") {
        // Replace everything before ** with child_dir
        let suffix = &pattern_str[pos..];
        child_dir.join(suffix)
    } else {
        pattern.to_path_buf()
    }
}

/// Count wildcard characters in a pattern.
pub fn count_wildcards(pattern: &str) -> usize {
    let mut count = 0;
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                count += 1;
                if chars.peek() == Some(&'*') {
                    chars.next();
                    count += 1;
                }
            }
            '?' | '[' => count += 1,
            _ => {}
        }
    }
    count
}

/// Parse an ignore file, absolutizing patterns relative to file location.
pub fn parse_ignore_file(path: &Path, index: &mut RuleIndex, action: Action) -> Result<()> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open ignore file: {}", path.display()))?;
    let reader = BufReader::new(file);
    let source = path.display().to_string();
    let ignore_file_dir = path.parent().unwrap_or_else(|| Path::new("."));

    // Track that we've loaded this ignore file
    if let Ok(canonical) = path.canonicalize() {
        index.loaded_ignore_files.insert(canonical);
    }

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let origin = RuleOrigin {
            source: source.clone(),
            line: Some(line_num + 1),
        };

        if let Err(e) = index.add_rule(trimmed, action, origin, ignore_file_dir) {
            eprintln!("{} {}", "⚠".yellow(), e);
        }
    }

    Ok(())
}

/// Print rules in verbose mode.
pub fn print_rules_verbose(index: &RuleIndex, verbose: bool) {
    if !verbose {
        return;
    }

    // Group by source
    let mut by_source: HashMap<String, Vec<&IndexedRule>> = HashMap::new();
    for rule in index.all_rules() {
        by_source
            .entry(rule.origin.source.clone())
            .or_default()
            .push(rule);
    }

    for (source, rules) in &by_source {
        println!("Excluding ({}):", source.cyan());
        for rule in rules {
            let indicator = if rule.action == Action::Include {
                "+".green()
            } else {
                "-".red()
            };
            println!("  {} {}", indicator, rule.info.original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn make_origin() -> RuleOrigin {
        RuleOrigin {
            source: "test".to_string(),
            line: None,
        }
    }

    // ========================================================================
    // Pattern Analysis Tests
    // ========================================================================

    #[test]
    fn test_analyze_explicit_path() {
        let root = Path::new("/project");
        let info = analyze_pattern("build/output.txt", root, root);

        assert_eq!(info.bucket, Bucket::ExplicitPath);
        assert_eq!(info.wildcard_count, 0);
        assert!(!info.has_double_star);
    }

    #[test]
    fn test_analyze_wildcard_filename() {
        let root = Path::new("/project");
        let info = analyze_pattern("build/*.txt", root, root);

        assert_eq!(info.bucket, Bucket::WildcardFilename);
        assert_eq!(info.wildcard_count, 1);
        assert!(!info.has_double_star);
    }

    #[test]
    fn test_analyze_deep_double_star() {
        let root = Path::new("/project");
        let info = analyze_pattern("src/**/test.txt", root, root);

        assert_eq!(info.bucket, Bucket::DeepDoubleStar);
        assert!(info.has_double_star);
        assert_eq!(info.double_star_depth, 1); // ** is after "src/"
    }

    #[test]
    fn test_analyze_universal_no_slash() {
        let root = Path::new("/project");
        let info = analyze_pattern("*.log", root, root);

        assert_eq!(info.bucket, Bucket::Universal);
        assert!(info.has_double_star); // Gets **/ prepended
    }

    #[test]
    fn test_analyze_universal_double_star_at_root() {
        let root = Path::new("/project");
        let info = analyze_pattern("**/*.log", root, root);

        assert_eq!(info.bucket, Bucket::Universal);
        assert!(info.has_double_star);
        assert_eq!(info.double_star_depth, 0);
    }

    #[test]
    fn test_analyze_directory_pattern() {
        let root = Path::new("/project");
        let info = analyze_pattern("build/", root, root);

        // Directory pattern should append /** to match contents
        assert!(info.is_dir_pattern);
        assert!(info.has_double_star);
        assert_eq!(info.absolute, PathBuf::from("/project/**/build/**"));
    }

    #[test]
    fn test_analyze_rooted_directory_pattern() {
        let root = Path::new("/project");
        let info = analyze_pattern("/build/", root, root);

        // Rooted directory pattern
        assert!(info.is_dir_pattern);
        assert!(info.has_double_star);
        assert_eq!(info.absolute, PathBuf::from("/project/build/**"));
    }

    #[test]
    fn test_analyze_rooted_with_leading_slash() {
        let root = Path::new("/project");
        let info = analyze_pattern("/build/output.txt", root, root);

        assert_eq!(info.bucket, Bucket::ExplicitPath);
        assert_eq!(info.absolute, PathBuf::from("/project/build/output.txt"));
    }

    // ========================================================================
    // Absolutization Tests
    // ========================================================================

    #[test]
    fn test_absolute_universal_pattern() {
        let root = Path::new("/project");
        let info = analyze_pattern("*.log", root, root);

        assert_eq!(info.absolute, PathBuf::from("/project/**/*.log"));
    }

    #[test]
    fn test_absolute_rooted_pattern() {
        let root = Path::new("/project");
        let info = analyze_pattern("build/output.txt", root, root);

        assert_eq!(info.absolute, PathBuf::from("/project/build/output.txt"));
    }

    #[test]
    fn test_absolute_from_subdir_ignore_file() {
        let root = Path::new("/project");
        let subdir = Path::new("/project/src");
        let info = analyze_pattern("*.test", subdir, root);

        // Universal pattern from src/.gitignore
        assert_eq!(info.absolute, PathBuf::from("/project/src/**/*.test"));
    }

    #[test]
    fn test_absolute_rooted_from_subdir() {
        let root = Path::new("/project");
        let subdir = Path::new("/project/src");
        let info = analyze_pattern("/local/cache", subdir, root);

        // Leading slash means relative to ignore file dir
        assert_eq!(info.absolute, PathBuf::from("/project/src/local/cache"));
    }

    // ========================================================================
    // Activation Path Tests
    // ========================================================================

    #[test]
    fn test_activation_path_explicit() {
        let root = Path::new("/project");
        let info = analyze_pattern("build/output.txt", root, root);

        assert_eq!(info.activation_path, PathBuf::from("/project/build"));
    }

    #[test]
    fn test_activation_path_wildcard_filename() {
        let root = Path::new("/project");
        let info = analyze_pattern("build/*.txt", root, root);

        assert_eq!(info.activation_path, PathBuf::from("/project/build"));
    }

    #[test]
    fn test_activation_path_universal() {
        let root = Path::new("/project");
        let info = analyze_pattern("*.log", root, root);

        // Universal patterns activate at root
        assert_eq!(info.activation_path, PathBuf::from("/project"));
    }

    // ========================================================================
    // Wildcard Counting Tests
    // ========================================================================

    #[test]
    fn test_count_wildcards_none() {
        assert_eq!(count_wildcards("file.txt"), 0);
    }

    #[test]
    fn test_count_wildcards_single_star() {
        assert_eq!(count_wildcards("*.txt"), 1);
    }

    #[test]
    fn test_count_wildcards_double_star() {
        assert_eq!(count_wildcards("**/*.txt"), 3); // ** = 2, * = 1
    }

    #[test]
    fn test_count_wildcards_question() {
        assert_eq!(count_wildcards("file?.txt"), 1);
    }

    #[test]
    fn test_count_wildcards_bracket() {
        assert_eq!(count_wildcards("file[0-9].txt"), 1);
    }

    // ========================================================================
    // Re-anchor Tests
    // ========================================================================

    #[test]
    fn test_reanchor_double_star_pattern() {
        let pattern = Path::new("/project/**/*.log");
        let child = Path::new("/project/src/deep");

        let result = reanchor_pattern(pattern, child);
        assert_eq!(result, PathBuf::from("/project/src/deep/**/*.log"));
    }

    #[test]
    fn test_reanchor_no_double_star() {
        let pattern = Path::new("/project/build/*.txt");
        let child = Path::new("/project/src");

        let result = reanchor_pattern(pattern, child);
        // No **, pattern unchanged
        assert_eq!(result, PathBuf::from("/project/build/*.txt"));
    }

    // ========================================================================
    // Index Building Tests
    // ========================================================================

    #[test]
    fn test_index_add_rule() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        index
            .add_rule("*.log", Action::Exclude, make_origin(), &root)
            .unwrap();

        assert_eq!(index.rules.len(), 1);
        assert_eq!(index.rules[0].info.bucket, Bucket::Universal);
    }

    #[test]
    fn test_index_build_populates_ancestors() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        // Rule activates at /project/build
        index
            .add_rule("build/output.txt", Action::Exclude, make_origin(), &root)
            .unwrap();
        index.build();

        // Should be in both /project and /project/build
        assert!(index.index.contains_key(Path::new("/project")));
        assert!(index.index.contains_key(Path::new("/project/build")));
    }

    #[test]
    fn test_index_rule_ordering() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        // Rules are kept in insertion order (last rule wins for matching)
        index
            .add_rule("**/*.log", Action::Exclude, make_origin(), &root)
            .unwrap();
        index
            .add_rule("build/*.log", Action::Exclude, make_origin(), &root)
            .unwrap();
        index
            .add_rule("build/debug.log", Action::Include, make_origin(), &root)
            .unwrap();

        index.build();

        // Rules should be in insertion order
        let rules = index.get_rules_for(Path::new("/project/build"));

        assert_eq!(rules.len(), 3);
        // Insertion order preserved: Universal, WildcardFilename, Explicit
        assert_eq!(rules[0].info.bucket, Bucket::Universal);
        assert_eq!(rules[1].info.bucket, Bucket::WildcardFilename);
        assert_eq!(rules[2].info.bucket, Bucket::ExplicitPath);

        // Last matching rule wins: build/debug.log should be Included
        let result = index.find_match(Path::new("/project/build/debug.log"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, Action::Include);
    }

    #[test]
    fn test_index_negation_flips_action() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        index
            .add_rule("!important.log", Action::Exclude, make_origin(), &root)
            .unwrap();

        assert_eq!(index.rules[0].action, Action::Include);
    }

    // ========================================================================
    // could_match_in_archive Tests
    // ========================================================================

    #[test]
    fn test_could_match_within_archive() {
        let archive_root = Path::new("/project/src");
        let activation = Path::new("/project/src/build");

        assert!(could_match_in_archive(activation, archive_root));
    }

    #[test]
    fn test_could_match_covers_archive() {
        let archive_root = Path::new("/project/src");
        let activation = Path::new("/project"); // Parent

        assert!(could_match_in_archive(activation, archive_root));
    }

    #[test]
    fn test_could_not_match_sibling() {
        let archive_root = Path::new("/project/src");
        let activation = Path::new("/project/build"); // Sibling

        assert!(!could_match_in_archive(activation, archive_root));
    }

    // ========================================================================
    // Rule Matching Tests
    // ========================================================================

    #[test]
    fn test_find_match_basic() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        index
            .add_rule("*.log", Action::Exclude, make_origin(), &root)
            .unwrap();
        index.build();

        let result = index.find_match(Path::new("/project/test.log"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, Action::Exclude);
    }

    #[test]
    fn test_find_match_no_match() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        index
            .add_rule("*.log", Action::Exclude, make_origin(), &root)
            .unwrap();
        index.build();

        let result = index.find_match(Path::new("/project/test.txt"));
        assert!(result.is_none());
    }

    #[test]
    fn test_find_match_specific_beats_general() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        index
            .add_rule("*.log", Action::Exclude, make_origin(), &root)
            .unwrap();
        index
            .add_rule("important.log", Action::Include, make_origin(), &root)
            .unwrap();
        index.build();

        // important.log should match Include (more specific)
        let result = index.find_match(Path::new("/project/important.log"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, Action::Include);

        // other.log should match Exclude
        let result = index.find_match(Path::new("/project/other.log"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, Action::Exclude);
    }

    #[test]
    fn test_find_match_deep_path() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        index
            .add_rule("*.log", Action::Exclude, make_origin(), &root)
            .unwrap();
        index.build();

        // Should match even in nested directories
        let result = index.find_match(Path::new("/project/a/b/c/test.log"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, Action::Exclude);
    }

    #[test]
    fn test_has_include_rules() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        index
            .add_rule("*.log", Action::Exclude, make_origin(), &root)
            .unwrap();
        index
            .add_rule("important.log", Action::Include, make_origin(), &root)
            .unwrap();
        index.build();

        assert!(index.has_include_rules(Path::new("/project")));
    }

    #[test]
    fn test_no_include_rules() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        index
            .add_rule("*.log", Action::Exclude, make_origin(), &root)
            .unwrap();
        index.build();

        assert!(!index.has_include_rules(Path::new("/project")));
    }

    #[test]
    fn test_directory_pattern_matching() {
        let root = PathBuf::from("/project");
        let mut index = RuleIndex::new(root.clone());

        let origin = RuleOrigin {
            source: "test".to_string(),
            line: Some(1),
        };
        index
            .add_rule("build/", Action::Exclude, origin, &root)
            .unwrap();
        index.build();

        // Check pattern was converted to **/build/**
        assert_eq!(index.rules.len(), 1);
        assert_eq!(
            index.rules[0].pattern,
            PathBuf::from("/project/**/build/**")
        );

        // Test matching - should match contents of build/
        let result = index.find_match(Path::new("/project/build/output.txt"));
        assert!(result.is_some(), "Should match build/output.txt");

        // Should NOT match files that just start with "build"
        let result = index.find_match(Path::new("/project/builder.txt"));
        assert!(result.is_none(), "Should NOT match builder.txt");
    }
}
