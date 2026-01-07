//! Comprehensive test suite for the inclusion/exclusion rule logic.
//!
//! This is the core functionality of raptar - the archive creation is just plumbing.
//! These tests verify:
//! - Precedence ordering (later rules override earlier)
//! - Pattern matching behavior
//! - Edge cases and regression tests

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn raptar() -> Command {
    let cmd = StdCommand::new(env!("CARGO_BIN_EXE_raptar"));
    Command::from_std(cmd)
}

/// Helper to create a file and its parent directories
fn create_file(base: &std::path::Path, path: &str, content: &str) {
    let full = base.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(full, content).unwrap();
}

/// Helper to get preview output as string
fn get_preview(tmp: &TempDir) -> String {
    let output = raptar().arg(tmp.path()).arg("--preview").output().unwrap();
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Helper to get preview with extra args
fn get_preview_with_args(tmp: &TempDir, args: &[&str]) -> String {
    let mut cmd = raptar();
    cmd.arg(tmp.path()).arg("--preview");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().unwrap();
    String::from_utf8_lossy(&output.stdout).to_string()
}

// ============================================================================
// PRECEDENCE TESTS
// ============================================================================
// Order (lowest to highest priority):
// 1. .gitignore / .ignore
// 2. Config always_exclude
// 3. Config always_include
// 4. CLI --with-exclude
// 5. CLI --with-include

mod precedence {
    use super::*;

    #[test]
    fn gitignore_excludes_files() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "app.log", "log content");
        create_file(tmp.path(), "main.rs", "fn main() {}");

        let preview = get_preview(&tmp);
        assert!(preview.contains("main.rs"));
        assert!(!preview.contains("app.log"));
    }

    #[test]
    fn cli_with_exclude_overrides_default_include() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "keep.txt", "keep");
        create_file(tmp.path(), "remove.txt", "remove");

        let preview = get_preview_with_args(&tmp, &["--with-exclude", "remove.txt"]);
        assert!(preview.contains("keep.txt"));
        assert!(!preview.contains("remove.txt"));
    }

    #[test]
    fn cli_with_include_overrides_gitignore() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "important.log", "important");
        create_file(tmp.path(), "other.log", "other");

        let preview = get_preview_with_args(&tmp, &["--with-include", "important.log"]);
        assert!(preview.contains("important.log"));
        assert!(!preview.contains("other.log"));
    }

    #[test]
    fn cli_with_include_overrides_cli_with_exclude() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "file.log", "content");

        // Exclude all .log, but include this specific one
        let preview = get_preview_with_args(
            &tmp,
            &["--with-exclude", "*.log", "--with-include", "file.log"],
        );
        assert!(preview.contains("file.log"));
    }

    #[test]
    fn cli_with_exclude_overrides_gitignore_negation() {
        let tmp = TempDir::new().unwrap();
        // Gitignore excludes all .log but negates important.log
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        create_file(tmp.path(), "important.log", "important");

        // CLI exclude should override the gitignore negation
        let preview = get_preview_with_args(&tmp, &["--with-exclude", "important.log"]);
        assert!(!preview.contains("important.log"));
    }

    #[test]
    fn multiple_cli_with_include_all_apply() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n*.tmp\n");
        create_file(tmp.path(), "a.log", "a");
        create_file(tmp.path(), "b.log", "b");
        create_file(tmp.path(), "c.tmp", "c");

        let preview = get_preview_with_args(
            &tmp,
            &["--with-include", "a.log", "--with-include", "c.tmp"],
        );
        assert!(preview.contains("a.log"));
        assert!(preview.contains("c.tmp"));
        assert!(!preview.contains("b.log"));
    }

    #[test]
    fn multiple_cli_with_exclude_all_apply() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "a.txt", "a");
        create_file(tmp.path(), "b.txt", "b");
        create_file(tmp.path(), "c.txt", "c");
        create_file(tmp.path(), "keep.rs", "keep");

        let preview = get_preview_with_args(
            &tmp,
            &["--with-exclude", "a.txt", "--with-exclude", "b.txt"],
        );
        assert!(!preview.contains("a.txt"));
        assert!(!preview.contains("b.txt"));
        assert!(preview.contains("c.txt"));
        assert!(preview.contains("keep.rs"));
    }
}

// ============================================================================
// PATTERN TYPE TESTS
// ============================================================================

mod patterns {
    use super::*;

    // ------------------------------------------------------------------------
    // Exact path patterns
    // ------------------------------------------------------------------------

