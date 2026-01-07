use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn raptar() -> Command {
    // Build the path to the binary - works with custom build directories
    let cmd = StdCommand::new(env!("CARGO_BIN_EXE_raptar"));
    Command::from_std(cmd)
}

fn create_test_project(dir: &std::path::Path) {
    // Create .git directory so ignore crate respects .gitignore
    fs::create_dir_all(dir.join(".git")).unwrap();

    fs::write(dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(dir.join("lib.rs"), "pub fn hello() {}").unwrap();
    fs::write(dir.join("README.md"), "# Test Project").unwrap();

    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/util.rs"), "pub fn util() {}").unwrap();

    fs::write(dir.join(".gitignore"), "target/\n*.log\n").unwrap();

    fs::create_dir_all(dir.join("target")).unwrap();
    fs::write(dir.join("target/debug"), "binary").unwrap();
    fs::write(dir.join("app.log"), "log content").unwrap();

    fs::write(dir.join(".hidden"), "secret").unwrap();
}

// ============================================================
// CLI argument tests
// ============================================================

#[test]
fn test_cli_help() {
    raptar()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("smart archive tool"))
        .stdout(predicate::str::contains("--preview"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--dereference"))
        .stdout(predicate::str::contains("--preserve-owner"))
        .stdout(predicate::str::contains("--with-exclude"))
        .stdout(predicate::str::contains("--with-include"))
        .stdout(predicate::str::contains("--without-ignorefiles"))
        .stdout(predicate::str::contains("--show-config"))
        .stdout(predicate::str::contains("--init-config"));
}

#[test]
fn test_cli_version() {
    raptar()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("raptar"));
}

#[test]
fn test_cli_nonexistent_path() {
    raptar()
        .arg("/nonexistent/path/that/does/not/exist")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"));
}

// ============================================================
// Preview mode tests
// ============================================================

#[test]
fn test_cli_preview_mode() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .assert()
        .success()
        .stdout(predicate::str::contains("Files to be archived"))
        .stdout(predicate::str::contains("main.rs"))
        .stdout(predicate::str::contains("lib.rs"))
        .stdout(predicate::str::contains("README.md"));
}

#[test]
fn test_cli_preview_excludes_gitignored() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .assert()
        .success()
        .stdout(predicate::str::contains("target").not())
        .stdout(predicate::str::contains("app.log").not());
}

#[test]
fn test_cli_preview_with_size() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("--size")
        .assert()
        .success()
        .stdout(predicate::str::contains("B")); // Size in bytes
}

// ============================================================
// Archive creation tests
// ============================================================

#[test]
fn test_cli_creates_tar_gz() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    let output = tmp.path().join("output.tar.gz");

    raptar()
        .arg(tmp.path())
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    assert!(output.exists());

    // Verify gzip magic bytes
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0], 0x1f);
    assert_eq!(bytes[1], 0x8b);
}

#[test]
fn test_cli_creates_tar() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    let output = tmp.path().join("output.tar");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("tar")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    assert!(output.exists());
}

#[test]
fn test_cli_creates_zip() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    let output = tmp.path().join("output.zip");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("zip")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    assert!(output.exists());

    // Verify zip magic bytes (PK)
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0], 0x50);
    assert_eq!(bytes[1], 0x4b);
}

#[test]
fn test_cli_creates_tar_bz2() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    let output = tmp.path().join("output.tar.bz2");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("tar.bz2")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    assert!(output.exists());

    // Verify bzip2 magic bytes (BZ)
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0], 0x42); // B
    assert_eq!(bytes[1], 0x5a); // Z
}

#[test]
fn test_cli_creates_tar_zst() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    let output = tmp.path().join("output.tar.zst");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("tar.zst")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    assert!(output.exists());

    // Verify zstd magic bytes (0xFD2FB528)
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0], 0x28);
    assert_eq!(bytes[1], 0xb5);
    assert_eq!(bytes[2], 0x2f);
    assert_eq!(bytes[3], 0xfd);
}

