use crate::vault::{VaultStore, getsecret, resolve};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Fish,
    Zsh,
}

impl FromStr for Shell {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "bash" => Ok(Self::Bash),
            "fish" => Ok(Self::Fish),
            "zsh" => Ok(Self::Zsh),
            _ => anyhow::bail!("expected bash, fish, or zsh"),
        }
    }
}

pub fn hook(shell: Shell) -> String {
    hookcommand(shell, "synaps")
}

pub fn hookcommand(shell: Shell, command: &str) -> String {
    let command = match shell {
        Shell::Bash | Shell::Zsh => posixquote(command),
        Shell::Fish => fishquote(command),
    };
    match shell {
        Shell::Bash => BASH,
        Shell::Fish => FISH,
        Shell::Zsh => ZSH,
    }
    .replace("__SYNAPS_COMMAND__", &command)
}

pub async fn changes(
    shell: Shell,
    store: &VaultStore,
    folder: &Path,
    previous: &str,
) -> Result<String> {
    let resolved = resolve(store, folder).await?;
    if resolved.scopes.is_empty() {
        return Ok(clear(shell, previous, "inactive"));
    }
    if !resolved.warnings.is_empty() {
        return Ok(clear(shell, previous, "blocked"));
    }

    let mut values = BTreeMap::new();
    for (name, secret) in resolved.env {
        values.insert(name, getsecret(&secret.account)?);
    }
    let scope = resolved
        .scopes
        .last()
        .and_then(|scope| scope.path.parent())
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    Ok(render(shell, previous, &values, &scope, "active"))
}

pub fn clear(shell: Shell, previous: &str, state: &str) -> String {
    render(shell, previous, &BTreeMap::new(), "", state)
}

fn render(
    shell: Shell,
    previous: &str,
    values: &BTreeMap<String, String>,
    scope: &str,
    state: &str,
) -> String {
    let previous = keys(previous);
    let current = values.keys().cloned().collect::<BTreeSet<_>>();
    let mut lines = Vec::new();
    for name in previous.difference(&current) {
        lines.push(restore(shell, name));
    }
    for (name, value) in values {
        if !previous.contains(name) {
            lines.push(save(shell, name));
        }
        lines.push(set(shell, name, value));
    }
    let keys = current.into_iter().collect::<Vec<_>>().join(",");
    lines.push(metadata(shell, "__synaps_keys", &keys));
    lines.push(metadata(shell, "__synaps_scope", scope));
    lines.push(metadata(shell, "__synaps_state", state));
    format!("{}\n", lines.join("\n"))
}

fn keys(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .filter(|name| validkey(name))
        .map(ToOwned::to_owned)
        .collect()
}

fn validkey(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn save(shell: Shell, name: &str) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => format!(
            "if [ -z \"${{__synaps_had_{name}+x}}\" ]; then\n  if [ \"${{{name}+x}}\" = x ]; then\n    __synaps_saved_{name}=\"${{{name}}}\"\n    __synaps_had_{name}=1\n  else\n    __synaps_had_{name}=0\n  fi\nfi"
        ),
        Shell::Fish => format!(
            "if not set -q __synaps_had_{name}\n  if set -q {name}\n    set -g __synaps_saved_{name} ${name}\n    set -g __synaps_had_{name} 1\n  else\n    set -g __synaps_had_{name} 0\n  end\nend"
        ),
    }
}

fn restore(shell: Shell, name: &str) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => format!(
            "if [ \"${{__synaps_had_{name}-}}\" = 1 ]; then\n  export {name}=\"${{__synaps_saved_{name}}}\"\nelse\n  unset {name}\nfi\nunset __synaps_saved_{name} __synaps_had_{name}"
        ),
        Shell::Fish => format!(
            "if test \"$__synaps_had_{name}\" = 1\n  set -gx {name} $__synaps_saved_{name}\nelse\n  set -e {name}\nend\nset -e __synaps_saved_{name} __synaps_had_{name}"
        ),
    }
}

fn set(shell: Shell, name: &str, value: &str) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => format!("export {name}={}", posixquote(value)),
        Shell::Fish => format!("set -gx {name} {}", fishquote(value)),
    }
}

fn metadata(shell: Shell, name: &str, value: &str) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => format!("{name}={}", posixquote(value)),
        Shell::Fish => format!("set -g {name} {}", fishquote(value)),
    }
}

fn posixquote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn fishquote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

