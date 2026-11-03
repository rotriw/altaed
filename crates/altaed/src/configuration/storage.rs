//! Human-readable, versioned config.json. This is separate from code-document
//! sidecars and session recovery. Invalid user files are never replaced silently.
use crate::{atomic_file, configuration::AppPreferences, configuration::commands};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, path::Path};

const VERSION: u32 = 1;
pub const FILE_NAME: &str = "config.json";
const LOCK_FILE_NAME: &str = "config.lock";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    version: u32,
    #[serde(default)]
    editor: crate::configuration::EditorPreferences,
    #[serde(default = "legacy_toolchain")]
    toolchain: crate::configuration::ToolchainPreferences,
    #[serde(default)]
    language_server: crate::language_server::LanguageServerSettings,
    #[serde(default)]
    workbench: crate::configuration::WorkbenchPreferences,
    #[serde(default)]
    keybindings: commands::Shortcuts,
    #[serde(default)]
    extensions: std::collections::BTreeMap<String, serde_json::Value>,
}
// Existing installations already passed the former first-run prompt. Newly
// generated configs explicitly persist setup_complete=false until verification.
fn legacy_toolchain() -> crate::configuration::ToolchainPreferences {
    crate::configuration::ToolchainPreferences {
        setup_complete: true,
        ..Default::default()
    }
}
impl From<ConfigFile> for AppPreferences {
    fn from(file: ConfigFile) -> Self {
        Self {
            editor: file.editor,
            toolchain: file.toolchain,
            language_server: file.language_server,
            workbench: file.workbench,
            keybindings: file.keybindings,
            extensions: file.extensions,
        }
    }
}
pub struct LoadedConfiguration {
    pub preferences: AppPreferences,
    pub created: bool,
}
fn read(directory: &Path) -> Result<Option<AppPreferences>> {
    let path = directory.join(FILE_NAME);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("无法读取 {}", path.display())),
    };
    let record: ConfigFile = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} 格式错误，请修正后重新启动", path.display()))?;
    ensure!(
        record.version == VERSION,
        "{} 的版本 {} 不受支持",
        path.display(),
        record.version
    );
    let mut preferences = AppPreferences::from(record);
    commands::migrate_shortcuts(&mut preferences.keybindings);
    preferences.validate()?;
    Ok(Some(preferences))
}
// The legacy envelope is read only at this migration boundary. New settings
// never depend on document/session persistence formats.
fn read_legacy_preferences(path: &Path) -> Result<AppPreferences> {
    #[derive(Deserialize)]
    struct LegacyRecord {
        version: u32,
        value: AppPreferences,
    }
    let mut record: LegacyRecord = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        record.version == 1,
        "旧配置版本 {} 不受支持",
        record.version
    );
    commands::migrate_shortcuts(&mut record.value.keybindings);
    record.value.validate()?;
    Ok(record.value)
}
pub fn load_or_create(directory: &Path) -> Result<LoadedConfiguration> {
    if let Some(preferences) = read(directory)? {
        return Ok(LoadedConfiguration {
            preferences,
            created: false,
        });
    }
    let legacy = directory.join("application-preferences.json");
    let preferences: AppPreferences = if legacy.try_exists()? {
        read_legacy_preferences(&legacy).context("旧版软件设置迁移失败，原文件已保留")?
    } else {
        AppPreferences::default()
    };
    let mut preferences = preferences;
    let mut defaults = commands::default_shortcuts()?;
    defaults.extend(preferences.keybindings);
    preferences.keybindings = defaults;
    preferences.validate()?;
    let bytes = encode(&preferences)?;
    let created = atomic_file::create(&directory.join(FILE_NAME), &bytes)?;
    let preferences = if created {
        preferences
    } else {
        read(directory)?.context("并发创建配置后文件不可读取")?
    };
    Ok(LoadedConfiguration {
        preferences,
        created,
    })
}
pub fn load(directory: &Path) -> Result<AppPreferences> {
    read(directory)?.context("config.json 不存在，请重新打开设置")
}
pub fn save(
    directory: &Path,
    expected: &AppPreferences,
    preferences: &AppPreferences,
) -> Result<()> {
    preferences.validate()?;
    // Lock a stable sibling file: config.json itself is replaced atomically and
    // therefore cannot serve as a lock shared by successive writers.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(LOCK_FILE_NAME))
        .context("无法打开配置写入锁")?;
    lock.lock().context("无法锁定配置文件")?;
    ensure!(
        &load(directory)? == expected,
        "config.json 已被其他窗口或程序修改，请重新打开设置"
    );
    atomic_file::replace(&directory.join(FILE_NAME), &encode(preferences)?)
}
fn encode(preferences: &AppPreferences) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(&ConfigFile {
        version: VERSION,
        editor: preferences.editor.clone(),
        toolchain: preferences.toolchain.clone(),
        language_server: preferences.language_server.clone(),
        workbench: preferences.workbench.clone(),
        keybindings: preferences.keybindings.clone(),
        extensions: preferences.extensions.clone(),
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_launch_creates_readable_config_and_never_overwrites_user_changes() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("altaed-config-{}", uuid::Uuid::new_v4()));
        let first = load_or_create(&directory)?;
        assert!(first.created);
        let path = directory.join(FILE_NAME);
        let bytes = fs::read(&path)?;
        let json: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(json["version"], VERSION);
        assert!(json["keybindings"]["run.compile"].is_string());
        assert!(json["editor"]["font_size"].is_number());
        assert!(!load_or_create(&directory)?.created);
        assert_eq!(fs::read(&path)?, bytes);
        let mut changed = first.preferences.clone();
        changed.keybindings.insert(
            commands::CommandId("run.compile".into()),
            "ctrl-alt-b".into(),
        );
        save(&directory, &first.preferences, &changed)?;
        assert_eq!(load(&directory)?, changed);
        assert!(save(&directory, &first.preferences, &first.preferences).is_err());
        fs::write(&path, b"{broken user json")?;
        assert!(load_or_create(&directory).is_err());
        assert_eq!(fs::read(&path)?, b"{broken user json");
        fs::remove_dir_all(directory)?;
        Ok(())
    }
    #[test]
    fn migration_keeps_existing_preferences_and_original_file() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("altaed-config-migration-{}", uuid::Uuid::new_v4()));
        let old = AppPreferences {
            editor: crate::configuration::EditorPreferences {
                font_size: 18.,
                ..Default::default()
            },
            ..Default::default()
        };
        let legacy = directory.join("application-preferences.json");
        fs::create_dir_all(&directory)?;
        fs::write(
            &legacy,
            serde_json::to_vec_pretty(&serde_json::json!({"version":1,"value":old}))?,
        )?;
        let bytes = fs::read(&legacy)?;
        let migrated = load_or_create(&directory)?;
        assert_eq!(migrated.preferences.editor.font_size, 18.);
        assert!(!migrated.preferences.keybindings.is_empty());
        assert_eq!(fs::read(&legacy)?, bytes);
        fs::remove_dir_all(directory)?;
        Ok(())
    }
    #[test]
    fn setup_resumes_until_completed_and_legacy_command_is_preserved() -> Result<()> {
        let directory = std::env::temp_dir().join(format!("altaed-setup-{}", uuid::Uuid::new_v4()));
        let first = load_or_create(&directory)?.preferences;
        assert!(!first.toolchain.setup_complete);
        assert!(
            !load_or_create(&directory)?
                .preferences
                .toolchain
                .setup_complete
        );
        let mut completed = first.clone();
        completed.toolchain.compiler_path = Some("/selected/compiler".into());
        completed.toolchain.setup_complete = true;
        save(&directory, &first, &completed)?;
        assert!(
            load_or_create(&directory)?
                .preferences
                .toolchain
                .setup_complete
        );
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join(FILE_NAME))?)?;
        json.as_object_mut().unwrap().remove("toolchain");
        json["keybindings"]
            .as_object_mut()
            .unwrap()
            .remove("run.compile_and_run");
        json["keybindings"]["run.terminal"] = "ctrl-alt-r".into();
        fs::write(directory.join(FILE_NAME), serde_json::to_vec(&json)?)?;
        let migrated = load(&directory)?;
        assert!(migrated.toolchain.setup_complete);
        assert_eq!(
            migrated.keybindings[&commands::CommandId("run.compile_and_run".into())],
            "ctrl-alt-r"
        );
        assert!(
            !migrated
                .keybindings
                .contains_key(&commands::CommandId("run.terminal".into()))
        );
        fs::remove_dir_all(directory)?;
        Ok(())
    }
    #[test]
    fn concurrent_writers_cannot_overwrite_the_same_revision() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("altaed-config-lock-{}", uuid::Uuid::new_v4()));
        let original = load_or_create(&directory)?.preferences;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let writers: Vec<_> = [18., 20.]
            .into_iter()
            .map(|font_size| {
                let directory = directory.clone();
                let original = original.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut next = original.clone();
                    next.editor.font_size = font_size;
                    barrier.wait();
                    save(&directory, &original, &next)
                })
            })
            .collect();
        barrier.wait();
        let results: Vec<_> = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(matches!(load(&directory)?.editor.font_size, 18. | 20.));
        fs::remove_dir_all(directory)?;
        Ok(())
    }
    #[test]
    fn global_preferences_persist_and_reject_stale_or_invalid_writes() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("altaed-preferences-{}", uuid::Uuid::new_v4()));
        let initial = load_or_create(&directory)?.preferences;
        let mut next = initial.clone();
        next.editor.font_size = 18.;
        next.workbench.documents = true;
        save(&directory, &initial, &next)?;
        assert_eq!(load(&directory)?, next);
        assert!(save(&directory, &initial, &initial).is_err());
        next.editor.font_size = f32::NAN;
        assert!(save(&directory, &load(&directory)?, &next).is_err());
        std::fs::write(directory.join(FILE_NAME), "{\"version\":999,\"value\":{}}")?;
        assert!(load(&directory).is_err());
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }
}
