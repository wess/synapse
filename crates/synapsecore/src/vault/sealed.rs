//! The encrypted value store: `vault.db`, one sealed envelope per account.
//!
//! Its own file rather than a table in `brain.db`, and that is the whole reason
//! it is a second database: `synapse data export` hands somebody a copy of
//! `brain.db`, and a backup of memory must not also be a backup of every
//! credential — sealed or not, a file somebody has is a file somebody can
//! attack at their leisure. It also keeps the promise `brain.db` has always
//! made, that no secret value is in it, true by construction rather than by
//! review.

use crate::database::Schema;
use crate::vault::cipher;
use crate::vault::key::{self, Key};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Sealed {
    pool: SqlitePool,
    key: Key,
    _lock: Arc<File>,
}

pub fn path() -> Result<PathBuf> {
    Ok(crate::files::data()?.join("vault.db"))
}

impl Sealed {
    pub async fn open() -> Result<Self> {
        let path = path()?;
        let opened = crate::database::openas(&path, Schema::Vault).await?;
        Ok(Self {
            pool: opened.pool,
            key: key::load()?,
            _lock: opened.lock,
        })
    }

    pub async fn set(&self, account: &str, value: &str) -> Result<()> {
        let envelope = cipher::seal(&self.key, account, value)?;
        sqlx::query(
            "INSERT INTO secretvalue(account, envelope, updated) VALUES (?, ?, ?) \
             ON CONFLICT(account) DO UPDATE SET envelope = excluded.envelope, \
             updated = excluded.updated",
        )
        .bind(account)
        .bind(envelope)
        .bind(now()?)
        .execute(&self.pool)
        .await
        .context("could not save the secret in the encrypted vault")?;
        Ok(())
    }

    pub async fn find(&self, account: &str) -> Result<Option<String>> {
        match self.envelope(account).await? {
            Some(envelope) => cipher::open(&self.key, account, &envelope).map(Some),
            None => Ok(None),
        }
    }

    pub async fn get(&self, account: &str) -> Result<String> {
        self.find(account)
            .await?
            .with_context(|| format!("{account} has no value in the encrypted vault"))
    }

    pub async fn has(&self, account: &str) -> Result<bool> {
        Ok(self.envelope(account).await?.is_some())
    }

    /// Removing a value that is not there is not an error, for the reason the
    /// Keychain backend gives: `forget` has to be able to finish tidying up a
    /// half-written secret.
    pub async fn delete(&self, account: &str) -> Result<()> {
        sqlx::query("DELETE FROM secretvalue WHERE account = ?")
            .bind(account)
            .execute(&self.pool)
            .await
            .context("could not remove the secret from the encrypted vault")?;
        Ok(())
    }

    async fn envelope(&self, account: &str) -> Result<Option<Vec<u8>>> {
        sqlx::query_scalar::<_, Vec<u8>>("SELECT envelope FROM secretvalue WHERE account = ?")
            .bind(account)
            .fetch_optional(&self.pool)
            .await
            .context("could not read the encrypted vault")
    }
}

fn now() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("the system clock is before 1970")?
        .as_secs() as i64)
}

#[cfg(test)]
#[allow(
    clippy::await_holding_lock,
    reason = "the guard serialises tests over one process-wide SYNAPSE_DATA; \
              holding it across the await is the point"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_value_survives_a_round_trip_and_is_not_in_the_file_in_the_clear() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(root.path());

        let sealed = Sealed::open().await.unwrap();
        sealed.set("work.token", "hunter2").await.unwrap();
        assert_eq!(sealed.get("work.token").await.unwrap(), "hunter2");
        assert!(sealed.has("work.token").await.unwrap());

        let bytes = std::fs::read(path().unwrap()).unwrap();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("hunter2"),
            "the value is in the database in the clear"
        );
    }

    #[tokio::test]
    async fn replacing_a_value_keeps_one_row_and_forgetting_leaves_none() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(root.path());

        let sealed = Sealed::open().await.unwrap();
        sealed.set("work.token", "first").await.unwrap();
        sealed.set("work.token", "second").await.unwrap();
        assert_eq!(sealed.get("work.token").await.unwrap(), "second");

        sealed.delete("work.token").await.unwrap();
        assert!(!sealed.has("work.token").await.unwrap());
        assert!(sealed.get("work.token").await.is_err());
        // Forgetting twice is how a half-written secret gets tidied up.
        sealed.delete("work.token").await.unwrap();
    }

    #[tokio::test]
    async fn a_vault_whose_key_is_gone_reports_it_rather_than_returning_nothing() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(root.path());

        let sealed = Sealed::open().await.unwrap();
        sealed.set("work.token", "hunter2").await.unwrap();
        drop(sealed);

        std::fs::remove_file(key::path().unwrap()).unwrap();
        let replaced = Sealed::open().await.unwrap();
        assert!(replaced.get("work.token").await.is_err());
    }
}