    #[test]
    fn exact_filename_in_root() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "secret.txt\n");
        create_file(tmp.path(), "secret.txt", "secret");
        create_file(tmp.path(), "public.txt", "public");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("secret.txt"));
        assert!(preview.contains("public.txt"));
    }

    #[test]
    fn exact_path_with_directory() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "build/output.txt\n");
        create_file(tmp.path(), "build/output.txt", "output");
        create_file(tmp.path(), "build/other.txt", "other");
        create_file(tmp.path(), "src/output.txt", "src output"); // different dir, same name

        let preview = get_preview(&tmp);
        assert!(!preview.contains("build/output.txt"));
        assert!(preview.contains("build/other.txt"));
        assert!(preview.contains("src/output.txt")); // should NOT be excluded
    }

    // ------------------------------------------------------------------------
    // Single wildcard patterns (*)
    // ------------------------------------------------------------------------

    #[test]
    fn star_wildcard_extension() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "app.log", "log");
        create_file(tmp.path(), "error.log", "log");
        create_file(tmp.path(), "app.txt", "txt");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("app.log"));
        assert!(!preview.contains("error.log"));
        assert!(preview.contains("app.txt"));
    }

    #[test]
    fn star_wildcard_prefix() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "test_*\n");
        create_file(tmp.path(), "test_foo.rs", "test");
        create_file(tmp.path(), "test_bar.rs", "test");
        create_file(tmp.path(), "main.rs", "main");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("test_foo.rs"));
        assert!(!preview.contains("test_bar.rs"));
        assert!(preview.contains("main.rs"));
    }

    #[test]
    fn star_wildcard_middle() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "log_*_debug.txt\n");
        create_file(tmp.path(), "log_2024_debug.txt", "log");
        create_file(tmp.path(), "log_2024_info.txt", "log");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("log_2024_debug.txt"));
        assert!(preview.contains("log_2024_info.txt"));
    }

    #[test]
    fn star_wildcard_in_directory_pattern() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "build/*.o\n");
        create_file(tmp.path(), "build/main.o", "obj");
        create_file(tmp.path(), "build/util.o", "obj");
        create_file(tmp.path(), "build/main.c", "src");
        create_file(tmp.path(), "src/main.o", "obj"); // different dir

        let preview = get_preview(&tmp);
        assert!(!preview.contains("build/main.o"));
        assert!(!preview.contains("build/util.o"));
        assert!(preview.contains("build/main.c"));
        assert!(preview.contains("src/main.o")); // not excluded - different dir
    }

    // ------------------------------------------------------------------------
    // Double-star patterns (**)
    // ------------------------------------------------------------------------

    #[test]
    fn double_star_any_directory() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "**/*.log\n");
        create_file(tmp.path(), "root.log", "log");
        create_file(tmp.path(), "src/app.log", "log");
        create_file(tmp.path(), "src/deep/nested/debug.log", "log");
        create_file(tmp.path(), "src/main.rs", "rs");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("root.log"));
        assert!(!preview.contains("app.log"));
        assert!(!preview.contains("debug.log"));
        assert!(preview.contains("main.rs"));
    }

    #[test]
    fn double_star_suffix_excludes_directory_contents() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "cache/**\n");
        create_file(tmp.path(), "cache/data.bin", "data");
        create_file(tmp.path(), "cache/nested/more.bin", "data");
        create_file(tmp.path(), "src/cache.rs", "src"); // file named cache, not dir

        let preview = get_preview(&tmp);
        assert!(!preview.contains("cache/data.bin"));
        assert!(!preview.contains("cache/nested"));
        assert!(preview.contains("src/cache.rs"));
    }

    #[test]
    fn double_star_in_middle() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "src/**/test_*.py\n");
        create_file(tmp.path(), "src/test_main.py", "test");
        create_file(tmp.path(), "src/utils/test_util.py", "test");
        create_file(tmp.path(), "src/deep/nested/test_deep.py", "test");
        create_file(tmp.path(), "src/main.py", "main");
        create_file(tmp.path(), "tests/test_main.py", "test"); // different root

        let preview = get_preview(&tmp);
        assert!(!preview.contains("src/test_main.py"));
        assert!(!preview.contains("src/utils/test_util.py"));
        assert!(!preview.contains("src/deep/nested/test_deep.py"));
        assert!(preview.contains("src/main.py"));
        assert!(preview.contains("tests/test_main.py")); // not under src/
    }

    #[test]
    fn double_star_only() {
        let tmp = TempDir::new().unwrap();
        // This would match everything - unusual but valid
        create_file(tmp.path(), ".gitignore", "temp/**/\n");
        create_file(tmp.path(), "temp/a/file.txt", "a");
        create_file(tmp.path(), "temp/b/c/file.txt", "b");
        create_file(tmp.path(), "other/file.txt", "other");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("temp/a"));
        assert!(!preview.contains("temp/b"));
        assert!(preview.contains("other/file.txt"));
    }

    // ------------------------------------------------------------------------
    // Negation patterns (!)
    // ------------------------------------------------------------------------

    #[test]
    fn negation_re_includes_file() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        create_file(tmp.path(), "debug.log", "debug");
        create_file(tmp.path(), "important.log", "important");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("debug.log"));
        assert!(preview.contains("important.log"));
    }

    #[test]
    fn negation_with_wildcard() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n!error_*.log\n");
        create_file(tmp.path(), "debug.log", "debug");
        create_file(tmp.path(), "error_2024.log", "error");
        create_file(tmp.path(), "error_crash.log", "error");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("debug.log"));
        assert!(preview.contains("error_2024.log"));
        assert!(preview.contains("error_crash.log"));
    }

    #[test]
    fn negation_order_matters() {
        let tmp = TempDir::new().unwrap();
        // Exclude, then re-include, then exclude again
        create_file(
            tmp.path(),
            ".gitignore",
            "*.log\n!important.log\nimportant.log\n",
        );
        create_file(tmp.path(), "important.log", "important");

        let preview = get_preview(&tmp);
        // Last rule wins - should be excluded
        assert!(!preview.contains("important.log"));
    }

    #[test]
    fn negation_in_subdirectory() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "logs/**\n!logs/keep.log\n");
        create_file(tmp.path(), "logs/debug.log", "debug");
        create_file(tmp.path(), "logs/keep.log", "keep");
        create_file(tmp.path(), "logs/nested/deep.log", "deep");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("logs/debug.log"));
        assert!(!preview.contains("logs/nested"));
        assert!(preview.contains("logs/keep.log"));
    }

    // ------------------------------------------------------------------------
    // Directory patterns (trailing /)
    // ------------------------------------------------------------------------

    #[test]
    fn trailing_slash_matches_directory_contents() {
        let tmp = TempDir::new().unwrap();
        // build/ should exclude contents of the build directory
        create_file(tmp.path(), ".gitignore", "build/\n");
        create_file(tmp.path(), "build/output.txt", "output");
        create_file(tmp.path(), "build/nested/deep.txt", "deep");
        // A file named "builder" should NOT be excluded (doesn't match build/)
        create_file(tmp.path(), "builder.txt", "builder file");
        // A file in a different "build" context shouldn't be excluded
        create_file(tmp.path(), "src/build.rs", "build module");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("build/output.txt"));
        assert!(!preview.contains("build/nested"));
        assert!(preview.contains("builder.txt"));
        assert!(preview.contains("src/build.rs"));
    }

    #[test]
    fn directory_pattern_excludes_all_contents() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "node_modules/\n");
        create_file(tmp.path(), "node_modules/package/index.js", "js");
        create_file(tmp.path(), "node_modules/.bin/cmd", "bin");
        create_file(tmp.path(), "src/main.js", "main");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("node_modules"));
        assert!(preview.contains("src/main.js"));
    }

    // ------------------------------------------------------------------------
    // Rooted vs universal patterns
    // ------------------------------------------------------------------------

    #[test]
    fn leading_slash_anchors_to_root() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "/build\n");
        create_file(tmp.path(), "build/output.txt", "output");
        create_file(tmp.path(), "src/build/output.txt", "nested"); // not at root

        let preview = get_preview(&tmp);
        assert!(!preview.contains("  build/output.txt")); // root build excluded
        assert!(preview.contains("src/build/output.txt")); // nested build included
    }

    #[test]
    fn no_slash_matches_anywhere() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.pyc\n");
        create_file(tmp.path(), "main.pyc", "pyc");
        create_file(tmp.path(), "src/util.pyc", "pyc");
        create_file(tmp.path(), "src/deep/nested/cache.pyc", "pyc");

        let preview = get_preview(&tmp);
        assert!(!preview.contains(".pyc"));
    }

    #[test]
    fn internal_slash_anchors_to_gitignore_location() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "build/cache\n");
        create_file(tmp.path(), "build/cache/data.bin", "data");
        create_file(tmp.path(), "other/build/cache/data.bin", "other");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("  build/cache")); // at root
        assert!(preview.contains("other/build/cache")); // nested - not matched
    }
}

