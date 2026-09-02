//! Which store a value lives in, and how a machine decides.
//!
//! Two backends, one setting. `encrypted` is a file this program owns and can
//! carry anywhere; `keychain` is macOS's, which gates access per application
//! and is the stronger of the two on a machine that has it. The choice belongs
//! to the person, not to the platform, so both are always addressable and
//! `vault migrate` moves values between them.

use crate::vault::sealed::Sealed;
use crate::vault::{Secret, VaultStore, keychain};
use anyhow::{Context, Result};
use sqlx::SqlitePool;

/// Where the choice is recorded. In `brain.db`'s settings beside every other
/// one, because it is a preference and not a secret.
pub const SETTING: &str = "vault.backend";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Keychain,
    Encrypted,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Keychain => "keychain",
            Backend::Encrypted => "encrypted",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "keychain" => Ok(Backend::Keychain),
            "encrypted" => Ok(Backend::Encrypted),
            other => {
                anyhow::bail!("unknown vault backend `{other}`; expected keychain or encrypted")
            }
        }
    }

    pub fn other(self) -> Self {
        match self {
            Backend::Keychain => Backend::Encrypted,
            Backend::Encrypted => Backend::Keychain,
        }
    }
}

/// What this machine stores values in.
pub async fn backend() -> Result<Backend> {
    let opened = crate::database::glance(&crate::files::database()?).await?;
    choose(&opened.pool).await
}

/// Record the choice. It does not move anything — [`migrate`] does that — so
/// nothing here can leave a value behind in a store nobody reads any more.
pub async fn setbackend(chosen: Backend) -> Result<()> {
    let opened = crate::database::open(&crate::files::database()?).await?;
    crate::brain::settings::writevalue(&opened.pool, SETTING, chosen.name()).await
}

/// With nothing recorded, the encrypted store — which is every fresh install,
/// and every machine that is not a Mac.
///
/// A machine that was already holding secrets when this release arrived is not
/// guessed at: migration 11 wrote `keychain` for it once, at upgrade, because
/// that was the only store there was. Deciding it here instead would mean
/// deciding it again on every read, from state that changes as soon as the
/// first secret is written.
async fn choose(pool: &SqlitePool) -> Result<Backend> {
    match crate::brain::settings::value(pool, SETTING).await? {
        Some(value) => Backend::parse(&value),
        None => Ok(Backend::Encrypted),
    }
}

/// The backend, resolved once, for a caller reading or writing more than one
/// value. The one-shot functions in `vault` are this with a shorter life.
pub struct Values {
    backend: Backend,
    sealed: Option<Sealed>,
}

impl Values {
    pub async fn open() -> Result<Self> {
        Self::of(backend().await?).await
    }

    pub async fn of(backend: Backend) -> Result<Self> {
        let sealed = match backend {
            Backend::Encrypted => Some(Sealed::open().await?),
            Backend::Keychain => None,
        };
        Ok(Self { backend, sealed })
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub async fn set(&self, account: &str, value: &str) -> Result<()> {
        match &self.sealed {
            Some(sealed) => sealed.set(account, value).await,
            None => keychain::set(account, value),
        }
    }

    pub async fn get(&self, account: &str) -> Result<String> {
        match &self.sealed {
            Some(sealed) => sealed.get(account).await,
            None => keychain::get(account),
        }
    }

    /// The value, or `None` when the store holds nothing under that name. An
    /// error here means the store could not be asked — which is not the same
    /// answer, and [`migrate`] is the reason the two are kept apart.
    pub async fn find(&self, account: &str) -> Result<Option<String>> {
        match &self.sealed {
            Some(sealed) => sealed.find(account).await,
            None => keychain::find(account),
        }
    }

    pub async fn has(&self, account: &str) -> Result<bool> {
        match &self.sealed {
            Some(sealed) => sealed.has(account).await,
            None => Ok(keychain::find(account)?.is_some()),
        }
    }

    pub async fn delete(&self, account: &str) -> Result<()> {
        match &self.sealed {
            Some(sealed) => sealed.delete(account).await,
            None => keychain::delete(account),
        }
    }
}

/// What happened to one secret during a migration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moved {
    /// Read from the old store and written to the new one.
    Copied,
    /// The new store already held it, and what is already in use is not
    /// overwritten by a copy of unknown age.
    Already,
    /// A label with nothing behind it. Worth reporting and not worth stopping
    /// for: `secret set` is the fix, and there is nothing here to lose.
    Absent,
}

