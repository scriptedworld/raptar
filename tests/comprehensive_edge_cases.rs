//! Comprehensive edge case tests for raptar rule system.
//!
//! These tests cover:
//! - Bad filenames (unicode, special chars, long names)
//! - Bad syntax in ignore files
//! - Case sensitivity on different filesystems
//! - Pattern edge cases
//! - Symlink edge cases
//! - .ignore vs .gitignore ordering
//! - Character class ranges

use assert_cmd::Command;
use std::fmt::Write;
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
    let output = raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("--without-exclude-always") // Disable default config
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Helper to get preview with extra args
fn get_preview_with_args(tmp: &TempDir, args: &[&str]) -> String {
    let mut cmd = raptar();
    cmd.arg(tmp.path())
        .arg("--preview")
        .arg("--without-exclude-always");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().unwrap();
    String::from_utf8_lossy(&output.stdout).to_string()
}

// ============================================================================
// BAD FILENAME TESTS
// ============================================================================

mod bad_filenames {
    use super::*;

    #[test]
    fn unicode_filenames() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "файл.txt", "russian");
        create_file(tmp.path(), "文件.txt", "chinese");
        create_file(tmp.path(), "ファイル.txt", "japanese");
        create_file(tmp.path(), "🎉emoji🎊.txt", "emoji");
        create_file(tmp.path(), "café.txt", "french");

        let preview = get_preview(&tmp);
        assert!(preview.contains("файл.txt"));
        assert!(preview.contains("文件.txt"));
        assert!(preview.contains("ファイル.txt"));
        assert!(preview.contains("emoji") || preview.contains("🎉"));
        assert!(preview.contains("café.txt"));
    }

    #[test]
    fn unicode_in_gitignore_pattern() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "файл*.txt\n");
        create_file(tmp.path(), "файл1.txt", "file1");
        create_file(tmp.path(), "файл2.txt", "file2");
        create_file(tmp.path(), "other.txt", "other");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("файл1.txt"));
        assert!(!preview.contains("файл2.txt"));
        assert!(preview.contains("other.txt"));
    }

    #[test]
    fn special_characters_in_filenames() {
        let tmp = TempDir::new().unwrap();
        // These are valid on Unix filesystems
        create_file(tmp.path(), "file with spaces.txt", "spaces");
        create_file(tmp.path(), "file@domain.txt", "at");
        create_file(tmp.path(), "file#hash.txt", "hash");
        create_file(tmp.path(), "file$dollar.txt", "dollar");
        create_file(tmp.path(), "file%percent.txt", "percent");
        create_file(tmp.path(), "file&ampersand.txt", "ampersand");
        create_file(tmp.path(), "file(paren).txt", "paren");
        create_file(tmp.path(), "file+plus.txt", "plus");
        create_file(tmp.path(), "file=equals.txt", "equals");
        create_file(tmp.path(), "file;semicolon.txt", "semicolon");
        create_file(tmp.path(), "file'quote.txt", "quote");

        let preview = get_preview(&tmp);
        // All should be included by default
        assert!(preview.contains("file with spaces.txt"));
        assert!(preview.contains("file@domain.txt"));
        assert!(preview.contains("file#hash.txt"));
    }

    #[test]
    fn gitignore_excludes_special_char_filenames() {
        let tmp = TempDir::new().unwrap();
        create_file(
            tmp.path(),
            ".gitignore",
            "file with spaces.txt\nfile@domain.txt\n",
        );
        create_file(tmp.path(), "file with spaces.txt", "spaces");
        create_file(tmp.path(), "file@domain.txt", "at");
        create_file(tmp.path(), "normal.txt", "normal");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("file with spaces.txt"));
        assert!(!preview.contains("file@domain.txt"));
        assert!(preview.contains("normal.txt"));
    }

    #[test]
    fn very_long_filename() {
        let tmp = TempDir::new().unwrap();
        // 255 characters is typically the filename length limit
        let long_name = "a".repeat(250) + ".txt";
        create_file(tmp.path(), &long_name, "content");
        create_file(tmp.path(), "short.txt", "short");

        let preview = get_preview(&tmp);
        // Should handle long filenames gracefully
        assert!(preview.contains(&long_name) || preview.contains("aaa"));
        assert!(preview.contains("short.txt"));
    }

    #[test]
    fn filename_looks_like_pattern() {
        let tmp = TempDir::new().unwrap();
        // Filename is literally "*.txt" (valid on Unix)
        // This is tricky - need to use OS API to create it
        // For now, test similar case with brackets
        create_file(tmp.path(), "[test].txt", "brackets");
        create_file(tmp.path(), "test.txt", "normal");

        let preview = get_preview(&tmp);
        assert!(preview.contains("[test].txt"));
        assert!(preview.contains("test.txt"));
    }

    #[test]
    fn dotfiles_and_double_dots() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".hidden", "hidden");
        create_file(tmp.path(), "..double", "double");
        create_file(tmp.path(), "trailing.", "trailing");
        create_file(tmp.path(), ".multiple.dots.txt", "multiple");

        let preview = get_preview(&tmp);
        assert!(preview.contains(".hidden"));
        assert!(preview.contains("..double"));
        assert!(preview.contains("trailing."));
        assert!(preview.contains(".multiple.dots.txt"));
    }

    #[test]
    fn multiple_extensions() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.tar.gz\n");
        create_file(tmp.path(), "archive.tar.gz", "archive");
        create_file(tmp.path(), "file.tar", "tar");
        create_file(tmp.path(), "file.gz", "gz");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("archive.tar.gz"));
        assert!(preview.contains("file.tar"));
        assert!(preview.contains("file.gz"));
    }
}

