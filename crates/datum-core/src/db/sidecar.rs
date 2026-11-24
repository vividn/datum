//! The files datum keeps beside a database — `.writing`, `.spool`,
//! `.spool.lock`, `.spool.rejected`, `.spool.torn` and the spool's `.tmp` —
//! are opened never through a symbolic link (da-bv58i). Where several users
//! or processes share a directory, a link planted at one steered datum into
//! whatever it named: a write through a link at `.writing` overwrote another
//! database, and a drain through one at `.spool` copied it out into
//! `.spool.rejected`. No setup of datum's puts a link there, so one is
//! refused, naming the file.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

/// Is there a symbolic link at `path`?
pub(crate) fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// The refusal of a link at `path`.
pub(crate) fn linked(path: &Path) -> io::Error {
    io::Error::other(format!(
        "{} is a symbolic link, and datum follows none at the files it keeps beside a \
         database: remove it",
        path.display()
    ))
}

/// Open `path` as `opts` says, refusing a link at it. On unix with
/// `O_NOFOLLOW`, so the refusal is the open's own and a link swapped in after
/// a look is never followed; elsewhere (wasi, where nothing spools) a look
/// first.
pub(crate) fn open(path: &Path, opts: &mut OpenOptions) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    if is_link(path) {
        return Err(linked(path));
    }
    // Only `O_NOFOLLOW`'s own refusal is the link's: ELOOP (EMLINK on the
    // BSDs). Any other failure is said as it is, link or not.
    opts.open(path).map_err(|e| {
        #[cfg(unix)]
        if matches!(e.raw_os_error(), Some(libc::ELOOP | libc::EMLINK)) && is_link(path) {
            return linked(path);
        }
        e
    })
}

/// `path`'s whole content, refusing a link at it.
pub(crate) fn read(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open(path, OpenOptions::new().read(true))?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// `path` created, or emptied, to write, refusing a link at it.
pub(crate) fn create(path: &Path) -> io::Result<File> {
    open(
        path,
        OpenOptions::new().write(true).create(true).truncate(true),
    )
}
