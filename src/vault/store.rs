use crate::vault::{Secret, Vault};
use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct VaultStore {
    pool: SqlitePool,
    _lock: Arc<File>,
}

impl VaultStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from(crate::database::open(path.as_ref()).await?)
    }

    /// Open only to report on scope and trust. See [`crate::database::glance`].
    pub async fn glance(path: impl AsRef<Path>) -> Result<Self> {
        Self::from(crate::database::glance(path.as_ref()).await?)
    }

    fn from(opened: crate::database::Opened) -> Result<Self> {
        Ok(Self {
            pool: opened.pool,
            _lock: opened.lock,
        })
    }

    pub async fn vaults(&self) -> Result<Vec<Vault>> {
        sqlx::query_as("SELECT id, name, created FROM vault ORDER BY lower(name)")
            .fetch_all(&self.pool)
            .await
            .context("could not list vaults")
    }

    pub async fn createvault(&self, name: &str) -> Result<Vault> {
        let name = validname(name)?;
        let created = now()?;
        let id = sqlx::query("INSERT INTO vault(name, created) VALUES (?, ?)")
            .bind(&name)
            .bind(created)
            .execute(&self.pool)
            .await
            .context("a vault with that name may already exist")?
            .last_insert_rowid();
        Ok(Vault { id, name, created })
    }

    pub async fn deletevault(&self, id: i64) -> Result<Option<Vault>> {
        let vault = sqlx::query_as::<_, Vault>("SELECT id, name, created FROM vault WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(vault) = vault else {
            return Ok(None);
        };
        let secrets = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret WHERE vaultid = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        anyhow::ensure!(
            secrets == 0,
            "forget the vault's secrets before deleting it"
        );
        sqlx::query("DELETE FROM vault WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(Some(vault))
    }

    pub async fn secrets(&self, vaultid: i64) -> Result<Vec<Secret>> {
        sqlx::query_as(
            "SELECT secret.id, secret.vaultid, vault.name AS vault, secret.name, secret.env, \
             secret.account, EXISTS(SELECT 1 FROM globalenv WHERE secretid = secret.id) AS global, \
             secret.created FROM secret JOIN vault ON vault.id = secret.vaultid \
             WHERE secret.vaultid = ? ORDER BY lower(secret.name)",
        )
        .bind(vaultid)
        .fetch_all(&self.pool)
        .await
        .context("could not list secrets")
    }

    pub async fn createsecret(
        &self,
        vaultid: i64,
        name: &str,
        env: &str,
        global: bool,
    ) -> Result<Secret> {
        let name = validname(name)?;
        let env = validenv(env)?;
        let vault = sqlx::query_as::<_, Vault>("SELECT id, name, created FROM vault WHERE id = ?")
            .bind(vaultid)
            .fetch_one(&self.pool)
            .await
            .context("the selected vault no longer exists")?;
        let created = now()?;
        let account = format!("{}.{}", vault.name, name);
        let mut transaction = self.pool.begin().await?;
        let id = sqlx::query(
            "INSERT INTO secret(vaultid, name, env, account, created) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(vaultid)
        .bind(&name)
        .bind(&env)
        .bind(&account)
        .bind(created)
        .execute(&mut *transaction)
        .await
        .context("that secret name or environment variable is already in this vault")?
        .last_insert_rowid();
        if global {
            sqlx::query("INSERT INTO globalenv(env, secretid) VALUES (?, ?)")
                .bind(&env)
                .bind(id)
                .execute(&mut *transaction)
                .await
                .context("that environment variable already has a global secret")?;
        }
        transaction.commit().await?;
        Ok(Secret {
            id,
            vaultid,
            vault: vault.name,
            name,
            env,
            account,
            global,
            created,
        })
    }

    pub async fn deletesecret(&self, id: i64) -> Result<Option<Secret>> {
        let secret = self.secretbyid(id).await?;
        if secret.is_some() {
            sqlx::query("DELETE FROM secret WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(secret)
    }

    pub async fn setglobal(&self, id: i64, global: bool) -> Result<()> {
        let secret = self
            .secretbyid(id)
            .await?
            .context("the secret no longer exists")?;
        if global {
            sqlx::query(
                "INSERT INTO globalenv(env, secretid) VALUES (?, ?) \
                 ON CONFLICT(env) DO UPDATE SET secretid = excluded.secretid",
            )
            .bind(secret.env)
            .bind(id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query("DELETE FROM globalenv WHERE secretid = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn globalsecrets(&self) -> Result<Vec<Secret>> {
        sqlx::query_as(
            "SELECT secret.id, secret.vaultid, vault.name AS vault, secret.name, secret.env, \
             secret.account, 1 AS global, secret.created FROM globalenv \
             JOIN secret ON secret.id = globalenv.secretid JOIN vault ON vault.id = secret.vaultid \
             ORDER BY globalenv.env",
        )
        .fetch_all(&self.pool)
        .await
        .context("could not load global secrets")
    }

    pub async fn findsecret(&self, reference: &str) -> Result<Option<Secret>> {
        let Some((vault, name)) = reference.split_once('.') else {
            return Ok(None);
        };
        sqlx::query_as(
            "SELECT secret.id, secret.vaultid, vault.name AS vault, secret.name, secret.env, \
             secret.account, EXISTS(SELECT 1 FROM globalenv WHERE secretid = secret.id) AS global, \
             secret.created FROM secret JOIN vault ON vault.id = secret.vaultid \
             WHERE vault.name = ? AND secret.name = ?",
        )
        .bind(vault)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .context("could not resolve a vault reference")
    }

    pub async fn trust(&self, path: &Path, digest: &str) -> Result<()> {
        let path = normalize(path)?;
        sqlx::query(
            "INSERT INTO trust(path, digest, updated) VALUES (?, ?, ?) \
             ON CONFLICT(path) DO UPDATE SET digest = excluded.digest, updated = excluded.updated",
        )
        .bind(path)
        .bind(digest)
        .bind(now()?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn digest(&self, path: &Path) -> Result<Option<String>> {
        let path = normalize(path)?;
        sqlx::query_scalar("SELECT digest FROM trust WHERE path = ?")
            .bind(path)
            .fetch_optional(&self.pool)
            .await
            .context("could not check scope trust")
    }

    pub async fn untrust(&self, path: &Path) -> Result<bool> {
        let path = normalize(path)?;
        sqlx::query("DELETE FROM trust WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected() > 0)
            .context("could not revoke scope trust")
    }

    async fn secretbyid(&self, id: i64) -> Result<Option<Secret>> {
        sqlx::query_as(
            "SELECT secret.id, secret.vaultid, vault.name AS vault, secret.name, secret.env, \
             secret.account, EXISTS(SELECT 1 FROM globalenv WHERE secretid = secret.id) AS global, \
             secret.created FROM secret JOIN vault ON vault.id = secret.vaultid WHERE secret.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("could not read secret metadata")
    }
}

fn validname(value: &str) -> Result<String> {
    let value = value.trim();
    anyhow::ensure!(!value.is_empty(), "name cannot be empty");
    anyhow::ensure!(
        value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-'),
        "names may contain letters, numbers, and hyphens"
    );
    Ok(value.to_owned())
}

fn validenv(value: &str) -> Result<String> {
    let value = value.trim();
    let mut characters = value.chars();
    anyhow::ensure!(
        characters
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_'),
        "environment names must begin with a letter or underscore"
    );
    anyhow::ensure!(
        characters.all(|character| character.is_ascii_alphanumeric() || character == '_'),
        "environment names may contain only letters, numbers, and underscores"
    );
    Ok(value.to_ascii_uppercase())
}

fn now() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs() as i64)
}

fn normalize(path: &Path) -> Result<String> {
    path.canonicalize()
        .with_context(|| format!("could not resolve {}", path.display()))
        .map(|path| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_vaults_secrets_and_global_scope() {
        let directory = tempfile::tempdir().unwrap();
        let store = VaultStore::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        let vault = store.createvault("work").await.unwrap();
        let secret = store
            .createsecret(vault.id, "database", "database_url", true)
            .await
            .unwrap();

        assert_eq!(secret.env, "DATABASE_URL");
        assert_eq!(store.globalsecrets().await.unwrap(), vec![secret]);
        assert_eq!(
            store
                .findsecret("work.database")
                .await
                .unwrap()
                .unwrap()
                .name,
            "database"
        );
    }
}
