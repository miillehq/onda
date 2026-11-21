use std::{fs::File, io, path::Path};

/// Replace a private file only after its complete contents reach disk.
pub(super) fn replace(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let directory = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    write(temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};

    #[test]
    fn an_interrupted_permission_save_preserves_the_previous_private_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("typing-permission");
        replace(&path, |file| file.write_all(b"previous")).unwrap();
        assert!(replace(&path, |file| {
            file.write_all(b"partial")?;
            Err(io::ErrorKind::Interrupted.into())
        })
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"previous");
        replace(&path, |file| file.write_all(b"replacement")).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