pub struct Migration {
    pub target: Backend,
    pub moved: Vec<(Secret, Moved)>,
    pub removed: bool,
}

/// Move every value into `target` and make it the backend.
///
/// Copies everything first and switches the setting only once all of it
/// arrived: a half-migrated machine that had already been pointed at the new
/// store would resolve some credentials and not others, which is the failure
/// this is most worth avoiding. Values leave the old store last, and `keep`
/// leaves them there.
pub async fn migrate(target: Backend, keep: bool) -> Result<Migration> {
    let store = VaultStore::open(crate::files::database()?).await?;
    let secrets = store.allsecrets().await?;
    let source = Values::of(target.other()).await?;
    let destination = Values::of(target).await?;

    let mut moved = Vec::new();
    for secret in secrets {
        let outcome = if destination.has(&secret.account).await? {
            Moved::Already
        } else {
            // A store that cannot be asked stops the migration here, with
            // nothing moved and nothing switched: a Keychain prompt somebody
            // denied must not read as an empty store and strand every value in
            // it. A label with no value behind it is different, and is only
            // worth reporting.
            match source
                .find(&secret.account)
                .await
                .with_context(|| format!("could not read {}", secret.account))?
            {
                Some(value) => {
                    destination.set(&secret.account, &value).await?;
                    anyhow::ensure!(
                        destination.get(&secret.account).await? == value,
                        "{} did not read back as it was written",
                        secret.account
                    );
                    Moved::Copied
                }
                None => Moved::Absent,
            }
        };
        moved.push((secret, outcome));
    }

    setbackend(target).await?;
    if !keep {
        for (secret, outcome) in &moved {
            if *outcome == Moved::Copied {
                source.delete(&secret.account).await?;
            }
        }
    }
    Ok(Migration {
        target,
        moved,
        removed: !keep,
    })
}

#[cfg(test)]
#[allow(
    clippy::await_holding_lock,
    reason = "the guard serialises tests over one process-wide SYNAPSE_DATA; \
              holding it across the await is the point"
)]
mod tests {
    use super::*;

    #[test]
    fn a_backend_round_trips_through_its_name() {
        for backend in [Backend::Keychain, Backend::Encrypted] {
            assert_eq!(Backend::parse(backend.name()).unwrap(), backend);
        }
        assert!(Backend::parse("kechain").is_err());
    }

    #[tokio::test]
    async fn a_fresh_machine_is_encrypted_and_an_explicit_setting_wins() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(root.path());

        assert_eq!(backend().await.unwrap(), Backend::Encrypted);
        setbackend(Backend::Keychain).await.unwrap();
        assert_eq!(backend().await.unwrap(), Backend::Keychain);
    }

    /// Storing the first secret must not change the answer. It used to: the
    /// backend was inferred from whether any secret existed, so the row written
    /// a moment before the value was read made the machine look like an
    /// upgrade, and the value went to the Keychain on a machine that had just
    /// said `encrypted`.
    #[tokio::test]
    async fn the_first_secret_does_not_move_the_machine_to_another_backend() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::files::scopeddata(root.path());

        let before = backend().await.unwrap();
        let store = VaultStore::open(crate::files::database().unwrap())
            .await
            .unwrap();
        let vault = store.createvault("work").await.unwrap();
        let secret = store
            .createsecret(vault.id, "token", "TOKEN", false)
            .await
            .unwrap();

        assert_eq!(backend().await.unwrap(), before);
        let values = Values::open().await.unwrap();
        values.set(&secret.account, "hunter2").await.unwrap();
        assert_eq!(values.get(&secret.account).await.unwrap(), "hunter2");
        assert_eq!(backend().await.unwrap(), before);
    }
}