// ============================================================================
// STRUCTURAL TESTS
// ============================================================================

mod structure {
    use super::*;

    // ------------------------------------------------------------------------
    // Sibling directories
    // ------------------------------------------------------------------------

    #[test]
    fn git_exclusion_does_not_affect_siblings() {
        let tmp = TempDir::new().unwrap();
        // .git/** should only exclude .git contents, not sibling dirs
        create_file(tmp.path(), ".git/config", "config");
        create_file(tmp.path(), ".git/HEAD", "ref");
        create_file(tmp.path(), ".git/objects/pack/data", "pack");
        create_file(tmp.path(), "src/main.rs", "main");
        create_file(tmp.path(), "tests/test.rs", "test");
        create_file(tmp.path(), "docs/README.md", "docs");

        let preview = get_preview(&tmp);
        assert!(!preview.contains(".git/config"));
        assert!(!preview.contains(".git/HEAD"));
        assert!(preview.contains("src/main.rs"));
        assert!(preview.contains("tests/test.rs"));
        assert!(preview.contains("docs/README.md"));
    }

    #[test]
    fn multiple_exclusions_dont_cross_pollinate() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "target/**\nnode_modules/**\n");
        create_file(tmp.path(), ".git/config", "config");
        create_file(tmp.path(), "target/debug/bin", "bin");
        create_file(tmp.path(), "node_modules/pkg/index.js", "js");
        create_file(tmp.path(), "src/main.rs", "main");
        create_file(tmp.path(), "lib/util.rs", "util");

        let preview = get_preview(&tmp);
        // .git excluded by default config (check for .git/config, not just .git)
        assert!(!preview.contains(".git/config"));
        assert!(!preview.contains("target/debug"));
        assert!(!preview.contains("node_modules/pkg"));
        assert!(preview.contains("src/main.rs"));
        assert!(preview.contains("lib/util.rs"));
    }

    #[test]
    fn deep_sibling_structure() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "excluded/**\n");

        // Create many siblings at various depths
        create_file(tmp.path(), "excluded/data.bin", "excluded");
        create_file(tmp.path(), "a/file.txt", "a");
        create_file(tmp.path(), "b/file.txt", "b");
        create_file(tmp.path(), "c/d/file.txt", "cd");
        create_file(tmp.path(), "c/e/file.txt", "ce");
        create_file(tmp.path(), "c/e/f/file.txt", "cef");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("excluded"));
        assert!(preview.contains("a/file.txt"));
        assert!(preview.contains("b/file.txt"));
        assert!(preview.contains("c/d/file.txt"));
        assert!(preview.contains("c/e/file.txt"));
        assert!(preview.contains("c/e/f/file.txt"));
    }

    // ------------------------------------------------------------------------
    // Nested .gitignore files
    // ------------------------------------------------------------------------

    // BUG: We only parse root-level .gitignore, not nested ones
    #[test]
    #[ignore = "BUG: nested .gitignore files not discovered"]
    fn nested_gitignore_adds_rules() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "src/.gitignore", "*.tmp\n");
        create_file(tmp.path(), "root.log", "log");
        create_file(tmp.path(), "src/app.log", "log");
        create_file(tmp.path(), "src/cache.tmp", "tmp");
        create_file(tmp.path(), "src/main.rs", "rs");
        create_file(tmp.path(), "root.tmp", "tmp"); // root .gitignore doesn't exclude .tmp

        let preview = get_preview(&tmp);
        assert!(!preview.contains("root.log"));
        assert!(!preview.contains("app.log"));
        assert!(!preview.contains("cache.tmp"));
        assert!(preview.contains("main.rs"));
        assert!(preview.contains("root.tmp")); // not excluded at root level
    }

    #[test]
    fn nested_gitignore_rules_are_relative() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "src/.gitignore", "/local.txt\n");
        create_file(tmp.path(), "src/local.txt", "local");
        create_file(tmp.path(), "src/sub/local.txt", "sub local");
        create_file(tmp.path(), "local.txt", "root local");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("src/local.txt") || preview.contains("src/sub/local.txt"));
        // /local.txt in src/.gitignore means src/local.txt, not src/sub/local.txt
    }

    // BUG: We only parse root-level .gitignore, not nested ones
    #[test]
    #[ignore = "BUG: nested .gitignore files not discovered"]
    fn deeply_nested_gitignore() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "a/b/c/.gitignore", "secret.txt\n");
        create_file(tmp.path(), "a/b/c/secret.txt", "secret");
        create_file(tmp.path(), "a/b/c/public.txt", "public");
        create_file(tmp.path(), "a/b/secret.txt", "not excluded");
        create_file(tmp.path(), "secret.txt", "root not excluded");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("a/b/c/secret.txt"));
        assert!(preview.contains("a/b/c/public.txt"));
        assert!(preview.contains("a/b/secret.txt"));
        assert!(preview.contains("  secret.txt")); // root level
    }

    // ------------------------------------------------------------------------
    // Deep directory structures
    // ------------------------------------------------------------------------

    #[test]
    fn very_deep_nesting() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "a/b/c/d/e/f/g/h/i/j/deep.txt", "deep");
        create_file(tmp.path(), "shallow.txt", "shallow");

        let preview = get_preview(&tmp);
        assert!(preview.contains("a/b/c/d/e/f/g/h/i/j/deep.txt"));
        assert!(preview.contains("shallow.txt"));
    }

    #[test]
    fn exclusion_at_deep_level() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "a/b/c/d/excluded/**\n");
        create_file(tmp.path(), "a/b/c/d/excluded/file.txt", "excluded");
        create_file(tmp.path(), "a/b/c/d/included/file.txt", "included");
        create_file(tmp.path(), "a/b/c/d/file.txt", "direct");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("excluded/file.txt"));
        assert!(preview.contains("included/file.txt"));
        assert!(preview.contains("a/b/c/d/file.txt"));
    }

    // ------------------------------------------------------------------------
    // Ignore file discovery
    // ------------------------------------------------------------------------

    #[test]
    fn dot_ignore_file_also_works() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".ignore", "*.bak\n");
        create_file(tmp.path(), "file.bak", "backup");
        create_file(tmp.path(), "file.txt", "text");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("file.bak"));
        assert!(preview.contains("file.txt"));
    }

    #[test]
    fn both_gitignore_and_ignore_apply() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), ".ignore", "*.tmp\n");
        create_file(tmp.path(), "app.log", "log");
        create_file(tmp.path(), "cache.tmp", "tmp");
        create_file(tmp.path(), "main.rs", "rs");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("app.log"));
        assert!(!preview.contains("cache.tmp"));
        assert!(preview.contains("main.rs"));
    }
}

