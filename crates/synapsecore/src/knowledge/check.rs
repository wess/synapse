use super::{io::Folder, paths};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;

pub(super) const SOURCELIMIT: usize = 2 * 1024 * 1024;
const SCANLIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub(super) enum Checked {
    Hash(String),
    Missing,
    Unchecked,
}

pub(super) struct Check {
    hashes: BTreeMap<String, Checked>,
    pub bytes: usize,
}

impl Check {
    pub fn new() -> Self {
        Self {
            hashes: BTreeMap::new(),
            bytes: 0,
        }
    }

    pub fn source(&mut self, root: &Folder, path: &str) -> Checked {
        if let Some(checked) = self.hashes.get(path) {
            return checked.clone();
        }
        let checked = self.hash(root, path).unwrap_or(Checked::Missing);
        self.hashes.insert(path.to_owned(), checked.clone());
        checked
    }

    fn hash(&mut self, root: &Folder, path: &str) -> Result<Checked> {
        paths::validsource(path)?;
        let mut file = root.source(path)?;
        let length = file.metadata()?.len();
        ensure!(length <= SOURCELIMIT as u64, "source exceeds size limit");
        if self.bytes + length as usize > SCANLIMIT {
            return Ok(Checked::Unchecked);
        }
        let before = file.metadata()?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut read = 0;
        loop {
            let remaining = (SCANLIMIT - self.bytes).min(SOURCELIMIT + 1 - read);
            if remaining == 0 {
                if read as u64 == length {
                    break;
                }
                return Ok(Checked::Unchecked);
            }
            let amount = remaining.min(buffer.len());
            let size = file.read(&mut buffer[..amount])?;
            if size == 0 {
                break;
            }
            read += size;
            self.bytes += size;
            ensure!(read <= SOURCELIMIT, "source exceeds size limit");
            digest.update(&buffer[..size]);
        }
        let after = file.metadata()?;
        ensure!(
            read as u64 == before.len()
                && before.len() == after.len()
                && before.modified().ok() == after.modified().ok(),
            "source changed during inspection"
        );
        Ok(Checked::Hash(format!("{:x}", digest.finalize())))
    }
}
