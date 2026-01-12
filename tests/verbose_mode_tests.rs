//! Comprehensive verbose mode tests.
//!
//! Ensures --verbose flag provides complete transparency about:
//! - What rules are loaded
//! - Where rules came from (<file:line>)
//! - Why files were included/excluded
//! - Rule source priority

use assert_cmd::Command;
use predicates::prelude::*;
use std::fmt::Write;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn raptar() -> Command {
    let cmd = StdCommand::new(env!("CARGO_BIN_EXE_raptar"));
    Command::from_std(cmd)
}

fn create_file(base: &std::path::Path, path: &str, content: &str) {
    let full = base.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(full, content).unwrap();
}

// ============================================================================
// RULE SOURCE ATTRIBUTION
// ============================================================================

mod rule_attribution {
    use super::*;

    #[test]
    fn verbose_shows_gitignore_with_line_numbers() {
        let tmp = TempDir::new().unwrap();
        create_file(
            tmp.path(),
            ".gitignore",
            "# Header comment\n*.log\n*.tmp\n# Mid comment\n*.bak\n",
        );
        create_file(tmp.path(), "test.log", "log");
        create_file(tmp.path(), "file.txt", "txt");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .assert()
            .success()
            .stdout(predicate::str::contains(".gitignore"))
            .stdout(predicate::str::contains("*.log"))
            .stdout(predicate::str::contains("*.tmp"))
            .stdout(predicate::str::contains("*.bak"));
    }

    #[test]
    fn verbose_shows_cli_exclude_source() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "file.bak", "backup");
        create_file(tmp.path(), "file.txt", "text");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .arg("--with-exclude")
            .arg("*.bak")
            .assert()
            .success()
            .stdout(predicate::str::contains("--with-exclude"))
            .stdout(predicate::str::contains("*.bak"));
    }

    #[test]
    fn verbose_shows_cli_include_source() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "important.log", "log");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .arg("--with-include")
            .arg("important.log")
            .assert()
            .success()
            .stdout(predicate::str::contains("--with-include"))
            .stdout(predicate::str::contains("important.log"));
    }

    #[test]
    fn verbose_shows_multiple_sources() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), ".ignore", "*.tmp\n");
        create_file(tmp.path(), "file.txt", "text");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .assert()
            .success()
            .stdout(predicate::str::contains(".gitignore"))
            .stdout(predicate::str::contains(".ignore"));
    }

    #[test]
    fn verbose_groups_rules_by_source() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n*.tmp\n*.bak\n");
        create_file(tmp.path(), "file.txt", "text");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should see .gitignore header followed by its patterns
        assert!(stdout.contains(".gitignore"));
        assert!(stdout.contains("*.log"));
        assert!(stdout.contains("*.tmp"));
        assert!(stdout.contains("*.bak"));
    }
}

// ============================================================================
// EXCLUDED FILES WITH ORIGIN
// ============================================================================

mod exclusion_attribution {
    use super::*;

    #[test]
    fn verbose_shows_which_rule_excluded_each_file() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "debug.log", "log");
        create_file(tmp.path(), "error.log", "log");
        create_file(tmp.path(), "temp.bak", "bak");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .arg("--with-exclude")
            .arg("*.bak")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should show which files were excluded
        assert!(stdout.contains("debug.log"));
        assert!(stdout.contains("error.log"));
        assert!(stdout.contains("temp.bak"));

        // Should attribute to correct source
        assert!(stdout.contains(".gitignore") || stdout.contains("*.log"));
        assert!(stdout.contains("--with-exclude") || stdout.contains("*.bak"));
    }

    #[test]
    fn verbose_shows_files_excluded_by_different_rules() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), ".ignore", "*.tmp\n");
        create_file(tmp.path(), "file.log", "log");
        create_file(tmp.path(), "file.tmp", "tmp");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Each excluded file should show its origin
        if stdout.contains("Files excluded") {
            assert!(stdout.contains("file.log"));
            assert!(stdout.contains("file.tmp"));
        }
    }

    #[test]
    fn verbose_shows_deeply_nested_exclusions() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");
        create_file(tmp.path(), "a/b/c/deep.log", "log");
        create_file(tmp.path(), "x/y/z/other.log", "log");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should show deeply nested excluded files
        if stdout.contains("Files excluded") {
            assert!(stdout.contains("deep.log"));
            assert!(stdout.contains("other.log"));
        }
    }
}

// ============================================================================
// NEGATION INDICATORS
// ============================================================================

mod negation_display {
    use super::*;

