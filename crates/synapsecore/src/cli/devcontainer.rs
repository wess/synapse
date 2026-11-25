mod client;

use crate::cli::Outcome;
use crate::mcp::ssh::Grant;
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DEFAULTKEY: &str = "/etc/ssh/ssh_host_ed25519_key.pub";

pub(super) fn run(arguments: &[OsString]) -> Result<Outcome> {
    match arguments.split_first() {
        Some((verb, rest)) if verb == "setup" => setup(rest)?,
        Some((verb, rest)) if verb == "revoke" => revoke(rest)?,
        Some((verb, rest)) if verb == "serve" => {
            let [path, generation] = rest else {
                bail!("invalid devcontainer grant");
            };
            runtime()?.block_on(crate::mcp::ssh::run(
                PathBuf::from(path),
                generation.to_str().context("invalid grant generation")?,
            ))?;
        }
        _ => bail!("use synapse devcontainer setup or synapse devcontainer revoke"),
    }
    Ok(Outcome::Exit(0))
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?)
}

fn setup(arguments: &[OsString]) -> Result<()> {
    let mut project = None;
    let mut host = "host.docker.internal";
    let mut port = 22;
    let mut hostkey = Path::new(DEFAULTKEY);
    for (flag, value) in options(arguments)? {
        match flag.to_str() {
            Some("--project") => project = Some(Path::new(value)),
            Some("--host") => host = value.to_str().context("invalid SSH host")?,
            Some("--port") => {
                port = value.to_str().context("invalid SSH port")?.parse::<u16>()?;
                ensure!(port != 0, "SSH port must be between 1 and 65535");
            }
            Some("--host-key") => hostkey = Path::new(value),
            _ => bail!("unknown setup option {}", flag.to_string_lossy()),
        }
    }
    ensure!(
        host.parse::<std::net::IpAddr>().is_ok()
            || (!host.is_empty()
                && !host.starts_with('-')
                && host
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))),
        "use a hostname or IP address for --host"
    );
    // The stock host service can create its keys on the first SSH connection.
    if hostkey == Path::new(DEFAULTKEY) && !hostkey.exists() && port == 22 {
        let _ = Command::new("ssh-keyscan")
            .args(["-T", "2", "-t", "ed25519", "127.0.0.1"])
            .output();
    }
    let hostkey = publickey(hostkey).context(
        "could not read the SSH host's Ed25519 public key. Enable the host's SSH server, or pass its public key with --host-key",
    )?;
    let login = Command::new("id").arg("-un").output()?;
    ensure!(
        login.status.success(),
        "could not determine the host login name"
    );
    let login = std::str::from_utf8(&login.stdout)?.trim();
    let root = projectroot(project)?;
    let project = crate::brain::projectkey(&root)?.context("could not resolve the project")?;
    let database = std::path::absolute(crate::files::database()?)?;
    let home = crate::files::home()?.canonicalize()?;
    let id = grantid(&root);
    let (ssh, grants, _lock) = grantstore(&home)?;
    let directory = grants.join(&id);
    let bundle = directory.join("client");
    let enrollment = client::Enrollment::new(&bundle, &id, login, host, port, &hostkey)?;
    let path = directory.join("grant.json");
    let previous = path.exists().then(|| Grant::load(&path)).transpose()?;
    if let Some(current) = &previous {
        ensure!(
            current.project == project && current.database == database,
            "the project's identity or data directory changed. Revoke grant {id} before recreating it"
        );
    }

    runtime()?.block_on(crate::brain::Brain::open(&database))?;
    privatefolder(&bundle)?;
    let key = identity(&bundle)?;
    let generation = digest(key.as_bytes());
    let command = format!(
        "{} devcontainer serve {} {generation}",
        client::shellword(pathtext(&std::env::current_exe()?)?),
        client::shellword(pathtext(&path)?),
    );
    let authorization = format!(
        "restrict,command=\"{}\" {key} synapse-devcontainer-{id}",
        command.replace('"', "\\\"")
    );
    enrollment.write()?;
    let grant = Grant {
        project,
        database,
        generation,
        authorization,
    };
    let keys = ssh.join("authorized_keys");
    let snapshot = crate::files::Snapshot::capture(&path)?;
    let replacement = updatedkeys(
        &keys,
        previous.as_ref().map(|grant| grant.authorization.as_str()),
        Some(&grant.authorization),
    )?;
    if let Err(error) = crate::files::atomicreplace(&path, &serde_json::to_vec(&grant)?, None)
        .and_then(|_| crate::files::write(&keys, &replacement))
    {
        snapshot.restore()?;
        return Err(error);
    }
    eprintln!("Grant {id}. Merge the snippet into .devcontainer/devcontainer.json and rebuild.");
    println!("{}", serde_json::to_string_pretty(&enrollment.snippet())?);
    Ok(())
}

