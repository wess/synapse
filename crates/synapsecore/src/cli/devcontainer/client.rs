use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) struct Enrollment {
    bundle: PathBuf,
    mount: String,
    knownhosts: String,
    stdio: String,
    connect: String,
}

impl Enrollment {
    pub fn new(
        bundle: &Path,
        id: &str,
        login: &str,
        host: &str,
        port: u16,
        key: &str,
    ) -> Result<Self> {
        let source = super::pathtext(bundle)?;
        ensure!(
            !source.contains(','),
            "the SSH bundle path contains a comma Docker cannot mount"
        );
        let name = format!("synapse-ssh-{id}");
        let mount = format!("/run/synapse/{id}");
        let knownhosts = format!("{name} {key}\n");
        let stdio = format!(
            "#!/bin/sh\nexec ssh -F /dev/null -T \\\n\
             -o BatchMode=yes -o IdentitiesOnly=yes -o IdentityAgent=none \\\n\
             -o StrictHostKeyChecking=yes -o GlobalKnownHostsFile=/dev/null -o UpdateHostKeys=no \\\n\
             -o UserKnownHostsFile={mount}/knownhosts -o HostKeyAlias={name} \\\n\
             -o ConnectTimeout=5 -o ServerAliveInterval=30 -o ServerAliveCountMax=2 \\\n\
             -i {mount}/identity -l {} -p {port} {}\n",
            shellword(login),
            shellword(host),
        );
        let mut connect = format!(
            "#!/bin/sh\nset -eu\n\
             if ! command -v ssh >/dev/null 2>&1; then\n\
               echo 'Install the OpenSSH client in the devcontainer to use Synapse.' >&2\n\
               exit 1\n\
             fi\n\
             if [ ! -r {mount}/identity ]; then\n\
               echo 'The devcontainer user cannot read the Synapse SSH identity mount.' >&2\n\
               exit 1\n\
             fi\n"
        );
        for agent in crate::agent::tool::tools(&crate::files::home()?, None) {
            let add = &agent.connect.add;
            let Some(separator) = add.iter().position(|token| token == "--") else {
                continue;
            };
            if !add.starts_with(&["mcp".to_owned(), "add".to_owned()])
                || add.get(separator - 1).map(String::as_str) != Some("synapse")
                || add.get(separator + 1).map(String::as_str) != Some("{server}")
            {
                continue;
            }
            let command = shellword(&agent.command);
            let arguments = add[..=separator]
                .iter()
                .enumerate()
                .map(|(index, token)| {
                    // A sandbox-local registration stays in this workspace when
                    // a client supports it, even with a shared container home.
                    if token == "user" && index > 0 && add[index - 1] == "--scope" {
                        shellword("local")
                    } else {
                        shellword(if token == "synapse" { &name } else { token })
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            connect.push_str(&format!(
                "if command -v {command} >/dev/null 2>&1 && ! {command} mcp get {name} >/dev/null 2>&1; then\n\
                   {command} {arguments} sh {mount}/stdio\n\
                 fi\n"
            ));
        }
        Ok(Self {
            bundle: bundle.to_owned(),
            mount,
            knownhosts,
            stdio,
            connect,
        })
    }

    pub fn write(&self) -> Result<()> {
        for (name, content) in [
            ("knownhosts", &self.knownhosts),
            ("stdio", &self.stdio),
            ("connect", &self.connect),
        ] {
            crate::files::atomicreplace(&self.bundle.join(name), content.as_bytes(), None)?;
        }
        Ok(())
    }

    pub fn snippet(&self) -> Value {
        json!({
            "mounts": [format!("source={},target={},type=bind,readonly", self.bundle.display(), self.mount)],
            "postStartCommand": { "synapse": ["sh", format!("{}/connect", self.mount)] }
        })
    }
}

pub(super) fn shellword(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