#[test]
fn test_cli_tbz2_alias() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("test.txt"), "content").unwrap();

    let output = tmp.path().join("out.tbz2");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("tbz2")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    assert!(output.exists());
}

#[test]
fn test_cli_tzst_alias() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("test.txt"), "content").unwrap();

    let output = tmp.path().join("out.tzst");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("tzst")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    assert!(output.exists());
}

// ============================================================
// Ignore behavior tests
// ============================================================

#[test]
fn test_cli_no_gitignore_includes_ignored() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("--without-ignorefile")
        .arg("gitignore")
        .assert()
        .success()
        .stdout(predicate::str::contains("app.log"));
}

#[test]
fn test_cli_dotfiles_included_by_default() {
    let tmp = TempDir::new().unwrap();
    create_test_project(tmp.path());

    // Dotfiles should be included by default (except .git which is in always_exclude)
    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .assert()
        .success()
        .stdout(predicate::str::contains(".gitignore"));
}

// Note: Adding custom ignore files is done via config file, not CLI flags.
// The CLI only supports --without-ignorefile to disable specific files.

// ============================================================
// Reproducible archive tests
// ============================================================

#[test]
fn test_cli_reproducible_flag() {
    let tmp = TempDir::new().unwrap();
    let src_dir = tmp.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(src_dir.join("a.txt"), "aaa").unwrap();
    fs::write(src_dir.join("b.txt"), "bbb").unwrap();

    let output1 = tmp.path().join("out1.tar");
    let output2 = tmp.path().join("out2.tar");

    // Create first archive
    raptar()
        .arg(&src_dir)
        .arg("-f")
        .arg("tar")
        .arg("-r")
        .arg("-o")
        .arg(&output1)
        .arg("-q")
        .assert()
        .success();

    // Create second archive
    raptar()
        .arg(&src_dir)
        .arg("-f")
        .arg("tar")
        .arg("-r")
        .arg("-o")
        .arg(&output2)
        .arg("-q")
        .assert()
        .success();

    // Should be identical
    let bytes1 = fs::read(&output1).unwrap();
    let bytes2 = fs::read(&output2).unwrap();
    assert_eq!(bytes1, bytes2);
}

// ============================================================
// Verbose/quiet mode tests
// ============================================================

#[test]
fn test_cli_quiet_mode() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("test.txt"), "content").unwrap();

    let output = tmp.path().join("out.tar.gz");

    raptar()
        .arg(tmp.path())
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn test_cli_verbose_mode() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("test.txt"), "content").unwrap();
    fs::write(tmp.path().join("remove.bak"), "bak").unwrap();

    // Verbose mode shows excluded files with reasons
    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("-v")
        .arg("--with-exclude")
        .arg("*.bak")
        .assert()
        .success()
        .stdout(predicate::str::contains("test.txt"))
        .stdout(predicate::str::contains("Files excluded"))
        .stdout(predicate::str::contains("remove.bak"));
}

// ============================================================
// Empty directory test
// ============================================================

#[test]
fn test_cli_empty_directory() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("empty")).unwrap();

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .assert()
        .success()
        .stdout(predicate::str::contains("No files to archive"));
}

// ============================================================
// Format alias test
// ============================================================

#[test]
fn test_cli_tgz_alias() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("test.txt"), "content").unwrap();

    let output = tmp.path().join("out.tgz");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("tgz")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    assert!(output.exists());
}

// ============================================================
// Symlink tests
// ============================================================

#[test]
fn test_cli_symlink_in_archive() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("target.txt"), "content").unwrap();
    symlink("target.txt", tmp.path().join("link.txt")).unwrap();

    let output = tmp.path().join("out.tar");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("tar")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    // Verify symlink is in archive
    let file = fs::File::open(&output).unwrap();
    let mut archive = tar::Archive::new(file);

    let mut found_symlink = false;
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        if entry.path().unwrap().to_string_lossy() == "link.txt" {
            assert_eq!(entry.header().entry_type(), tar::EntryType::Symlink);
            found_symlink = true;
        }
    }
    assert!(found_symlink, "Symlink not found in archive");
}