fn revoke(arguments: &[OsString]) -> Result<()> {
    let mut project = None;
    let mut id = None;
    for (flag, value) in options(arguments)? {
        match flag.to_str() {
            Some("--project") => project = Some(Path::new(value)),
            Some("--id") => id = Some(value.to_str().context("invalid devcontainer grant id")?),
            _ => bail!("unknown revoke option {}", flag.to_string_lossy()),
        }
    }
    ensure!(
        id.is_none() || project.is_none(),
        "use either --project or --id"
    );
    let id = match id {
        Some(id) => {
            ensure!(
                id.len() == 24 && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "invalid devcontainer grant id"
            );
            id.to_ascii_lowercase()
        }
        None => grantid(&projectroot(project)?),
    };
    let (ssh, grants, _lock) = grantstore(&crate::files::home()?.canonicalize()?)?;
    let directory = grants.join(&id);
    let path = directory.join("grant.json");
    if path.exists() {
        let grant = Grant::load(&path)?;
        let keys = ssh.join("authorized_keys");
        let replacement = updatedkeys(&keys, Some(&grant.authorization), None)?;
        crate::files::write(&keys, &replacement)?;
        fs::remove_file(&path)?;
    }
    let bundle = directory.join("client");
    for name in ["identity", "identity.pub", "knownhosts", "connect", "stdio"] {
        remove(&bundle.join(name))?;
    }
    for folder in [&bundle, &directory] {
        match fs::remove_dir(folder) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    println!("Devcontainer grant {id} revoked");
    Ok(())
}

fn options(arguments: &[OsString]) -> Result<impl Iterator<Item = (&OsString, &OsString)>> {
    ensure!(
        arguments.len().is_multiple_of(2),
        "each option needs a value"
    );
    Ok(arguments.chunks_exact(2).map(|pair| (&pair[0], &pair[1])))
}

fn projectroot(project: Option<&Path>) -> Result<PathBuf> {
    let current = std::env::current_dir()?;
    let folder = project.unwrap_or(&current);
    ensure!(
        folder.is_dir(),
        "{} is not a project folder",
        folder.display()
    );
    crate::brain::projectroot(folder)?.context("could not resolve the project folder")
}

fn grantid(project: &Path) -> String {
    let hash = digest(project.as_os_str().as_encoded_bytes());
    hash[..24].to_owned()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn grantstore(home: &Path) -> Result<(PathBuf, PathBuf, File)> {
    let ssh = home.join(".ssh");
    let grants = ssh.join("synapse");
    privatefolder(&grants)?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(grants.join("lock"))?;
    fs2::FileExt::lock_exclusive(&lock)?;
    Ok((ssh, grants, lock))
}

fn privatefolder(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    Ok(())
}

fn identity(bundle: &Path) -> Result<String> {
    let path = bundle.join("identity");
    if !path.exists() {
        let status = Command::new("ssh-keygen")
            .args([
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-C",
                "synapse-devcontainer",
                "-f",
            ])
            .arg(&path)
            .stdin(Stdio::null())
            .status()
            .context("could not start ssh-keygen")?;
        ensure!(
            status.success(),
            "could not generate the devcontainer SSH key"
        );
    }
    // Derive the public key from the private half so a stale .pub file cannot
    // authorize a different identity after setup is retried.
    let output = Command::new("ssh-keygen")
        .arg("-y")
        .arg("-f")
        .arg(&path)
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success(),
        "could not read the devcontainer public key"
    );
    parsekey(std::str::from_utf8(&output.stdout)?)
}

fn publickey(path: &Path) -> Result<String> {
    parsekey(&fs::read_to_string(path)?)
}

fn parsekey(input: &str) -> Result<String> {
    use base64::Engine;
    let mut fields = input.split_whitespace();
    let kind = fields.next().unwrap_or_default();
    let key = fields.next().unwrap_or_default();
    let decoded = base64::engine::general_purpose::STANDARD.decode(key)?;
    ensure!(
        kind == "ssh-ed25519"
            && decoded.len() == 51
            && decoded.starts_with(b"\0\0\0\x0bssh-ed25519\0\0\0\x20"),
        "expected an Ed25519 SSH public key"
    );
    Ok(format!("{kind} {key}"))
}

fn updatedkeys(path: &Path, old: Option<&str>, add: Option<&str>) -> Result<String> {
    let mut content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", path.display()));
        }
    };
    if old == add && add.is_some_and(|line| content.lines().any(|current| current == line)) {
        return Ok(content);
    }
    if let Some(line) = old {
        content = content
            .split_inclusive('\n')
            .filter(|current| current.trim_end_matches(['\r', '\n']) != line)
            .collect();
    }
    if let Some(line) = add.filter(|line| !content.lines().any(|current| current == *line)) {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(line);
        content.push('\n');
    }
    Ok(content)
}

fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("could not remove {}", path.display())),
    }
}

fn pathtext(path: &Path) -> Result<&str> {
    let path = path.to_str().context("SSH paths must use UTF-8")?;
    ensure!(
        !path.contains(['\r', '\n']),
        "SSH paths must fit on one line"
    );
    Ok(path)
}