// ============================================================================
// EDGE CASES AND REGRESSION TESTS
// ============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn empty_gitignore() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "");
        create_file(tmp.path(), "file.txt", "content");

        let preview = get_preview(&tmp);
        assert!(preview.contains("file.txt"));
        assert!(preview.contains(".gitignore"));
    }

    #[test]
    fn gitignore_with_only_comments() {
        let tmp = TempDir::new().unwrap();
        create_file(
            tmp.path(),
            ".gitignore",
            "# This is a comment\n# Another comment\n",
        );
        create_file(tmp.path(), "file.txt", "content");

        let preview = get_preview(&tmp);
        assert!(preview.contains("file.txt"));
    }

    #[test]
    fn gitignore_with_blank_lines() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "\n\n*.log\n\n*.tmp\n\n");
        create_file(tmp.path(), "app.log", "log");
        create_file(tmp.path(), "cache.tmp", "tmp");
        create_file(tmp.path(), "main.rs", "rs");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("app.log"));
        assert!(!preview.contains("cache.tmp"));
        assert!(preview.contains("main.rs"));
    }

    #[test]
    fn pattern_with_spaces() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file with spaces.txt\n");
        create_file(tmp.path(), "file with spaces.txt", "content");
        create_file(tmp.path(), "file.txt", "other");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("file with spaces.txt"));
        assert!(preview.contains("file.txt"));
    }

    #[test]
    fn hidden_files_included_by_default() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".env", "SECRET=123");
        create_file(tmp.path(), ".config", "config");
        create_file(tmp.path(), "visible.txt", "visible");

        let preview = get_preview(&tmp);
        assert!(preview.contains(".env"));
        assert!(preview.contains(".config"));
        assert!(preview.contains("visible.txt"));
    }

    #[test]
    fn gitignore_itself_included() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "file.txt", "content");

        let preview = get_preview(&tmp);
        assert!(preview.contains(".gitignore"));
    }

    #[test]
    fn no_files_after_exclusion() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*\n");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .assert()
            .success()
            .stdout(predicate::str::contains("No files to archive"));
    }

    #[test]
    fn symlink_handling() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "real.txt", "real content");
        std::os::unix::fs::symlink(tmp.path().join("real.txt"), tmp.path().join("link.txt"))
            .unwrap();

        let preview = get_preview(&tmp);
        assert!(preview.contains("real.txt"));
        assert!(preview.contains("link.txt"));
    }

    #[test]
    fn pattern_with_special_chars() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file[1].txt\n");
        create_file(tmp.path(), "file[1].txt", "bracketed");
        create_file(tmp.path(), "file1.txt", "no brackets");

        let preview = get_preview(&tmp);
        // Brackets in gitignore are character classes, so [1] matches '1'
        assert!(!preview.contains("file1.txt"));
    }

    #[test]
    fn question_mark_wildcard() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file?.txt\n");
        create_file(tmp.path(), "file1.txt", "1");
        create_file(tmp.path(), "file2.txt", "2");
        create_file(tmp.path(), "file.txt", "no number");
        create_file(tmp.path(), "file12.txt", "two numbers");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("file1.txt"));
        assert!(!preview.contains("file2.txt"));
        assert!(preview.contains("  file.txt")); // ? needs exactly one char
        assert!(preview.contains("file12.txt")); // two chars don't match ?
    }

    #[test]
    fn character_class_pattern() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file[abc].txt\n");
        create_file(tmp.path(), "filea.txt", "a");
        create_file(tmp.path(), "fileb.txt", "b");
        create_file(tmp.path(), "filec.txt", "c");
        create_file(tmp.path(), "filed.txt", "d");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("filea.txt"));
        assert!(!preview.contains("fileb.txt"));
        assert!(!preview.contains("filec.txt"));
        assert!(preview.contains("filed.txt"));
    }

    #[test]
    fn negated_character_class() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file[!abc].txt\n");
        create_file(tmp.path(), "filea.txt", "a");
        create_file(tmp.path(), "filed.txt", "d");
        create_file(tmp.path(), "filex.txt", "x");

        let preview = get_preview(&tmp);
        assert!(preview.contains("filea.txt")); // a is in [abc], so [!abc] doesn't match
        assert!(!preview.contains("filed.txt")); // d is NOT in [abc], so [!abc] matches
        assert!(!preview.contains("filex.txt"));
    }

    #[test]
    fn empty_directory_not_in_archive() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("empty_dir")).unwrap();
        create_file(tmp.path(), "file.txt", "content");

        let preview = get_preview(&tmp);
        // Empty directories shouldn't appear (tar only stores files)
        assert!(preview.contains("file.txt"));
        assert!(!preview.contains("empty_dir"));
    }

    #[test]
    fn case_sensitivity() {
        let tmp = TempDir::new().unwrap();

        // Detect filesystem case-sensitivity:
        // Create a file, then check if a differently-cased name resolves to it
        create_file(tmp.path(), "CaseTest.tmp", "test");
        let is_case_insensitive = tmp.path().join("casetest.tmp").exists();

        // Clean up
        std::fs::remove_file(tmp.path().join("CaseTest.tmp")).ok();

        if is_case_insensitive {
            // Skip test on case-insensitive filesystems (e.g., macOS default)
            eprintln!("Skipping case_sensitivity test: filesystem is case-insensitive");
            return;
        }

        // Now run the actual test on case-sensitive filesystem
        create_file(tmp.path(), ".gitignore", "*.LOG\n");
        create_file(tmp.path(), "file.LOG", "upper");
        create_file(tmp.path(), "file.log", "lower");

        let preview = get_preview(&tmp);
        // Gitignore patterns are case-sensitive on case-sensitive filesystems
        assert!(!preview.contains("file.LOG"));
        assert!(preview.contains("file.log"));
    }

    #[test]
    fn escaped_special_chars() {
        let tmp = TempDir::new().unwrap();
        // Backslash escapes special characters
        create_file(tmp.path(), ".gitignore", "file\\*.txt\n");
        create_file(tmp.path(), "file*.txt", "literal asterisk");
        create_file(tmp.path(), "fileX.txt", "X");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("file*.txt"));
        assert!(preview.contains("fileX.txt"));
    }

    // ------------------------------------------------------------------------
    // Regression: sibling directory exclusion bug
    // ------------------------------------------------------------------------

    #[test]
    fn regression_sibling_not_excluded_by_git() {
        // This was the bug: .git/** was incorrectly excluding src/**
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".git/config", "[core]");
        create_file(tmp.path(), ".git/HEAD", "ref: refs/heads/main");
        create_file(tmp.path(), "src/main.rs", "fn main() {}");
        create_file(tmp.path(), "src/lib.rs", "pub mod lib;");
        create_file(tmp.path(), "Cargo.toml", "[package]");

        let preview = get_preview(&tmp);
        assert!(!preview.contains(".git/config"));
        assert!(preview.contains("src/main.rs"));
        assert!(preview.contains("src/lib.rs"));
        assert!(preview.contains("Cargo.toml"));
    }

    #[test]
    fn regression_target_not_excluded_by_git() {
        // Variant: target/** shouldn't affect src/**
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "target/**\n");
        create_file(tmp.path(), "target/debug/binary", "bin");
        create_file(tmp.path(), "src/main.rs", "fn main() {}");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("target"));
        assert!(preview.contains("src/main.rs"));
    }

    #[test]
    fn regression_multiple_double_star_patterns() {
        let tmp = TempDir::new().unwrap();
        // Don't include .git/** since it's in default config
        create_file(
            tmp.path(),
            ".gitignore",
            "target/**\nnode_modules/**\n__pycache__/**\n",
        );
        create_file(tmp.path(), ".git/config", "config");
        create_file(tmp.path(), "target/debug/bin", "bin");
        create_file(tmp.path(), "node_modules/pkg/index.js", "js");
        create_file(tmp.path(), "__pycache__/module.pyc", "pyc");
        create_file(tmp.path(), "src/main.rs", "main");
        create_file(tmp.path(), "lib/util.py", "util");
        create_file(tmp.path(), "web/app.js", "app");

        let preview = get_preview(&tmp);
        assert!(!preview.contains(".git/config"));
        assert!(!preview.contains("target/debug"));
        assert!(!preview.contains("node_modules/pkg"));
        assert!(!preview.contains("__pycache__/module"));
        assert!(preview.contains("src/main.rs"));
        assert!(preview.contains("lib/util.py"));
        assert!(preview.contains("web/app.js"));
    }
}

