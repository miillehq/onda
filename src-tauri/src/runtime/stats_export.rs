//! Write the share card at the location chosen in the system save dialog.
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

/// Only the exact path confirmed by the picker may replace an existing file.
pub(super) fn write_png(
    chosen: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let confirmed = match chosen.extension() {
        Some(extension) if extension.eq_ignore_ascii_case("png") => true,
        None => false,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Choose a PNG filename.",
            ))
        }
    };
    let destination = if confirmed {
        chosen.to_owned()
    } else {
        chosen.with_extension("png")
    };
    let directory = destination.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let mut staged = NamedTempFile::new_in(directory)?;
    if confirmed {
        match std::fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.is_file() && !metadata.permissions().readonly() => {
                staged.as_file().set_permissions(metadata.permissions())?;
            }
            Ok(_) => return Err(io::ErrorKind::PermissionDenied.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    write(staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    if confirmed {
        staged.persist(&destination)?;
    } else {
        staged.persist_noclobber(&destination)?;
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};

    #[test]
    fn an_interrupted_export_preserves_the_previous_image() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("card.png");
        fs::write(&destination, b"original").unwrap();
        let interrupted = write_png(&destination, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("simulated interrupted write"))
        });
        assert!(interrupted.is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"original");
        assert_eq!(directory.path().read_dir().unwrap().count(), 1);
        assert_eq!(
            write_png(&destination, |file| file.write_all(b"replacement")).unwrap(),
            destination
        );
        assert_eq!(fs::read(destination).unwrap(), b"replacement");
    }

    #[test]
    fn an_implicit_extension_cannot_replace_an_unconfirmed_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("card.png");
        fs::write(&destination, b"original").unwrap();
        for selected in ["card", "card.db"] {
            assert!(
                write_png(&directory.path().join(selected), |file| file
                    .write_all(b"export"))
                .is_err(),
                "overwrote the existing PNG when the user chose {selected}"
            );
            assert_eq!(fs::read(&destination).unwrap(), b"original");
        }
        let fresh = directory.path().join("new-card");
        assert_eq!(
            write_png(&fresh, |file| file.write_all(b"export")).unwrap(),
            fresh.with_extension("png")
        );
    }
}
