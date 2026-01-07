//! Archive creation for various formats.

use crate::walk::{EntryType, FileEntry};

use anyhow::Result;
use bzip2::write::BzEncoder;
use flate2::write::GzEncoder;
use flate2::Compression as GzCompression;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::{self, Write};
use time::OffsetDateTime;
use zstd::stream::write::Encoder as ZstdEncoder;

/// Creates a progress bar for archive operations.
pub fn create_progress_bar(len: u64, quiet: bool, verbose: bool) -> Option<ProgressBar> {
    if !quiet && !verbose {
        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} files")
                .unwrap()
                .progress_chars("━━─"),
        );
        Some(pb)
    } else {
        None
    }
}

/// Sets tar header metadata from a file entry.
pub fn set_header_metadata(
    header: &mut tar::Header,
    entry: &FileEntry,
    reproducible: bool,
    preserve_owner: bool,
) {
    if reproducible {
        // Reproducible mode: normalize everything
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
    } else {
        header.set_mtime(entry.mtime);
        if preserve_owner {
            header.set_uid(u64::from(entry.uid));
            header.set_gid(u64::from(entry.gid));
        } else {
            header.set_uid(0);
            header.set_gid(0);
        }
    }
    header.set_mode(entry.mode);
}

/// Creates a tar archive from the given entries.
#[allow(clippy::fn_params_excessive_bools)]
pub fn create_tar<W: Write>(
    writer: W,
    entries: &[FileEntry],
    reproducible: bool,
    preserve_owner: bool,
    quiet: bool,
    verbose: bool,
) -> Result<()> {
    let mut builder = tar::Builder::new(writer);
    let progress = create_progress_bar(entries.len() as u64, quiet, verbose);

    for entry in entries {
        match entry.entry_type {
            EntryType::Symlink => {
                if let Some(ref target) = entry.link_target {
                    let mut header = tar::Header::new_gnu();
                    header.set_entry_type(tar::EntryType::Symlink);
                    set_header_metadata(&mut header, entry, reproducible, preserve_owner);
                    header.set_size(0);
                    builder.append_link(&mut header, &entry.relative_path, target)?;
                }
            }
            EntryType::File => {
                let file = File::open(&entry.path)?;
                let mut header = tar::Header::new_gnu();
                header.set_size(entry.size);
                set_header_metadata(&mut header, entry, reproducible, preserve_owner);
                builder.append_data(&mut header, &entry.relative_path, file)?;
            }
            EntryType::Directory => {
                // Skip directories for now
            }
        }

        if let Some(ref pb) = progress {
            pb.inc(1);
        }
    }

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    builder.finish()?;
    Ok(())
}

/// Creates a zip archive from the given entries.
#[allow(clippy::fn_params_excessive_bools)]
pub fn create_zip<W: Write + io::Seek>(
    writer: W,
    entries: &[FileEntry],
    reproducible: bool,
    quiet: bool,
    verbose: bool,
) -> Result<()> {
    use zip::write::SimpleFileOptions;
    use zip::CompressionMethod;

    let mut zip = zip::ZipWriter::new(writer);
    let progress = create_progress_bar(entries.len() as u64, quiet, verbose);

    for entry in entries {
        let path_str = entry.relative_path.to_string_lossy();

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let last_modified = if reproducible {
            zip::DateTime::default()
        } else {
            // Convert Unix timestamp to zip DateTime components
            #[allow(clippy::cast_possible_wrap)]
            let dt = OffsetDateTime::from_unix_timestamp(entry.mtime as i64)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            #[allow(clippy::cast_sign_loss)]
            zip::DateTime::from_date_and_time(
                dt.year() as u16,
                dt.month() as u8,
                dt.day(),
                dt.hour(),
                dt.minute(),
                dt.second(),
            )
            .unwrap_or_default()
        };

        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(last_modified);

        match entry.entry_type {
            EntryType::Symlink => {
                if let Some(ref target) = entry.link_target {
                    let options = options.unix_permissions(0o777);
                    // Store symlink target as file content (zip convention for some tools)
                    zip.start_file(&*path_str, options)?;
                    zip.write_all(target.to_string_lossy().as_bytes())?;
                }
            }
            EntryType::File => {
                let options = options.unix_permissions(entry.mode & 0o7777);
                zip.start_file(&*path_str, options)?;
                let mut file = File::open(&entry.path)?;
                io::copy(&mut file, &mut zip)?;
            }
            EntryType::Directory => {
                // Skip directories
            }
        }

        if let Some(ref pb) = progress {
            pb.inc(1);
        }
    }

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    zip.finish()?;
    Ok(())
}

/// Creates a gzip-compressed tar archive.
#[allow(clippy::fn_params_excessive_bools)]
pub fn create_tar_gz<W: Write>(
    writer: W,
    entries: &[FileEntry],
    reproducible: bool,
    preserve_owner: bool,
    quiet: bool,
    verbose: bool,
) -> Result<()> {
    let encoder = GzEncoder::new(writer, GzCompression::default());
    create_tar(
        encoder,
        entries,
        reproducible,
        preserve_owner,
        quiet,
        verbose,
    )
}

/// Creates a bzip2-compressed tar archive.
#[allow(clippy::fn_params_excessive_bools)]
pub fn create_tar_bz2<W: Write>(
    writer: W,
    entries: &[FileEntry],
    reproducible: bool,
    preserve_owner: bool,
    quiet: bool,
    verbose: bool,
) -> Result<()> {
    let encoder = BzEncoder::new(writer, bzip2::Compression::best());
    create_tar(
        encoder,
        entries,
        reproducible,
        preserve_owner,
        quiet,
        verbose,
    )
}

/// Creates a zstd-compressed tar archive.
#[allow(clippy::fn_params_excessive_bools)]
pub fn create_tar_zst<W: Write>(
    writer: W,
    entries: &[FileEntry],
    reproducible: bool,
    preserve_owner: bool,
    quiet: bool,
    verbose: bool,
) -> Result<()> {
    let encoder = ZstdEncoder::new(writer, 3)?.auto_finish();
    create_tar(
        encoder,
        entries,
        reproducible,
        preserve_owner,
        quiet,
        verbose,
    )
}
