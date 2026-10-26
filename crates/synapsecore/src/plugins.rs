//! versioned, local scene packs. Plugins are data, never executable code.
mod tests;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const ENTRYLIMIT: usize = 256;
const PLUGINLIMIT: usize = 128;
const MANIFESTLIMIT: u64 = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub scene: Scene,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub floor: String,
    pub wall: String,
    pub wood: String,
    pub accent: String,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub characters: Option<String>,
    #[serde(default = "framewidth")]
    pub framewidth: u32,
    #[serde(default = "frameheight")]
    pub frameheight: u32,
}
fn framewidth() -> u32 {
    26
}
fn frameheight() -> u32 {
    36
}

#[derive(Clone, Debug)]
pub struct Plugin {
    pub manifest: Manifest,
    pub root: Option<PathBuf>,
}

#[derive(Default)]
pub struct Catalog {
    pub plugins: Vec<Plugin>,
    pub problems: Vec<String>,
}

pub fn builtin() -> Plugin {
    Plugin {
        manifest: toml::from_str(include_str!("../../../plugins/workshop/plugin.toml"))
            .expect("bundled workshop manifest"),
        root: None,
    }
}

pub fn folder() -> Result<PathBuf> {
    Ok(crate::files::data()?.join("plugins"))
}

pub fn catalog(root: &Path) -> Catalog {
    let mut result = Catalog {
        plugins: vec![builtin()],
        problems: Vec::new(),
    };
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return result,
        Err(error) => {
            result.problems.push(format!("{}: {error}", root.display()));
            return result;
        }
    };
    let mut paths = Vec::new();
    let mut truncated = false;
    for (number, entry) in entries.enumerate() {
        if number == ENTRYLIMIT {
            truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.is_dir() {
            if paths.len() == PLUGINLIMIT {
                truncated = true;
                break;
            }
            paths.push(path);
        }
    }
    if truncated {
        result.problems.push(
            "plugin scan stopped at 256 entries or 128 scene packs; remove unused entries"
                .to_owned(),
        );
    }
    paths.sort();
    for path in paths {
        match load(&path) {
            Ok(plugin)
                if result
                    .plugins
                    .iter()
                    .any(|other| other.manifest.id == plugin.manifest.id) =>
            {
                result
                    .problems
                    .push(format!("{}: duplicate plugin id", path.display()))
            }
            Ok(plugin) => result.plugins.push(plugin),
            Err(error) => result
                .problems
                .push(format!("{}: {error:#}", path.display())),
        }
    }
    result
}

pub fn load(root: &Path) -> Result<Plugin> {
    let path = root.join("plugin.toml");
    let file = std::fs::File::open(&path)?;
    ensure!(
        file.metadata()?.len() <= MANIFESTLIMIT,
        "manifest exceeds 64 KiB"
    );
    let mut text = String::new();
    file.take(MANIFESTLIMIT + 1).read_to_string(&mut text)?;
    ensure!(
        text.len() <= MANIFESTLIMIT as usize,
        "manifest exceeds 64 KiB"
    );
    let manifest: Manifest = toml::from_str(&text).context("invalid manifest")?;
    ensure!(
        manifest.api == 1,
        "unsupported plugin API {}; expected 1",
        manifest.api
    );
    ensure!(
        manifest.kind == "scene",
        "unsupported plugin kind; expected scene"
    );
    ensure!(
        !manifest.id.is_empty()
            && manifest.id.len() <= 64
            && manifest
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit()),
        "id must contain lowercase letters and digits"
    );
    ensure!(
        !manifest.name.trim().is_empty() && manifest.name.len() <= 80,
        "name must contain 1 to 80 bytes"
    );
    ensure!(!manifest.version.is_empty(), "version is required");
    for color in [
        &manifest.scene.floor,
        &manifest.scene.wall,
        &manifest.scene.wood,
        &manifest.scene.accent,
    ] {
        colorvalue(color)?;
    }
    ensure!(
        (1..=128).contains(&manifest.scene.framewidth)
            && (1..=128).contains(&manifest.scene.frameheight),
        "sprite frames must be 1 to 128 pixels"
    );
    for name in [&manifest.scene.background, &manifest.scene.characters]
        .into_iter()
        .flatten()
    {
        asset(root, name)?;
    }
    Ok(Plugin {
        manifest,
        root: Some(root.canonicalize()?),
    })
}

pub fn colorvalue(value: &str) -> Result<[u8; 4]> {
    ensure!(
        value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|b| b.is_ascii_hexdigit()),
        "colors must use #rrggbb"
    );
    Ok([
        u8::from_str_radix(&value[1..3], 16)?,
        u8::from_str_radix(&value[3..5], 16)?,
        u8::from_str_radix(&value[5..7], 16)?,
        255,
    ])
}

pub fn asset(root: &Path, name: &str) -> Result<PathBuf> {
    let relative = Path::new(name);
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "asset must be a relative path inside the plugin"
    );
    ensure!(
        relative.extension().and_then(|value| value.to_str()) == Some("png"),
        "scene assets must be PNG files"
    );
    let path = root
        .join(relative)
        .canonicalize()
        .context("missing scene asset")?;
    ensure!(
        path.starts_with(root.canonicalize()?),
        "asset escapes plugin directory"
    );
    ensure!(
        std::fs::metadata(&path)?.len() <= 16 * 1024 * 1024,
        "asset exceeds 16 MiB"
    );
    Ok(path)
}
