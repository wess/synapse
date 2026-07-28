pub fn warning(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    if lower.contains("begin private key")
        || lower.contains("begin rsa private key")
        || lower.contains("begin openssh private key")
    {
        return Some("contains a private-key marker".to_owned());
    }
    for line in body.lines() {
        let credentialname = line
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .filter(|word| {
                word.len() >= 8 && word.chars().all(|character| !character.is_lowercase())
            })
            .any(|word| {
                ["_PASSWORD", "_TOKEN", "_SECRET", "_APIKEY", "_API_KEY"]
                    .iter()
                    .any(|suffix| word.ends_with(suffix))
            });
        if credentialname {
            return Some("mentions a credential-shaped environment variable".to_owned());
        }
        let lower = line.trim().to_ascii_lowercase();
        let assignment = lower.contains('=') || lower.contains(':');
        let sensitive = ["password", "passwd", "api_key", "apikey", "token", "secret"]
            .iter()
            .any(|name| lower.starts_with(name) || lower.contains(&format!("_{name}")));
        if assignment && sensitive {
            return Some("contains a possible credential assignment".to_owned());
        }
        if lower.starts_with("bearer ") && lower.len() > 24 {
            return Some("contains a possible bearer token".to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_values_without_flagging_ordinary_discussion() {
        assert!(warning("API_KEY=abc123").is_some());
        assert!(warning("Recover APPLE_APP_PASSWORD from the release vault.").is_some());
        assert!(warning("-----BEGIN PRIVATE KEY-----").is_some());
        assert!(warning("Secrets stay in Keychain.").is_none());
    }
}