const ZSH: &str = r#"if [[ -z "${__synaps_hook_loaded-}" ]]; then
  typeset -g __synaps_hook_loaded=1
  typeset -g __synaps_keys=""
  typeset -g __synaps_scope=""
  typeset -g __synaps_state="inactive"
  export SYNAPS_SHELL_ACTIVE=zsh

  __synaps_hook() {
    local previous=$?
    local output
    output="$(command env SYNAPS_SHELL_KEYS="${__synaps_keys-}" __SYNAPS_COMMAND__ export zsh)"
    local result=$?
    if (( result == 0 )); then
      eval "$output"
    fi
    return $previous
  }

  autoload -Uz add-zsh-hook
  add-zsh-hook chpwd __synaps_hook
  add-zsh-hook precmd __synaps_hook
  __synaps_hook
fi"#;

const BASH: &str = r#"if [ -z "${__synaps_hook_loaded-}" ]; then
  __synaps_hook_loaded=1
  __synaps_keys=""
  __synaps_scope=""
  __synaps_state="inactive"
  export SYNAPS_SHELL_ACTIVE=bash

  __synaps_hook() {
    local previous=$?
    local output
    output="$(command env SYNAPS_SHELL_KEYS="${__synaps_keys-}" __SYNAPS_COMMAND__ export bash)"
    local result=$?
    if [ "$result" -eq 0 ]; then
      eval "$output"
    fi
    return "$previous"
  }

  case ";${PROMPT_COMMAND-};" in
    *";__synaps_hook;"*) ;;
    *) PROMPT_COMMAND="__synaps_hook${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
  esac
  __synaps_hook
fi"#;

const FISH: &str = r#"if not set -q __synaps_hook_loaded
  set -g __synaps_hook_loaded 1
  set -g __synaps_keys
  set -g __synaps_scope
  set -g __synaps_state inactive
  set -gx SYNAPS_SHELL_ACTIVE fish

  function __synaps_hook --on-variable PWD
    set -l previous $status
    set -l output (env SYNAPS_SHELL_KEYS=(string join , $__synaps_keys) __SYNAPS_COMMAND__ export fish)
    set -l result $status
    if test $result -eq 0
      eval (string join \n $output)
    end
    return $previous
  end

  function __synaps_prompt --on-event fish_prompt
    __synaps_hook
  end
  __synaps_hook
end"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn quotes_shell_metacharacters_as_data() {
        assert_eq!(posixquote("a'b $HOME $(bad)"), "'a'\\''b $HOME $(bad)'");
        assert_eq!(fishquote("a'b\\c"), "'a\\'b\\\\c'");
    }

    #[test]
    fn hook_quotes_an_absolute_cli_path() {
        let output = hookcommand(Shell::Zsh, "/Applications/Synaps App/synaps's");

        assert!(output.contains("'/Applications/Synaps App/synaps'\\''s' export zsh"));
        assert!(!output.contains("__SYNAPS_COMMAND__"));
    }

    #[test]
    fn clear_ignores_unsafe_previous_keys() {
        let output = clear(Shell::Zsh, "SAFE,BAD;echo nope", "inactive");
        assert!(output.contains("unset SAFE"));
        assert!(!output.contains("BAD;"));
    }

    #[test]
    fn activation_saves_and_later_restores_the_original_value() {
        let values = BTreeMap::from([("TOKEN".to_owned(), "scoped".to_owned())]);
        let active = render(Shell::Bash, "", &values, "/project", "active");
        let inactive = clear(Shell::Bash, "TOKEN", "inactive");

        assert!(active.contains("__synaps_saved_TOKEN=\"${TOKEN}\""));
        assert!(active.contains("export TOKEN='scoped'"));
        assert!(inactive.contains("export TOKEN=\"${__synaps_saved_TOKEN}\""));
        assert!(inactive.contains("unset __synaps_saved_TOKEN __synaps_had_TOKEN"));
    }

    #[test]
    fn bash_roundtrip_restores_the_original_value() {
        let values = BTreeMap::from([("TOKEN".to_owned(), "scoped ' value".to_owned())]);
        let active = render(Shell::Bash, "", &values, "/project", "active");
        let inactive = clear(Shell::Bash, "TOKEN", "inactive");
        let script = format!(
            "export TOKEN=original\n{active}\nprintf '%s\\n' \"$TOKEN\"\n{inactive}\nprintf '%s\\n' \"$TOKEN\""
        );
        let output = Command::new("/bin/bash")
            .args(["-c", &script])
            .output()
            .unwrap();

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "scoped ' value\noriginal\n"
        );
    }
}