#[test]
fn test_cli_dereference_follows_symlinks() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("target.txt"), "actual content").unwrap();
    symlink("target.txt", tmp.path().join("link.txt")).unwrap();

    let output = tmp.path().join("out.tar");

    raptar()
        .arg(tmp.path())
        .arg("-f")
        .arg("tar")
        .arg("--dereference")
        .arg("-o")
        .arg(&output)
        .arg("-q")
        .assert()
        .success();

    // Verify symlink was dereferenced (should be regular file)
    let file = fs::File::open(&output).unwrap();
    let mut archive = tar::Archive::new(file);

    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        if entry.path().unwrap().to_string_lossy() == "link.txt" {
            assert_eq!(entry.header().entry_type(), tar::EntryType::Regular);
            return;
        }
    }
    panic!("link.txt not found in archive");
}

// ============================================================
// Dockerignore tests
// ============================================================

#[test]
fn test_cli_dockerignore_not_auto_loaded() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("app.py"), "print('hi')").unwrap();
    fs::write(tmp.path().join(".dockerignore"), "*.py\n").unwrap();

    // .dockerignore is NOT respected by default (only .gitignore and .ignore are)
    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .assert()
        .success()
        .stdout(predicate::str::contains("app.py")); // NOT excluded
}

// Note: testing with additional ignore files requires config file setup
// which is tested in unit tests. CLI only supports disabling ignore files.

#[test]
fn test_cli_show_config() {
    raptar()
        .arg("--show-config")
        .assert()
        .success()
        .stdout(predicate::str::contains("raptar configuration"))
        .stdout(predicate::str::contains("ignore.use"))
        .stdout(predicate::str::contains("always_exclude"))
        .stdout(predicate::str::contains("--with-exclude"));
}

// ============================================================
// Exclude pattern tests
// ============================================================

#[test]
fn test_cli_exclude_pattern() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("keep.txt"), "keep").unwrap();
    fs::write(tmp.path().join("remove.bak"), "remove").unwrap();

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("--with-exclude")
        .arg("*.bak")
        .assert()
        .success()
        .stdout(predicate::str::contains("keep.txt"))
        .stdout(predicate::str::contains("remove.bak").not());
}

#[test]
fn test_cli_exclude_multiple_patterns() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("keep.txt"), "keep").unwrap();
    fs::write(tmp.path().join("remove.bak"), "remove").unwrap();
    fs::write(tmp.path().join("remove.tmp"), "remove").unwrap();

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("--with-exclude")
        .arg("*.bak")
        .arg("--with-exclude")
        .arg("*.tmp")
        .assert()
        .success()
        .stdout(predicate::str::contains("keep.txt"))
        .stdout(predicate::str::contains("remove.bak").not())
        .stdout(predicate::str::contains("remove.tmp").not());
}

#[test]
fn test_cli_exclude_verbose_shows_patterns() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("test.txt"), "test").unwrap();

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("-v")
        .arg("--with-exclude")
        .arg("*.bak")
        .assert()
        .success()
        .stdout(predicate::str::contains("Excluding"))
        .stdout(predicate::str::contains("--with-exclude"))
        .stdout(predicate::str::contains("*.bak"));
}

#[test]
fn test_cli_include_overrides_exclude() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("keep.log"), "keep").unwrap();
    fs::write(tmp.path().join("remove.log"), "remove").unwrap();

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("--with-exclude")
        .arg("*.log")
        .arg("--with-include")
        .arg("keep.log")
        .assert()
        .success()
        .stdout(predicate::str::contains("keep.log"))
        .stdout(predicate::str::contains("remove.log").not());
}

#[test]
fn test_cli_verbose_shows_excluded_files() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("keep.txt"), "keep").unwrap();
    fs::write(tmp.path().join("remove.bak"), "remove").unwrap();

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .arg("-v")
        .arg("--with-exclude")
        .arg("*.bak")
        .assert()
        .success()
        .stdout(predicate::str::contains("Files excluded"))
        .stdout(predicate::str::contains("remove.bak"))
        .stdout(predicate::str::contains("--with-exclude"));
}

