use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::PathBuf;

pub(super) struct Options {
    pub action: String,
    pub project: PathBuf,
    pub command: Vec<OsString>,
    id: Option<i64>,
    values: HashMap<String, String>,
    flags: HashSet<String>,
}

impl Options {
    pub fn parse(arguments: &[OsString]) -> Result<Self> {
        let action = arguments
            .first()
            .and_then(|value| value.to_str())
            .context(super::USAGE)?;
        let mut options = Self {
            action: action.to_owned(),
            project: std::env::current_dir()?,
            command: Vec::new(),
            id: None,
            values: HashMap::new(),
            flags: HashSet::new(),
        };
        let mut rest = arguments.iter().skip(1);
        while let Some(argument) = rest.next() {
            let text = argument.to_str().context("invalid argument encoding")?;
            if text == "--" {
                anyhow::ensure!(action == "check", "only check takes a command");
                options.command = rest.cloned().collect();
                break;
            }
            let valued = matches!(
                text,
                "--project" | "--session" | "--timeout" | "--limit" | "--budget" | "--fail-under"
            );
            if valued {
                let allowed = text == "--project"
                    || match action {
                        "report" => text == "--session",
                        "check" => text == "--timeout",
                        "eval" => matches!(text, "--limit" | "--budget" | "--fail-under"),
                        _ => false,
                    };
                anyhow::ensure!(allowed, "unexpected {text} for {action}");
                let value = rest.next().context("flag needs a value")?;
                if text == "--project" {
                    options.project = PathBuf::from(value);
                } else {
                    let value = value.to_str().context("invalid argument encoding")?;
                    anyhow::ensure!(
                        options.values.insert(text.into(), value.into()).is_none(),
                        "duplicate {text}"
                    );
                }
            } else if text.starts_with('-') {
                let allowed = text == "--json"
                    || match action {
                        "confirm" | "reject" | "dismiss" | "resolve" | "restore" => {
                            text == "--confirm"
                        }
                        "approve" => matches!(text, "--confirm" | "--global"),
                        "review" => matches!(text, "--confirm" | "--accept" | "--reject"),
                        _ => false,
                    };
                anyhow::ensure!(allowed, "unexpected {text} for {action}");
                anyhow::ensure!(options.flags.insert(text.into()), "duplicate {text}");
            } else {
                anyhow::ensure!(
                    matches!(
                        action,
                        "show"
                            | "proposal"
                            | "check"
                            | "confirm"
                            | "reject"
                            | "approve"
                            | "dismiss"
                            | "review"
                            | "restore"
                    ),
                    "unexpected argument {text}"
                );
                anyhow::ensure!(options.id.is_none(), "only one id at a time");
                let id: i64 = text.parse().context("expected an id")?;
                anyhow::ensure!(id > 0, "expected a positive id");
                options.id = Some(id);
            }
        }
        Ok(options)
    }

    pub fn id(&self) -> Result<i64> {
        self.id.context("this command needs an id")
    }
    pub fn value(&self, flag: &str) -> Option<&str> {
        self.values.get(flag).map(String::as_str)
    }
    pub fn has(&self, flag: &str) -> bool {
        self.flags.contains(flag)
    }
    pub fn confirm(&self) -> Result<()> {
        anyhow::ensure!(self.has("--confirm"), "this action needs --confirm");
        Ok(())
    }
}
