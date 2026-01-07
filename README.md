# 🦖 raptar

A smart tar wrapper that respects `.gitignore` and friends.

## Features

- **Respects `.gitignore` and `.ignore`** by default
- **Ecosystem templates** - Apply standard ignores for Rust, Python, Node, and 30+ other ecosystems
- **Opt-in support for any ignore file** - `.dockerignore`, `.npmignore`, or your own
- **Configuration file** - Persist preferences at `~/.config/raptar/config.toml`
- **Multiple formats** - `tar`, `tar.gz`, `tar.bz2`, `tar.zst`, `zip`
- **Reproducible builds** - Deterministic ordering and zero timestamps
- **Symlink handling** - Preserves symlinks or dereferences them
- **Permission preservation** - Maintains file modes and optionally uid/gid
- **Preview mode** - See what would be included before archiving

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
./target/release/raptar --help
```

## Quick Start

```bash
# Archive current directory (defaults to tar.gz)
raptar

# Preview what would be included
raptar --preview --size

# Create a reproducible archive
raptar -r -o my-project-v1.0.0.tar.gz

# Use zstd compression (best speed/ratio)
raptar -f tar.zst
```

## Ignore File Handling

By default, raptar respects:
- `.gitignore` - Git version control
- `.ignore` - Generic ignore (ripgrep convention)

Additional ignore files can be enabled in the config file under `[ignore].use`. Any file using gitignore syntax works.

**Note:** Only ignore files at the project root are processed automatically. Nested ignore files (e.g., `src/.gitignore`) are rare in practice and not auto-discovered—raptar will warn you if it finds them. If you need to use nested ignore files, add them explicitly:

```bash
# Explicitly use a nested ignore file
raptar --with-ignorefile src/.gitignore
```

**Adding ignore files:**

```bash
# Use a custom ignore file
raptar --with-ignorefile .dockerignore

# Use multiple custom ignore files
raptar --with-ignorefile .dockerignore --with-ignorefile .npmignore
```

**Disabling ignore files:**

```bash
# Disable a specific ignore file
raptar --without-ignorefile gitignore

# Disable all ignore files
raptar --without-ignorefiles
```

**Verbose mode:** Use `-v` to see which rules are applied and where exclusions come from.

## Ecosystem Templates

raptar includes standard gitignore rules from [GitHub's gitignore templates](https://github.com/github/gitignore) for common project types. Templates are baked into the binary—no network required.

```bash
# List available ecosystems (shows download date)
raptar --list-ecosystems

# Use a template
raptar --with-ecosystem Python

# Use multiple templates
raptar --with-ecosystem Rust --with-ecosystem Node
```

**35 ecosystems included:** Rust, Python, Node, Go, Ruby, Java, Swift, Kotlin, Haskell, and more. Run `--list-ecosystems` for the full list.

**Priority:** Ecosystem templates have the lowest priority. Your `.gitignore`, config settings, and CLI options always override them.

**Updating templates:** Run `just fetch-ecosystems` to download fresh templates from GitHub, then rebuild.

## Always Exclude

Use `--with-exclude` to exclude files regardless of what ignore files say:

```bash
# Exclude all .bak files
raptar --with-exclude '*.bak'

# Exclude multiple patterns
raptar --with-exclude '*.bak' --with-exclude '*.tmp' --with-exclude 'node_modules/**'

# Exclude a directory and all its contents
raptar --with-exclude '.git/**' --with-exclude 'dist/**'
```

These patterns use gitignore syntax. For directories, use `**` to match all contents.

## Force Include

Use `--with-include` to force include files, overriding any exclusion:

```bash
# Exclude all logs except important.log
raptar --with-exclude '*.log' --with-include 'important.log'

# Override config exclusions for a specific file
raptar --with-include 'dist/release.tar.gz'
```

You can also set permanent exclusions/inclusions in the config file under `[ignore]`.

## Configuration File

Persist your preferences so you don't have to specify them every time:

```bash
# Edit config (creates if missing, opens in $EDITOR)
raptar --edit-config

# Show current configuration
raptar --show-config
```

Config location: `~/.config/raptar/config.toml`

```toml
[ignore]
# Honor these ignore files by default (any gitignore-format file)
use = [".dockerignore", ".npmignore"]

# Patterns to ALWAYS exclude, regardless of other ignore files
# Uses gitignore syntax (use ** for directories)
always_exclude = [
    ".git/**",
    ".idea/**",
    ".vscode/**",
    ".DS_Store",
    "*.swp",
]