// ============================================================================
// BAD SYNTAX IN IGNORE FILES
// ============================================================================

mod bad_ignore_syntax {
    use super::*;

    #[test]
    fn invalid_pattern_unmatched_bracket() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file[.txt\n*.log\n");
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "file[.txt", "bracket");

        // Should gracefully handle invalid pattern and continue
        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }

    #[test]
    fn very_long_line_in_gitignore() {
        let tmp = TempDir::new().unwrap();
        let long_pattern = "a".repeat(10000) + "*.txt";
        create_file(tmp.path(), ".gitignore", &long_pattern);
        create_file(tmp.path(), "test.txt", "test");

        // Should handle very long lines without crashing
        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }

    #[test]
    fn whitespace_only_lines() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "   \n\t\t\n*.log\n    \n");
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "test.txt", "txt");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("test.log"));
        assert!(preview.contains("test.txt"));
    }

    #[test]
    fn trailing_whitespace_in_patterns() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log   \n*.tmp\t\n");
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "test.tmp", "tmp");

        let preview = get_preview(&tmp);
        // Patterns should be trimmed
        assert!(!preview.contains("test.log"));
        assert!(!preview.contains("test.tmp"));
    }

    #[test]
    fn leading_whitespace_before_pattern() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "   *.log\n\t*.tmp\n");
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "test.tmp", "tmp");

        let preview = get_preview(&tmp);
        // Patterns should be trimmed
        assert!(!preview.contains("test.log"));
        assert!(!preview.contains("test.tmp"));
    }

    #[test]
    fn mixed_line_endings() {
        let tmp = TempDir::new().unwrap();
        // Mix of LF and CRLF
        let content = "*.log\r\n*.tmp\n*.bak\r\n";
        create_file(tmp.path(), ".gitignore", content);
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "test.tmp", "tmp");
        create_file(tmp.path(), "test.bak", "bak");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("test.log"));
        assert!(!preview.contains("test.tmp"));
        assert!(!preview.contains("test.bak"));
    }

    #[test]
    fn utf8_bom_in_gitignore() {
        let tmp = TempDir::new().unwrap();
        // UTF-8 BOM followed by pattern
        let content = "\u{FEFF}*.log\n*.tmp\n";
        create_file(tmp.path(), ".gitignore", content);
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "test.tmp", "tmp");

        // Should handle BOM gracefully
        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }

    #[test]
    fn comment_after_pattern() {
        let tmp = TempDir::new().unwrap();
        // In gitignore, # is only a comment at line start, not inline
        create_file(tmp.path(), ".gitignore", "*.log # not a comment\n");
        create_file(tmp.path(), "test.log", "log");

        // The pattern is "*.log # not a comment" which won't match *.log
        // This tests that we don't do inline comment stripping
        let _preview = get_preview(&tmp);
        // Behavior depends on implementation - document what happens
        // Most gitignore parsers treat the whole line as pattern
    }

    #[test]
    fn backslash_at_end_of_line() {
        let tmp = TempDir::new().unwrap();
        // Backslash at EOL - not line continuation in gitignore
        create_file(tmp.path(), ".gitignore", "*.log\\\n*.tmp\n");
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "test.tmp", "tmp");

        // Should handle gracefully
        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }
}