// ============================================================
// Subdirectory inclusion tests (regression test for sibling exclusion bug)
// ============================================================

#[test]
fn test_cli_subdirectories_included_with_git() {
    let tmp = TempDir::new().unwrap();

    // Create a project structure similar to a real repo
    fs::create_dir_all(tmp.path().join(".git/objects")).unwrap();
    fs::write(tmp.path().join(".git/config"), "[core]").unwrap();
    fs::write(tmp.path().join(".git/HEAD"), "ref: refs/heads/main").unwrap();

    fs::create_dir_all(tmp.path().join("src/nested")).unwrap();
    fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(tmp.path().join("src/nested/util.rs"), "pub fn util() {}").unwrap();

    fs::create_dir_all(tmp.path().join("tests")).unwrap();
    fs::write(tmp.path().join("tests/test.rs"), "#[test] fn t() {}").unwrap();

    fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
    fs::write(tmp.path().join("README.md"), "# Test").unwrap();

    // Run raptar - should include src/ and tests/ but exclude .git/
    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .assert()
        .success()
        .stdout(predicate::str::contains("src/main.rs"))
        .stdout(predicate::str::contains("src/nested/util.rs"))
        .stdout(predicate::str::contains("tests/test.rs"))
        .stdout(predicate::str::contains("Cargo.toml"))
        .stdout(predicate::str::contains("README.md"))
        .stdout(predicate::str::contains(".git").not());
}

#[test]
fn test_cli_deep_subdirectories() {
    let tmp = TempDir::new().unwrap();

    // Create deeply nested structure
    fs::create_dir_all(tmp.path().join("a/b/c/d/e")).unwrap();
    fs::write(tmp.path().join("a/file1.txt"), "1").unwrap();
    fs::write(tmp.path().join("a/b/file2.txt"), "2").unwrap();
    fs::write(tmp.path().join("a/b/c/file3.txt"), "3").unwrap();
    fs::write(tmp.path().join("a/b/c/d/file4.txt"), "4").unwrap();
    fs::write(tmp.path().join("a/b/c/d/e/file5.txt"), "5").unwrap();

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .assert()
        .success()
        .stdout(predicate::str::contains("a/file1.txt"))
        .stdout(predicate::str::contains("a/b/file2.txt"))
        .stdout(predicate::str::contains("a/b/c/file3.txt"))
        .stdout(predicate::str::contains("a/b/c/d/file4.txt"))
        .stdout(predicate::str::contains("a/b/c/d/e/file5.txt"));
}

#[test]
fn test_cli_multiple_sibling_directories() {
    let tmp = TempDir::new().unwrap();

    // Create multiple sibling directories - none should be excluded by .git rules
    fs::create_dir_all(tmp.path().join(".git")).unwrap();
    fs::write(tmp.path().join(".git/config"), "config").unwrap();

    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::create_dir_all(tmp.path().join("tests")).unwrap();
    fs::create_dir_all(tmp.path().join("docs")).unwrap();
    fs::create_dir_all(tmp.path().join("examples")).unwrap();

    fs::write(tmp.path().join("src/lib.rs"), "lib").unwrap();
    fs::write(tmp.path().join("tests/test.rs"), "test").unwrap();
    fs::write(tmp.path().join("docs/README.md"), "docs").unwrap();
    fs::write(tmp.path().join("examples/ex.rs"), "ex").unwrap();

    raptar()
        .arg(tmp.path())
        .arg("--preview")
        .assert()
        .success()
        .stdout(predicate::str::contains("src/lib.rs"))
        .stdout(predicate::str::contains("tests/test.rs"))
        .stdout(predicate::str::contains("docs/README.md"))
        .stdout(predicate::str::contains("examples/ex.rs"))
        .stdout(predicate::str::contains(".git").not());
}