# Force include (overrides always_exclude patterns)
# always_include = ["important.log", "dist/release.tar.gz"]

[defaults]
# Default output format
format = "tar.zst"

# Always create reproducible archives  
reproducible = true
```

## Options

```
-o, --output <FILE>           Output file (auto-generated if not specified)
-f, --format <FORMAT>         Output format [default: tar.gz]
                              Formats: tar, tar.gz, tar.bz2, tar.zst, zip
                              Aliases: tgz, tbz2, tzst
-p, --preview                 Preview mode - show files without creating archive
-s, --size                    Show size estimation
    --with-exclude <PATTERN>  Add exclude pattern (can be repeated, gitignore syntax)
    --with-include <PATTERN>  Add include pattern, overrides exclusions (can be repeated)
    --without-exclude-always  Disable config always_exclude patterns
    --without-include-always  Disable config always_include patterns
    --with-ignorefile <FILE>  Add ignore file to use (can be repeated)
    --without-ignorefiles     Disable all ignore files (.gitignore, .ignore, etc.)
    --without-ignorefile <F>  Disable specific ignore file (can be repeated)
    --with-ecosystem <NAME>   Use ecosystem template (can be repeated)
    --list-ecosystems         List available ecosystem templates
    --dereference             Follow symlinks instead of archiving them as links
    --preserve-owner          Preserve file ownership (uid/gid)
-r, --reproducible            Deterministic ordering and zero timestamps
-q, --quiet                   Minimal output
-v, --verbose                 Show rules and exclusion reasons
    --show-config             Show config file location and current settings
    --init-config             Initialize config file with defaults
    --edit-config             Open config file in $EDITOR (creates if missing)
```

## Precedence

**Last match wins.** Rules are applied in order, and the last matching rule determines whether a file is included or excluded. This follows standard gitignore semantics.

**Sources** (in order of precedence, lowest to highest):

| Priority | Source | Description |
|----------|--------|-------------|
| 1 (lowest) | `--with-ecosystem` | Ecosystem templates from GitHub |
| 2 | `.gitignore`, `.ignore` | Root-level ignore files |
| 3 | Config `use` files | Additional ignore files from config |
| 4 | `--with-ignorefile` | CLI-specified ignore files |
| 5 | Config `always_exclude` | Patterns that always exclude |
| 6 | Config `always_include` | Patterns that always include |
| 7 | `--with-exclude` | CLI exclude patterns |
| 8 (highest) | `--with-include` | CLI include patterns (always wins) |

**Within each source**, rules are applied in file order. Later rules override earlier ones:
- `*.log` followed by `!important.log` → includes `important.log`
- A pattern in `.gitignore` can be overridden by `--with-include`
- An ecosystem template exclusion can be overridden by `.gitignore`

**Examples:**

```bash
# Ecosystem excludes *.pyc, but .gitignore has !important.pyc
# Result: important.pyc is INCLUDED (.gitignore wins over ecosystem)

# .gitignore excludes *.log, CLI has --with-include debug.log  
# Result: debug.log is INCLUDED (CLI wins over .gitignore)

# Config always_exclude has dist/**, CLI has --with-include dist/release.tar.gz
# Result: dist/release.tar.gz is INCLUDED (--with-include wins over config)
```

**Verbose mode** (`-v`) shows all rules and which rule caused each exclusion:
```
Files excluded:
  debug.log (.gitignore:5)
  temp.bak (--with-exclude)
```

## Compression Comparison

Example compression ratios (your mileage may vary):

| Format   | Size  | Notes |
|----------|-------|-------|
| tar      | 117K  | No compression |
| tar.gz   | 26K   | Good balance, widely supported |
| tar.bz2  | 23K   | Best compression, slower |
| tar.zst  | 27K   | Fast compression/decompression |
| zip      | 28K   | Cross-platform compatible |

## Why raptar?

Unlike `git archive`:
- Works with uncommitted changes
- Supports multiple archive formats
- Offers preview and size estimation
- Explicit control over ignore files
- Creates reproducible archives
- Handles symlinks properly

Unlike plain `tar`:
- Automatically respects `.gitignore`
- Has a friendly CLI with progress feedback
- Supports reproducible builds out of the box
- Config file for persistent preferences

## Development

raptar uses `just` for task automation:

```bash
cargo install just

just              # Show all commands
just check        # fmt + clippy + test
just ci           # Full CI check
just compare-formats  # Compare compression ratios
```

## Name

Like Reptar the dinosaur from Rugrats, but **raptar** because it wraps tar. 🦖

## License

Apache-2.0