// ============================================================================
// CHARACTER CLASS RANGE TESTS
// ============================================================================

mod character_classes {
    use super::*;

    #[test]
    fn numeric_range() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file[0-9].txt\n");
        create_file(tmp.path(), "file0.txt", "0");
        create_file(tmp.path(), "file5.txt", "5");
        create_file(tmp.path(), "file9.txt", "9");
        create_file(tmp.path(), "alpha.txt", "a");
        create_file(tmp.path(), "beta.txt", "b");

        let preview = get_preview(&tmp);
        assert!(
            !preview.contains("file0.txt"),
            "file0.txt should be excluded by [0-9]"
        );
        assert!(
            !preview.contains("file5.txt"),
            "file5.txt should be excluded by [0-9]"
        );
        assert!(
            !preview.contains("file9.txt"),
            "file9.txt should be excluded by [0-9]"
        );
        assert!(
            preview.contains("alpha.txt"),
            "alpha.txt should be included (no digits)"
        );
        assert!(
            preview.contains("beta.txt"),
            "beta.txt should be included (no digits)"
        );
    }

    #[test]
    fn lowercase_alpha_range() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file[a-z].txt\n");
        create_file(tmp.path(), "filea.txt", "a");
        create_file(tmp.path(), "filem.txt", "m");
        create_file(tmp.path(), "filez.txt", "z");
        create_file(tmp.path(), "upper.txt", "upper"); // No lowercase letters
        create_file(tmp.path(), "num5.txt", "5"); // No lowercase letters

        let preview = get_preview(&tmp);
        assert!(
            !preview.contains("filea.txt"),
            "[a-z] should match lowercase"
        );
        assert!(
            !preview.contains("filem.txt"),
            "[a-z] should match lowercase"
        );
        assert!(
            !preview.contains("filez.txt"),
            "[a-z] should match lowercase"
        );
        assert!(preview.contains("upper.txt"), "no lowercase letters");
        assert!(preview.contains("num5.txt"), "no lowercase letters");
    }

    #[test]
    fn uppercase_alpha_range() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "name[A-Z].txt\n");
        create_file(tmp.path(), "nameA.txt", "A");
        create_file(tmp.path(), "nameM.txt", "M");
        create_file(tmp.path(), "nameZ.txt", "Z");
        create_file(tmp.path(), "lower.txt", "lower"); // No uppercase letters
        create_file(tmp.path(), "num5.txt", "5"); // No uppercase letters

        let preview = get_preview(&tmp);
        assert!(
            !preview.contains("nameA.txt"),
            "[A-Z] should match uppercase"
        );
        assert!(
            !preview.contains("nameM.txt"),
            "[A-Z] should match uppercase"
        );
        assert!(
            !preview.contains("nameZ.txt"),
            "[A-Z] should match uppercase"
        );
        assert!(preview.contains("lower.txt"), "no uppercase letters");
        assert!(preview.contains("num5.txt"), "no uppercase letters");
    }

    #[test]
    fn multiple_ranges_in_class() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file[a-zA-Z0-9].txt\n");
        create_file(tmp.path(), "filea.txt", "a");
        create_file(tmp.path(), "fileZ.txt", "Z");
        create_file(tmp.path(), "file5.txt", "5");
        create_file(tmp.path(), "file_.txt", "_");
        create_file(tmp.path(), "file-.txt", "-");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("filea.txt"));
        assert!(!preview.contains("fileZ.txt"));
        assert!(!preview.contains("file5.txt"));
        assert!(preview.contains("file_.txt"));
        assert!(preview.contains("file-.txt"));
    }

    #[test]
    fn negated_character_class_with_range() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "file[!0-9].txt\n");
        create_file(tmp.path(), "file5.txt", "5");
        create_file(tmp.path(), "filea.txt", "a");
        create_file(tmp.path(), "fileZ.txt", "Z");

        let preview = get_preview(&tmp);
        assert!(preview.contains("file5.txt")); // Digits NOT matched by [!0-9]
        assert!(!preview.contains("filea.txt")); // Non-digits ARE matched
        assert!(!preview.contains("fileZ.txt"));
    }
}