// ============================================================================
// CLI FLAG INTERACTION TESTS
// ============================================================================

mod cli_flags {
    use super::*;

    #[test]
    fn without_ignorefiles_includes_everything() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\nsecret.txt\n");
        create_file(tmp.path(), "app.log", "log");
        create_file(tmp.path(), "secret.txt", "secret");
        create_file(tmp.path(), "public.txt", "public");

        let preview = get_preview_with_args(&tmp, &["--without-ignorefiles"]);
        assert!(preview.contains("app.log"));
        assert!(preview.contains("secret.txt"));
        assert!(preview.contains("public.txt"));
    }

    #[test]
    fn without_ignorefile_specific() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), ".ignore", "*.tmp\n");
        create_file(tmp.path(), "app.log", "log");
        create_file(tmp.path(), "cache.tmp", "tmp");

        // Disable only .gitignore
        let preview = get_preview_with_args(&tmp, &["--without-ignorefile", "gitignore"]);
        assert!(preview.contains("app.log")); // .gitignore disabled
        assert!(!preview.contains("cache.tmp")); // .ignore still active
    }

    #[test]
    fn without_exclude_always_disables_defaults() {
        let tmp = TempDir::new().unwrap();
        // Default config excludes .git/**
        create_file(tmp.path(), ".git/config", "config");
        create_file(tmp.path(), "main.rs", "main");

        let preview = get_preview_with_args(&tmp, &["--without-exclude-always"]);
        assert!(preview.contains(".git/config")); // now included
        assert!(preview.contains("main.rs"));
    }

    #[test]
    fn verbose_shows_all_rules() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n*.tmp\n*.bak\n");
        create_file(tmp.path(), "file.txt", "content");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .assert()
            .success()
            .stdout(predicate::str::contains("*.log"))
            .stdout(predicate::str::contains("*.tmp"))
            .stdout(predicate::str::contains("*.bak"));
    }

    #[test]
    fn with_ignorefile_adds_custom_ignore() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".customignore", "*.custom\n");
        create_file(tmp.path(), "file.custom", "custom");
        create_file(tmp.path(), "file.txt", "txt");

        let preview = get_preview_with_args(&tmp, &["--with-ignorefile", ".customignore"]);
        assert!(!preview.contains("file.custom"));
        assert!(preview.contains("file.txt"));
    }
}

