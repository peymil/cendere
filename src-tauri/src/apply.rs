use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Replaces `target` with `bytes` via a temp file in the same directory, fsync, then rename.
pub fn atomic_replace(target: &Path, bytes: &[u8]) -> Result<()> {
    let dir = target.parent().context("Target has no parent directory")?;
    let name = target
        .file_name()
        .context("Target has no file name")?
        .to_string_lossy()
        .into_owned();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{}.{}.{}.tmp", name, std::process::id(), nonce));
    let permissions = fs::metadata(target)?.permissions();

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::set_permissions(&tmp, permissions)?;
        fs::rename(&tmp, target)?;
        #[cfg(unix)]
        {
            if let Ok(d) = fs::File::open(dir) {
                let _ = d.sync_all();
            }
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_replace_overwrites_and_keeps_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("img.png");
        fs::write(&target, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        }
        atomic_replace(&target, b"new content").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new content");
        let leftovers = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 1, "temp file must be renamed away");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o640);
        }
    }

    #[test]
    fn sha256_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
