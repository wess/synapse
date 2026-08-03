use anyhow::{Context, Result};
use std::path::Path;

pub fn content(path: &Path, value: &str) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);

    match extension.as_deref() {
        Some("json") => {
            serde_json::from_str::<serde_json::Value>(value)
                .with_context(|| format!("{} is not valid JSON", path.display()))?;
        }
        Some("toml") => {
            toml::from_str::<toml::Value>(value)
                .with_context(|| format!("{} is not valid TOML", path.display()))?;
        }
        Some("yaml" | "yml") => {
            serde_saphyr::from_str::<serde_json::Value>(value)
                .with_context(|| format!("{} is not valid YAML", path.display()))?;
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_structured_formats() {
        assert!(content(Path::new("config.json"), "{\"enabled\":true}").is_ok());
        assert!(content(Path::new("config.toml"), "enabled = true\n").is_ok());
        assert!(content(Path::new("config.yaml"), "enabled: true\n").is_ok());
        assert!(content(Path::new("config.json"), "{").is_err());
        assert!(content(Path::new("config.toml"), "enabled = [\n").is_err());
        assert!(content(Path::new("config.yml"), "enabled: [\n").is_err());
    }
}