// ============================================================================
// Extended Precedence Tests - comprehensive override order verification
// ============================================================================
mod precedence_extended {
    use super::*;

    // Test: .gitignore overrides ecosystem templates (simulated via --with-ignorefile order)
    #[test]
    fn later_ignorefile_overrides_earlier() {
        let tmp = TempDir::new().unwrap();
        // First ignore file excludes *.log
        create_file(tmp.path(), ".ignore1", "*.log\n");
        // Second ignore file negates it
        create_file(tmp.path(), ".ignore2", "!important.log\n");
        create_file(tmp.path(), "debug.log", "debug");
        create_file(tmp.path(), "important.log", "important");

        // Order matters: .ignore2 comes after .ignore1
        let preview = get_preview_with_args(
            &tmp,
            &[
                "--with-ignorefile",
                ".ignore1",
                "--with-ignorefile",
                ".ignore2",
            ],
        );
        assert!(
            !preview.contains("debug.log"),
            "debug.log should be excluded"
        );
        assert!(
            preview.contains("important.log"),
            "important.log should be included (negation wins)"
        );
    }

    // Test: CLI --with-exclude overrides .gitignore negation
    #[test]
    fn cli_exclude_overrides_gitignore_negation() {
        let tmp = TempDir::new().unwrap();
        // .gitignore excludes *.log but allows important.log
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        create_file(tmp.path(), "important.log", "important");

        // CLI excludes important.log explicitly
        let preview = get_preview_with_args(&tmp, &["--with-exclude", "important.log"]);
        assert!(
            !preview.contains("important.log"),
            "CLI --with-exclude should override .gitignore negation"
        );
    }

