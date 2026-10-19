//! Fixture qualification only, NOT a stable creator handle or product witness.
//! A numeric task path is useful here only after held-task visibility has been
//! established, with the same queryable parent bracketing every later lookup.
use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub(super) struct TaskVisibility {
    parent: PathBuf,
    target: PathBuf,
    parent_identity: (u64, u64),
}

fn directory(path: &Path) -> io::Result<Metadata> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "task path is not a directory",
        ));
    }
    Ok(metadata)
}

impl TaskVisibility {
    pub(super) fn before_exit(parent: &Path, tid: i32) -> io::Result<Self> {
        let metadata = directory(parent)?;
        let this = Self {
            parent: parent.to_owned(),
            target: parent.join(tid.to_string()),
            parent_identity: (metadata.dev(), metadata.ino()),
        };
        directory(&this.target)?; // Target MUST actually be visible before exit.
        this.check_parent()?;
        Ok(this)
    }

    fn check_parent(&self) -> io::Result<()> {
        let metadata = directory(&self.parent)?;
        if (metadata.dev(), metadata.ino()) != self.parent_identity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "task parent changed",
            ));
        }
        Ok(())
    }

    pub(super) fn exists(&self) -> io::Result<bool> {
        self.check_parent()?;
        let lookup = directory(&self.target);
        // A missing/unavailable/replaced parent may never stand in for target
        // absence. Check even when target lookup reports ENOENT or another error.
        self.check_parent()?;
        match lookup {
            Ok(_) => Ok(true),
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(false),
            Err(error) => Err(error),
        }
    }
}