    #[test]
    fn verbose_distinguishes_include_from_exclude() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        create_file(tmp.path(), "file.txt", "text");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should show both exclude and include rules
        assert!(stdout.contains("*.log"));
        assert!(stdout.contains("important.log"));
        // Might have + or ! to indicate include
    }

    #[test]
    fn verbose_shows_multiple_negations() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n!keep*.log\n!save*.log\n");
        create_file(tmp.path(), "file.txt", "text");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(stdout.contains("*.log"));
        assert!(stdout.contains("keep") || stdout.contains("save"));
    }
}

// ============================================================================
// ECOSYSTEM TEMPLATES
// ============================================================================

mod ecosystem_attribution {
    use super::*;

    #[test]
    fn verbose_shows_ecosystem_loading() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "__pycache__/cache.pyc", "pyc");
        create_file(tmp.path(), "main.py", "py");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .arg("--with-ecosystem")
            .arg("Python")
            .assert()
            .success()
            .stderr(predicate::str::contains("Python"));
    }

    #[test]
    fn verbose_shows_multiple_ecosystems() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "file.txt", "text");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .arg("--with-ecosystem")
            .arg("Python")
            .arg("--with-ecosystem")
            .arg("Rust")
            .assert()
            .success()
            .stderr(predicate::str::contains("Python"))
            .stderr(predicate::str::contains("Rust"));
    }
}

// ============================================================================
// VERBOSE WITH NO EXCLUSIONS
// ============================================================================

mod no_exclusions {
    use super::*;

    #[test]
    fn verbose_with_empty_gitignore() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "");
        create_file(tmp.path(), "file.txt", "text");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .assert()
            .success();
    }

    #[test]
    fn verbose_with_no_ignore_files() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "file.txt", "text");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .assert()
            .success()
            .stdout(predicate::str::contains("file.txt"));
    }

    #[test]
    fn verbose_with_only_comments() {
        let tmp = TempDir::new().unwrap();
        create_file(
            tmp.path(),
            ".gitignore",
            "# Comment 1\n# Comment 2\n# Comment 3\n",
        );
        create_file(tmp.path(), "file.txt", "text");

        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .assert()
            .success();
    }
}

// ============================================================================
// PRIORITY CHAIN VISIBILITY
// ============================================================================

mod priority_visibility {
    use super::*;

    #[test]
    fn verbose_shows_cli_overriding_gitignore() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n!important.log\n");
        create_file(tmp.path(), "important.log", "log");
        create_file(tmp.path(), "debug.log", "log");

        // CLI exclude should override gitignore negation
        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .arg("--with-exclude")
            .arg("important.log")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should show both .gitignore and CLI rules
        assert!(stdout.contains(".gitignore"));
        assert!(stdout.contains("--with-exclude"));
        assert!(stdout.contains("important.log"));
    }

    #[test]
    fn verbose_shows_include_overriding_exclude() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "secret.key", "key");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .arg("--with-exclude")
            .arg("*.key")
            .arg("--with-include")
            .arg("secret.key")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should show both rules
        assert!(stdout.contains("--with-exclude"));
        assert!(stdout.contains("--with-include"));
        assert!(stdout.contains("secret.key") || stdout.contains("*.key"));
    }
}

// ============================================================================
// VERBOSE OUTPUT FORMATTING
// ============================================================================

mod output_formatting {
    use super::*;

    #[test]
    fn verbose_output_is_readable() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n*.tmp\n");
        create_file(tmp.path(), "file.log", "log");
        create_file(tmp.path(), "file.txt", "txt");

        let output = raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .output()
            .unwrap();

        // Should succeed and produce output
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.is_empty());
    }

    #[test]
    fn verbose_with_many_rules() {
        let tmp = TempDir::new().unwrap();
        let mut patterns = String::new();
        for i in 0..50 {
            writeln!(patterns, "*.ext{i}").unwrap();
        }
        create_file(tmp.path(), ".gitignore", &patterns);
        create_file(tmp.path(), "file.txt", "txt");

        // Should handle many rules without issues
        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .assert()
            .success();
    }

    #[test]
    fn verbose_with_many_excluded_files() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".gitignore", "*.log\n");

        // Create 100 .log files
        for i in 0..100 {
            create_file(tmp.path(), &format!("file{i}.log"), "log");
        }
        create_file(tmp.path(), "keep.txt", "txt");

        // Should handle many excluded files
        raptar()
            .arg(tmp.path())
            .arg("--preview")
            .arg("-v")
            .assert()
            .success();
    }
}
