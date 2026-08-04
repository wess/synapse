use anyhow::{Context, Result};

const SERVICE: &str = "app.synapse.vault";

#[cfg(target_os = "macos")]
pub fn set(account: &str, value: &str) -> Result<()> {
    security_framework::passwords::set_generic_password(SERVICE, account, value.as_bytes())
        .context("could not save the secret in macOS Keychain")
}

#[cfg(target_os = "macos")]
pub fn get(account: &str) -> Result<String> {
    let value = security_framework::passwords::get_generic_password(SERVICE, account)
        .context("could not read the secret from macOS Keychain")?;
    String::from_utf8(value).context("the Keychain value is not valid UTF-8")
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
    anyhow::bail!("vault values currently require macOS Keychain")
}

#[cfg(not(target_os = "macos"))]
pub fn get(_account: &str) -> Result<String> {
    anyhow::bail!("vault values currently require macOS Keychain")
}

#[cfg(not(target_os = "macos"))]
pub fn delete(_account: &str) -> Result<()> {
    anyhow::bail!("vault values currently require macOS Keychain")
}
