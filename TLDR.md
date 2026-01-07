# raptar

> Smart tar wrapper that respects .gitignore 🦖

## Basic Usage

```bash
# Archive current directory (defaults to tar.gz)
raptar

# Preview what would be included
raptar --preview

# Preview with file sizes
raptar --preview --size

# Create archive with specific name
raptar -o my-project.tar.gz
```

## Output Formats

```bash
raptar -f tar        # Plain tar
raptar -f tar.gz     # Gzip (default)
raptar -f tar.bz2    # Bzip2
raptar -f tar.zst    # Zstandard (fast + good ratio)
raptar -f zip        # Zip
```

## Include/Exclude

```bash
# Exclude patterns (gitignore syntax)
raptar --with-exclude '*.log' --with-exclude 'temp/'

# Force include (overrides exclusions)
raptar --with-exclude '*.log' --with-include 'important.log'

# Use additional ignore file
raptar --with-ignorefile .dockerignore

# Disable all ignore files
raptar --without-ignorefiles
```

## Ecosystem Templates

```bash
# List available ecosystems
raptar --list-ecosystems

# Use an ecosystem template (lowest priority)
raptar --with-ecosystem Python

# Use multiple templates
raptar --with-ecosystem Rust --with-ecosystem Node
```

## Reproducible Builds

```bash
# Zero timestamps, deterministic ordering
raptar -r -o release.tar.gz
```

## Configuration

```bash
# Edit config file
raptar --edit-config

# Show current config
raptar --show-config
```

Config location: `~/.config/raptar/config.toml`

## Verbose Mode

```bash
# See all rules and exclusion reasons
raptar -v --preview
```

## Precedence (lowest to highest)

1. `--with-ecosystem` templates
2. `.gitignore` / `.ignore` (root only)
3. Config `use` files
4. `--with-ignorefile`
5. Config `always_exclude`
6. Config `always_include`
7. `--with-exclude`
8. `--with-include` (always wins)

Last match wins within each level.