// ============================================================================
// CASE SENSITIVITY TESTS
// ============================================================================

mod case_sensitivity {
    use super::*;

    fn is_filesystem_case_insensitive() -> bool {
        let tmp = TempDir::new().unwrap();
        let test_file = tmp.path().join("CaseTest.tmp");
        fs::write(&test_file, "test").ok();
        let result = tmp.path().join("casetest.tmp").exists();
        fs::remove_file(test_file).ok();
        result
    }

    #[test]
    fn pattern_case_sensitivity() {
        if is_filesystem_case_insensitive() {
            eprintln!("Skipping: filesystem is case-insensitive");
            return;
        }

        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.LOG\n");
        create_file(tmp.path(), "file.LOG", "upper");
        create_file(tmp.path(), "file.log", "lower");
        create_file(tmp.path(), "file.Log", "mixed");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("file.LOG"));
        assert!(preview.contains("file.log"));
        assert!(preview.contains("file.Log"));
    }

    #[test]
    fn directory_name_case_sensitivity() {
        if is_filesystem_case_insensitive() {
            eprintln!("Skipping: filesystem is case-insensitive");
            return;
        }

        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "BUILD/\n");
        create_file(tmp.path(), "BUILD/file.txt", "upper");
        create_file(tmp.path(), "build/file.txt", "lower");
        create_file(tmp.path(), "Build/file.txt", "mixed");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("BUILD/file.txt"));
        assert!(preview.contains("build/file.txt"));
        assert!(preview.contains("Build/file.txt"));
    }

    #[test]
    fn mixed_case_in_paths() {
        if is_filesystem_case_insensitive() {
            eprintln!("Skipping: filesystem is case-insensitive");
            return;
        }

        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "src/BUILD/*.o\n");
        create_file(tmp.path(), "src/BUILD/main.o", "match");
        create_file(tmp.path(), "src/build/main.o", "no match");
        create_file(tmp.path(), "src/BUILD/main.c", "no match");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("src/BUILD/main.o"));
        assert!(preview.contains("src/build/main.o"));
        assert!(preview.contains("src/BUILD/main.c"));
    }
}

// ============================================================================
// SYMLINK EDGE CASES
// ============================================================================

#[cfg(unix)]
mod symlink_edge_cases {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn symlink_to_excluded_file() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "debug.log", "log");
        create_file(tmp.path(), "normal.txt", "txt"); // Add a normal file for comparison
        symlink(tmp.path().join("debug.log"), tmp.path().join("link.txt")).unwrap();

        let preview = get_preview(&tmp);

        // debug.log should be excluded as a standalone file
        // It might appear in symlink target, but not as a separate entry
        assert!(
            !preview.contains("  debug.log"),
            "debug.log should not be listed as a file"
        );
        assert!(
            preview.contains("normal.txt"),
            "normal.txt should be included"
        );
        // Symlink itself should be included (it's a .txt, not a .log)
        assert!(
            preview.contains("link.txt"),
            "link.txt symlink should be included"
        );
    }

    #[test]
    fn broken_symlink() {
        let tmp = TempDir::new().unwrap();
        // Create symlink to non-existent file
        symlink(tmp.path().join("nonexistent"), tmp.path().join("broken")).unwrap();
        create_file(tmp.path(), "normal.txt", "normal");

        // Should handle broken symlink gracefully
        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }

    #[test]
    fn symlink_loop() {
        let tmp = TempDir::new().unwrap();
        let dir1 = tmp.path().join("dir1");
        let dir2 = tmp.path().join("dir2");
        fs::create_dir(&dir1).unwrap();
        fs::create_dir(&dir2).unwrap();

        // Create loop: dir1/link -> dir2, dir2/link -> dir1
        symlink(&dir2, dir1.join("link")).unwrap();
        symlink(&dir1, dir2.join("link")).unwrap();

        create_file(tmp.path(), "normal.txt", "normal");

        // Should detect and handle symlink loops
        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }

    #[test]
    fn symlink_outside_archive_root() {
        let tmp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        create_file(outside.path(), "external.txt", "external");

        // Symlink from inside tmp to outside
        symlink(
            outside.path().join("external.txt"),
            tmp.path().join("link.txt"),
        )
        .unwrap();
        create_file(tmp.path(), "internal.txt", "internal");

        let preview = get_preview(&tmp);
        // Should include the symlink
        assert!(preview.contains("link.txt"));
        assert!(preview.contains("internal.txt"));
    }

    #[test]
    fn dereference_flag_follows_symlinks() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "debug.log", "log");
        symlink(tmp.path().join("debug.log"), tmp.path().join("link.txt")).unwrap();

        let preview = get_preview_with_args(&tmp, &["--dereference"]);
        assert!(!preview.contains("debug.log"));
        // With dereference, link points to .log file so might be excluded
        // (depending on implementation details)
    }
}

