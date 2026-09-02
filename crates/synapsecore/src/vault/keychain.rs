use anyhow::{Context, Result};

const SERVICE: &str = "app.synapse.vault";

#[cfg(target_os = "macos")]
pub fn set(account: &str, value: &str) -> Result<()> {
    security_framework::passwords::set_generic_password(SERVICE, account, value.as_bytes())
        .context("could not save the secret in macOS Keychain")
}

/// Nothing stored under that name is `Ok(None)`; being *told no* is an error.
///
/// The difference matters exactly once, and it matters a lot: a migration reads
/// every value out of here, and a denied Keychain prompt that read as "empty"
/// would move nothing, report nothing missing, and then point the machine at
/// the other store.
#[cfg(target_os = "macos")]
pub fn find(account: &str) -> Result<Option<String>> {
    match security_framework::passwords::get_generic_password(SERVICE, account) {
        Ok(value) => String::from_utf8(value)
            .map(Some)
            .context("the Keychain value is not valid UTF-8"),
        Err(error) if error.code() == -25300 => Ok(None),
        Err(error) => Err(error).context("could not read the secret from macOS Keychain"),
    }
}

#[cfg(target_os = "macos")]
pub fn get(account: &str) -> Result<String> {
    find(account)?.with_context(|| format!("{account} has no value in the Keychain"))
}

#[cfg(target_os = "macos")]
pub fn delete(account: &str) -> Result<()> {
    match security_framework::passwords::delete_generic_password(SERVICE, account) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == -25300 => Ok(()),
        Err(error) => Err(error).context("could not remove the secret from macOS Keychain"),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set(_account: &str, _value: &str) -> Result<()> {
    anyhow::bail!("the Keychain backend needs macOS; this machine has the encrypted vault")
}

#[cfg(not(target_os = "macos"))]
pub fn find(_account: &str) -> Result<Option<String>> {
    anyhow::bail!("the Keychain backend needs macOS; this machine has the encrypted vault")
}

#[cfg(not(target_os = "macos"))]
pub fn get(_account: &str) -> Result<String> {
    anyhow::bail!("the Keychain backend needs macOS; this machine has the encrypted vault")
}

#[cfg(not(target_os = "macos"))]
pub fn delete(_account: &str) -> Result<()> {
    anyhow::bail!("the Keychain backend needs macOS; this machine has the encrypted vault")
}
