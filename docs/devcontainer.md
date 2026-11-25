# Synapse in a VS Code devcontainer

Install Synapse on the macOS or Linux host. Inside the container, your existing
agent CLI uses the OpenSSH client to recall, read, and save memory for the host
project you approve. Those writes appear in the host's Synapse store.

## Set it up

1. Enable the host's OpenSSH service for your account. On macOS, open **System
   Settings → General → Sharing → Remote Login** and allow your user. The host
   command uses the standard `~/.ssh/authorized_keys` file and pins the host's
   Ed25519 public key.
2. From the host checkout for the devcontainer, run:

   ```sh
   synapse devcontainer setup
   ```

   Merge the printed `mounts` and `postStartCommand` entries into
   `.devcontainer/devcontainer.json`. Append to any existing mounts. If you
   already have a startup command, keep it and add the generated `synapse`
   entry alongside it in an object, under a different key.
3. In VS Code, select **Dev Containers: Rebuild Container**. The startup
   command registers Synapse with compatible agent CLIs found on the container's
   PATH. Start a fresh agent session and ask it to recall a known project
   decision. Save a new project decision and find it on the host with
   `synapse memory list "<words from the decision>"`.

The container needs `sh` and `ssh`. For a Debian or Ubuntu image without SSH,
add `openssh-client` to the packages installed in its Dockerfile. The grant's
private identity lives in the host's `~/.ssh/synapse` directory. The generated
bind mount makes only that grant's client files available in the container, so
the container user must be able to read them. Keep the key directory on the
host and share the configuration only with containers you trust to access the
project's memory.

## Connection settings

- Docker Desktop uses `host.docker.internal` by default.
- For a native Linux Docker Engine, add
  `"runArgs": ["--add-host=host.docker.internal:host-gateway"]` to an
  image- or Dockerfile-based configuration. With Compose, give the devcontainer
  service an `extra_hosts` entry of `host.docker.internal=host-gateway`.
  SSH must listen on an address reachable from the container.
- For a different SSH address, pass `--host <hostname>`. Use `--port <number>`
  for a custom SSH port and `--host-key <file>` for a custom Ed25519 **public**
  host key. Re-run setup with the same settings after an intentional host-key
  rotation.
- The mount source must exist on the machine running the Docker engine. Run
  setup on that machine for a remote Docker host.
- Use `--project <folder>` to approve a checkout from another working directory.
- If you install the agent CLI after the container starts, run the generated
  startup command in a container terminal. Substitute the grant ID printed by
  setup and then start a fresh agent session.

  ```sh
  sh /run/synapse/GRANT_ID/connect
  ```

The host fixes the project identity at setup, so container paths can differ
from the host checkout. An SSH session gets only `recall`, `readmemory`, and
`remember` for that project. Any process that can read the mounted key can
use that grant. See the [security, stability, and resource review](review.md#devcontainer-release-audit).

## Troubleshoot the connection

- If setup cannot read the SSH host key, check that SSH is enabled for your
  account and that an Ed25519 host key exists. Use `--host-key` for a custom
  public key location.
- If the container cannot reach the host, check the address and port from inside
  it. On native Linux, check the host-gateway mapping and the SSH listener.
- If startup reports that it cannot read the identity, check that the mount is
  present and readable by the container user. Keep the private key owner-only.
- If the host executable moved, run setup again from the checkout to refresh the
  forced command. If setup reports that the project identity or data directory
  changed, revoke the old grant before creating the new one.
- The host allows eight simultaneous devcontainer sessions per account.
  Close unused agent sessions if all slots are in use.

## Revoke access

Run `synapse devcontainer revoke` from the host checkout. For a moved or deleted
checkout, use `synapse devcontainer revoke --id <id>` with the grant ID printed
by setup. Revocation removes the grant's authorized key and client files.
Subsequent memory calls in an open session also lose access. Remove the
generated mount and startup entry before rebuilding the container again.
Close open agent sessions to release their database locks. Revoke each grant
before uninstalling the host CLI.
