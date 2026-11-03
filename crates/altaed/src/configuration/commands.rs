//! Stable command identities and editable defaults, independent of window state.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(pub String);
/// Missing entries inherit catalog defaults; an explicit empty string disables
/// the binding. The persisted identity stays stable when UI wording changes.
pub type Shortcuts = BTreeMap<CommandId, String>;

#[derive(Clone, Deserialize)]
pub struct CommandDefinition {
    pub id: CommandId,
    pub label: String,
    pub action: String,
    pub context: Option<String>,
    macos: String,
    other: String,
}
impl CommandDefinition {
    pub fn default_shortcut(&self) -> &str {
        if cfg!(target_os = "macos") {
            &self.macos
        } else {
            &self.other
        }
    }
    pub fn setting_id(&self) -> String {
        format!("core.keybindings.{}", self.id.0)
    }
    pub fn effective_shortcut<'a>(&'a self, overrides: &'a Shortcuts) -> &'a str {
        overrides
            .get(&self.id)
            .map(String::as_str)
            .unwrap_or_else(|| self.default_shortcut())
    }
}
pub fn definitions() -> Result<Vec<CommandDefinition>> {
    let commands: Vec<CommandDefinition> =
        serde_json::from_str(include_str!("../../settings/commands.json"))?;
    let ids: BTreeSet<_> = commands.iter().map(|command| &command.id).collect();
    ensure!(ids.len() == commands.len(), "命令描述存在重复标识");
    Ok(commands)
}
pub fn default_shortcuts() -> Result<Shortcuts> {
    Ok(definitions()?
        .into_iter()
        .map(|command| {
            let shortcut = command.default_shortcut().to_owned();
            (command.id, shortcut)
        })
        .collect())
}
pub fn validate_overrides(shortcuts: &Shortcuts) -> Result<()> {
    let commands = definitions()?;
    for id in shortcuts.keys() {
        ensure!(
            commands.iter().any(|command| &command.id == id),
            "未知快捷键命令：{}",
            id.0
        );
    }
    Ok(())
}

/// Preserve customized shortcuts when a command gets a more precise identity.
/// An explicitly configured new identity always wins over its legacy alias.
pub fn migrate_shortcuts(shortcuts: &mut Shortcuts) {
    if let Some(shortcut) = shortcuts.remove(&CommandId("run.terminal".into())) {
        shortcuts
            .entry(CommandId("run.compile_and_run".into()))
            .or_insert(shortcut);
    }
}