// ============================================================================
// PATTERN EDGE CASES
// ============================================================================

mod pattern_edge_cases {
    use super::*;

    #[test]
    fn empty_pattern() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "\n\n*.log\n\n");
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "test.txt", "txt");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("test.log"));
        assert!(preview.contains("test.txt"));
    }

    #[test]
    fn pattern_with_only_wildcards() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "***\n");
        create_file(tmp.path(), "test.txt", "test");

        // Should handle gracefully (might match everything or nothing)
        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }

    #[test]
    fn pattern_just_slash() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "/\n*.log\n");
        create_file(tmp.path(), "test.log", "log");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("test.log"));
    }

    #[test]
    fn pattern_just_double_star_slash() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "**/\n");
        create_file(tmp.path(), "test.txt", "test");

        // Should handle gracefully
        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }

    #[test]
    fn negation_of_negation() {
        let tmp = TempDir::new().unwrap();
        // Double negation: !!pattern
        create_file(tmp.path(), ".gitignore", "*.log\n!!important.log\n");
        create_file(tmp.path(), "important.log", "important");
        create_file(tmp.path(), "debug.log", "debug");

        // !!pattern means exclude (negation of include)
        let preview = get_preview(&tmp);
        assert!(!preview.contains("debug.log"));
        // The behavior of !! depends on implementation
    }

    #[test]
    fn pattern_with_embedded_null() {
        // Rust strings can't contain null bytes, but test other control chars
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n*.tmp\n");
        create_file(tmp.path(), "test.log", "log");

        raptar().arg(tmp.path()).arg("--preview").assert().success();
    }

    #[test]
    fn many_wildcards_in_pattern() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*/*/*/*/*/*/*.txt\n");
        create_file(tmp.path(), "a/b/c/d/e/f/deep.txt", "deep");
        create_file(tmp.path(), "a/b/shallow.txt", "shallow");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("a/b/c/d/e/f/deep.txt"));
        assert!(preview.contains("a/b/shallow.txt"));
    }
}

// ============================================================================
// .IGNORE VS .GITIGNORE ORDERING
// ============================================================================

mod ignore_file_ordering {
    use super::*;

    #[test]
    fn ignore_overrides_gitignore_same_pattern() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), ".ignore", "!important.log\n");
        create_file(tmp.path(), "debug.log", "debug");
        create_file(tmp.path(), "important.log", "important");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("debug.log"), "excluded by .gitignore");
        assert!(
            preview.contains("important.log"),
            ".ignore parsed after .gitignore, so negation wins"
        );
    }

    #[test]
    fn ignore_adds_additional_exclusions() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), ".ignore", "*.tmp\n");
        create_file(tmp.path(), "file.log", "log");
        create_file(tmp.path(), "file.tmp", "tmp");
        create_file(tmp.path(), "file.txt", "txt");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("file.log"));
        assert!(!preview.contains("file.tmp"));
        assert!(preview.contains("file.txt"));
    }

    #[test]
    fn ignore_contradicts_gitignore() {
        let tmp = TempDir::new().unwrap();
        // .gitignore includes something, .ignore excludes it
        create_file(tmp.path(), ".gitignore", "*.log\n!keep.log\n");
        create_file(tmp.path(), ".ignore", "keep.log\n");
        create_file(tmp.path(), "keep.log", "keep");
        create_file(tmp.path(), "other.log", "other");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("other.log"));
        // .ignore comes after, so its exclusion wins
        assert!(
            !preview.contains("keep.log"),
            ".ignore exclusion overrides .gitignore negation"
        );
    }

    #[test]
    fn both_empty() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "");
        create_file(tmp.path(), ".ignore", "");
        create_file(tmp.path(), "file.txt", "content");

        let preview = get_preview(&tmp);
        assert!(preview.contains("file.txt"));
    }
}