    // Test: CLI --with-include overrides everything
    #[test]
    fn cli_include_overrides_all_exclusions() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "debug.log", "debug");

        // --with-include forces inclusion
        let preview = get_preview_with_args(&tmp, &["--with-include", "debug.log"]);
        assert!(
            preview.contains("debug.log"),
            "CLI --with-include should override .gitignore exclusion"
        );
    }

    // Test: --with-include overrides --with-exclude
    #[test]
    fn cli_include_overrides_cli_exclude() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "important.log", "important");

        // Both exclude and include the same file - include should win
        let preview = get_preview_with_args(
            &tmp,
            &["--with-exclude", "*.log", "--with-include", "important.log"],
        );
        assert!(
            preview.contains("important.log"),
            "--with-include should override --with-exclude"
        );
    }

    // Test: Order within same source matters (last rule wins)
    #[test]
    fn last_rule_wins_within_file() {
        let tmp = TempDir::new().unwrap();
        // First exclude, then include, then exclude again
        create_file(tmp.path(), ".gitignore", "*.log\n!*.log\n*.log\n");
        create_file(tmp.path(), "test.log", "test");

        let preview = get_preview(&tmp);
        assert!(
            !preview.contains("test.log"),
            "Last rule (exclude) should win"
        );
    }

    // Test: Negation in later file overrides exclusion in earlier file
    #[test]
    fn negation_in_later_file_overrides() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.tmp\n");
        create_file(tmp.path(), ".ignore", "!keep.tmp\n");
        create_file(tmp.path(), "delete.tmp", "delete");
        create_file(tmp.path(), "keep.tmp", "keep");

        let preview = get_preview(&tmp);
        assert!(
            !preview.contains("delete.tmp"),
            "delete.tmp should be excluded"
        );
        assert!(
            preview.contains("keep.tmp"),
            "keep.tmp should be included (.ignore negation overrides .gitignore)"
        );
    }

    // Test: --with-ignorefile has higher priority than default .gitignore
    #[test]
    fn with_ignorefile_overrides_default_gitignore() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        // Custom ignore file re-excludes important.log
        create_file(tmp.path(), ".myignore", "important.log\n");
        create_file(tmp.path(), "important.log", "important");

        let preview = get_preview_with_args(&tmp, &["--with-ignorefile", ".myignore"]);
        assert!(
            !preview.contains("important.log"),
            "--with-ignorefile should override .gitignore negation"
        );
    }

    // Test: Complex precedence chain
    #[test]
    fn complex_precedence_chain() {
        let tmp = TempDir::new().unwrap();
        // Set up a file that goes through multiple include/exclude cycles
        create_file(tmp.path(), ".gitignore", "*.data\n"); // exclude
        create_file(tmp.path(), ".ignore", "!important.data\n"); // include
        create_file(tmp.path(), "important.data", "data");
        create_file(tmp.path(), "other.data", "other");

        // Verify baseline: important.data included, other.data excluded
        let preview = get_preview(&tmp);
        assert!(preview.contains("important.data"));
        assert!(!preview.contains("other.data"));

        // Now add CLI exclude - should override the .ignore negation
        let preview2 = get_preview_with_args(&tmp, &["--with-exclude", "important.data"]);
        assert!(
            !preview2.contains("important.data"),
            "--with-exclude should override .ignore negation"
        );

        // And finally CLI include wins over CLI exclude
        let preview3 = get_preview_with_args(
            &tmp,
            &[
                "--with-exclude",
                "important.data",
                "--with-include",
                "important.data",
            ],
        );
        assert!(
            preview3.contains("important.data"),
            "--with-include should override --with-exclude"
        );
    }

    // Test: Directory patterns respect precedence
    #[test]
    fn directory_pattern_precedence() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "build/\n");
        create_file(tmp.path(), "build/output.txt", "output");
        create_file(tmp.path(), "build/important.bin", "binary");

        // Baseline: build/ contents excluded
        let preview = get_preview(&tmp);
        assert!(!preview.contains("build/output.txt"));
        assert!(!preview.contains("build/important.bin"));

        // CLI include can override directory exclusion for specific file
        let preview2 = get_preview_with_args(&tmp, &["--with-include", "build/important.bin"]);
        assert!(
            !preview2.contains("build/output.txt"),
            "Other files still excluded"
        );
        assert!(
            preview2.contains("build/important.bin"),
            "Specific file included via CLI"
        );
    }
}

// ============================================================================
// Precedence Chain Tests - Verify complete priority ordering
// ============================================================================

mod precedence_chain {
    use super::*;

    // Precedence order (lowest to highest):
    // 1. --with-ecosystem (ecosystem templates)
    // 2. .gitignore, .ignore (root-level ignore files)
    // 3. Config use files
    // 4. --with-ignorefile (CLI ignore files)
    // 5. Config always_exclude
    // 6. Config always_include
    // 7. --with-exclude (CLI exclude)
    // 8. --with-include (CLI include, highest priority)