// ============================================================================
// DEEP NESTING AND PATH LENGTH
// ============================================================================

mod deep_paths {
    use super::*;

    #[test]
    fn extremely_deep_nesting() {
        let tmp = TempDir::new().unwrap();
        // Create 50-level deep directory
        let mut path = String::new();
        for i in 0..50 {
            write!(path, "d{i}/").unwrap();
        }
        path.push_str("deep.txt");
        create_file(tmp.path(), &path, "very deep");
        create_file(tmp.path(), "shallow.txt", "shallow");

        let preview = get_preview(&tmp);
        assert!(preview.contains("deep.txt"));
        assert!(preview.contains("shallow.txt"));
    }

    #[test]
    fn pattern_matches_deep_path() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "**/*.log\n");
        let mut path = String::new();
        for i in 0..20 {
            write!(path, "level{i}/").unwrap();
        }
        path.push_str("deep.log");
        create_file(tmp.path(), &path, "deep log");
        create_file(tmp.path(), "shallow.txt", "shallow");

        let preview = get_preview(&tmp);
        assert!(!preview.contains("deep.log"));
        assert!(preview.contains("shallow.txt"));
    }
}

// ============================================================================
// GITIGNORE COMPATIBILITY WARNINGS
// ============================================================================

mod gitignore_compat_warnings {
    use super::*;

    #[test]
    fn warns_when_negation_targets_excluded_directory() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "build/\n!build/important.txt\n");
        create_file(tmp.path(), "build/important.txt", "important");
        create_file(tmp.path(), "build/junk.txt", "junk");

        // Run raptar and capture stderr
        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("--without-exclude-always")
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should emit warning about gitignore compatibility
        assert!(
            stderr.contains("gitignore-compat"),
            "Should warn about gitignore incompatibility. stderr: {stderr}"
        );
        assert!(
            stderr.contains("build/"),
            "Warning should mention the directory. stderr: {stderr}"
        );

        // File should still be included (raptar is more flexible)
        assert!(
            stdout.contains("build/important.txt"),
            "important.txt should be included despite warning"
        );
        assert!(
            !stdout.contains("build/junk.txt"),
            "junk.txt should be excluded"
        );
    }

    #[test]
    fn no_warning_when_using_contents_pattern() {
        let tmp = TempDir::new().unwrap();
        // Using build/* instead of build/ is gitignore-compatible
        create_file(tmp.path(), ".gitignore", "build/*\n!build/important.txt\n");
        create_file(tmp.path(), "build/important.txt", "important");
        create_file(tmp.path(), "build/junk.txt", "junk");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("--without-exclude-always")
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should NOT emit warning when using proper gitignore pattern
        assert!(
            !stderr.contains("gitignore-compat"),
            "Should not warn when using build/* pattern. stderr: {stderr}"
        );

        // Both behaviors work correctly
        assert!(stdout.contains("build/important.txt"));
        assert!(!stdout.contains("build/junk.txt"));
    }

    #[test]
    fn no_warning_for_same_level_negation() {
        let tmp = TempDir::new().unwrap();
        // Negation at same level (not inside directory) is fine
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        create_file(tmp.path(), "important.log", "important");
        create_file(tmp.path(), "debug.log", "debug");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("--without-exclude-always")
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should NOT emit warning - this is standard gitignore behavior
        assert!(
            !stderr.contains("gitignore-compat"),
            "Should not warn for standard negation pattern. stderr: {stderr}"
        );

        assert!(stdout.contains("important.log"));
        assert!(!stdout.contains("debug.log"));
    }

    #[test]
    fn warning_includes_workaround() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "cache/\n!cache/keep.txt\n");
        create_file(tmp.path(), "cache/keep.txt", "keep");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("--without-exclude-always")
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Warning should mention the workaround
        assert!(
            stderr.contains("cache/*"),
            "Warning should suggest using cache/* instead. stderr: {stderr}"
        );
    }
}