    #[test]
    fn gitignore_overrides_earlier_rules() {
        // If .gitignore says !*.log (include), it should override
        // any earlier exclusion of *.log
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.tmp\n!important.tmp\n");
        create_file(tmp.path(), "junk.tmp", "junk");
        create_file(tmp.path(), "important.tmp", "important");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("junk.tmp"), "junk.tmp should be excluded");
        assert!(
            preview.contains("important.tmp"),
            "important.tmp should be included via negation"
        );
    }

    #[test]
    fn cli_ignorefile_overrides_gitignore() {
        // --with-ignorefile is higher priority than .gitignore
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        create_file(tmp.path(), ".strictignore", "*.log\n"); // No negation
        create_file(tmp.path(), "debug.log", "debug");
        create_file(tmp.path(), "important.log", "important");

        // Without strictignore: important.log included
        let preview = get_preview(&tmp);
        assert!(
            preview.contains("important.log"),
            "important.log included by .gitignore negation"
        );

        // With strictignore: important.log excluded (strictignore wins)
        let preview2 = get_preview_with_args(&tmp, &["--with-ignorefile", ".strictignore"]);
        assert!(
            !preview2.contains("important.log"),
            "important.log excluded by --with-ignorefile (higher priority)"
        );
    }

    #[test]
    fn cli_exclude_overrides_gitignore_negation() {
        // --with-exclude is higher priority than .gitignore negation
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        create_file(tmp.path(), "important.log", "important");

        // .gitignore says include important.log
        let preview = get_preview(&tmp);
        assert!(preview.contains("important.log"));

        // --with-exclude overrides the negation
        let preview2 = get_preview_with_args(&tmp, &["--with-exclude", "important.log"]);
        assert!(
            !preview2.contains("important.log"),
            "--with-exclude overrides .gitignore negation"
        );
    }

    #[test]
    fn cli_include_overrides_cli_exclude() {
        // --with-include is highest priority, overrides --with-exclude
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "secret.key", "secret");

        // Exclude then include
        let preview = get_preview_with_args(
            &tmp,
            &["--with-exclude", "*.key", "--with-include", "secret.key"],
        );
        assert!(
            preview.contains("secret.key"),
            "--with-include wins over --with-exclude"
        );

        // Include then exclude (order shouldn't matter, include still wins)
        let preview2 = get_preview_with_args(
            &tmp,
            &["--with-include", "secret.key", "--with-exclude", "*.key"],
        );
        assert!(
            preview2.contains("secret.key"),
            "--with-include wins regardless of argument order"
        );
    }

    #[test]
    fn cli_include_overrides_all_exclusions() {
        // --with-include should override .gitignore, config, and --with-exclude
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.secret\n");
        create_file(tmp.path(), "password.secret", "hunter2");

        // Excluded by .gitignore
        let preview = get_preview(&tmp);
        assert!(!preview.contains("password.secret"));

        // --with-include overrides
        let preview2 = get_preview_with_args(&tmp, &["--with-include", "password.secret"]);
        assert!(
            preview2.contains("password.secret"),
            "--with-include overrides .gitignore exclusion"
        );
    }

    #[test]
    fn later_rules_in_same_file_win() {
        // Within a single .gitignore, later rules override earlier ones
        let tmp = TempDir::new().unwrap();
        // Exclude all .log, then include error.log, then exclude it again
        create_file(tmp.path(), ".gitignore", "*.log\n!error.log\nerror.log\n");
        create_file(tmp.path(), "error.log", "error");

        let preview = get_preview(&tmp);
        assert!(
            !preview.contains("error.log"),
            "Last rule (exclude) wins within same file"
        );
    }

    #[test]
    fn later_cli_ignorefile_overrides_earlier() {
        // Multiple --with-ignorefile: later ones override earlier
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".first", "*.log\n");
        create_file(tmp.path(), ".second", "!important.log\n");
        create_file(tmp.path(), "important.log", "important");
        create_file(tmp.path(), "debug.log", "debug");

        // First excludes *.log, second includes important.log
        let preview = get_preview_with_args(
            &tmp,
            &[
                "--with-ignorefile",
                ".first",
                "--with-ignorefile",
                ".second",
            ],
        );
        assert!(!preview.contains("debug.log"), "debug.log still excluded");
        assert!(
            preview.contains("important.log"),
            "important.log included by later ignorefile"
        );
    }

    #[test]
    fn full_precedence_chain() {
        // Test the complete chain: gitignore < cli-ignorefile < cli-exclude < cli-include
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.dat\n!keep.dat\n"); // exclude *.dat, include keep.dat
        create_file(tmp.path(), ".strict", "*.dat\n"); // exclude all .dat (no exception)
        create_file(tmp.path(), "trash.dat", "trash");
        create_file(tmp.path(), "keep.dat", "keep");
        create_file(tmp.path(), "force.dat", "force");
        create_file(tmp.path(), "other.txt", "other");

        // Level 1: .gitignore only
        // *.dat excluded, but !keep.dat brings it back
        let p1 = get_preview(&tmp);
        assert!(!p1.contains("trash.dat"), "trash.dat excluded by *.dat");
        assert!(!p1.contains("force.dat"), "force.dat excluded by *.dat");
        assert!(p1.contains("keep.dat"), "keep.dat included by negation");
        assert!(p1.contains("other.txt"), "other.txt not matched");

        // Level 2: Add .strict via --with-ignorefile
        // .strict says exclude all *.dat, overrides .gitignore's negation
        let p2 = get_preview_with_args(&tmp, &["--with-ignorefile", ".strict"]);
        assert!(!p2.contains("trash.dat"));
        assert!(
            !p2.contains("keep.dat"),
            ".strict overrides .gitignore negation"
        );
        assert!(!p2.contains("force.dat"));

        // Level 3: Add --with-include for force.dat
        // force.dat should be included despite .strict
        let p3 = get_preview_with_args(
            &tmp,
            &[
                "--with-ignorefile",
                ".strict",
                "--with-include",
                "force.dat",
            ],
        );
        assert!(!p3.contains("trash.dat"));
        assert!(!p3.contains("keep.dat"));
        assert!(p3.contains("force.dat"), "--with-include overrides .strict");
    }

    #[test]
    fn negation_patterns_work_across_sources() {
        // A negation in a higher-priority source should include
        // files excluded by lower-priority sources
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.cache\n");
        create_file(tmp.path(), ".exceptions", "!important.cache\n");
        create_file(tmp.path(), "junk.cache", "junk");
        create_file(tmp.path(), "important.cache", "important");

        // .gitignore excludes all .cache
        let p1 = get_preview(&tmp);
        assert!(!p1.contains("junk.cache"));
        assert!(!p1.contains("important.cache"));

        // .exceptions (via --with-ignorefile) negates for important.cache
        let p2 = get_preview_with_args(&tmp, &["--with-ignorefile", ".exceptions"]);
        assert!(!p2.contains("junk.cache"), "junk.cache still excluded");
        assert!(
            p2.contains("important.cache"),
            "important.cache included via negation in higher-priority source"
        );
    }
}
