//! GPUI adapter for the command catalog. Reload replaces the entire managed map,
//! so disabling a shortcut cannot leave an earlier binding active.
use crate::{configuration::AppPreferences, configuration::commands};
use anyhow::{Context, Result, ensure};
use gpui::{App, DummyKeyboardMapper, Global, KeyBinding, KeyBindingContextPredicate};
use settings::{KeymapFile, KeymapFileLoadResult};

struct KeymapSource {
    editor_bindings: Vec<KeyBinding>,
}
impl Global for KeymapSource {}
gpui::actions!(altaed_keymap, [DisabledShortcut]);

pub fn init(preferences: &AppPreferences, cx: &mut App) -> Result<()> {
    cx.on_action(|_: &DisabledShortcut, cx| cx.stop_propagation());
    let bindings = match KeymapFile::load(settings::default_keymap().as_ref(), cx) {
        KeymapFileLoadResult::Success { key_bindings } => key_bindings,
        KeymapFileLoadResult::SomeFailedToLoad {
            key_bindings,
            error_message,
        } => {
            eprintln!("部分 Zed 工作台命令不适用于独立 IDE：{error_message}");
            key_bindings
        }
        KeymapFileLoadResult::JsonParseFailure { error } => {
            anyhow::bail!("编辑器默认快捷键无效：{error}")
        }
    };
    let editor_bindings = bindings
        .into_iter()
        .filter(|binding| {
            let name = binding.action().name();
            name.starts_with("editor::") || name.starts_with("menu::")
        })
        .collect();
    cx.set_global(KeymapSource { editor_bindings });
    apply(preferences, cx)
}
fn load_binding(
    command: &commands::CommandDefinition,
    preferences: &AppPreferences,
    cx: &App,
) -> Result<Option<KeyBinding>> {
    let shortcut = command.effective_shortcut(&preferences.keybindings).trim();
    if shortcut.is_empty() {
        return Ok(None);
    }
    let action = cx
        .build_action(&command.action, None)
        .with_context(|| format!("无法加载命令 {}", command.id.0))?;
    let predicate = command
        .context
        .as_deref()
        .map(KeyBindingContextPredicate::parse)
        .transpose()?
        .map(Into::into);
    let binding = KeyBinding::load(
        shortcut,
        action,
        predicate,
        false,
        None,
        &DummyKeyboardMapper,
    )
    .with_context(|| format!("{}的快捷键无效：{shortcut}", command.label))?;
    ensure!(
        !binding.keystrokes().is_empty(),
        "{}的快捷键不能为空组合",
        command.label
    );
    Ok(Some(binding))
}
fn conflicting(
    first: &commands::CommandDefinition,
    first_binding: &KeyBinding,
    second: &commands::CommandDefinition,
    second_binding: &KeyBinding,
) -> bool {
    (first.context == second.context || first.context.is_none() || second.context.is_none())
        && (first_binding
            .keystrokes()
            .starts_with(second_binding.keystrokes())
            || second_binding
                .keystrokes()
                .starts_with(first_binding.keystrokes()))
}
fn validate_bindings(
    preferences: &AppPreferences,
    cx: &App,
) -> Result<(Vec<KeyBinding>, std::collections::BTreeMap<String, String>)> {
    let mut bindings: Vec<(commands::CommandDefinition, KeyBinding)> = Vec::new();
    let mut errors = std::collections::BTreeMap::new();
    for command in commands::definitions()? {
        let binding = match load_binding(&command, preferences, cx) {
            Ok(Some(binding)) => binding,
            Ok(None) => continue,
            Err(error) => {
                errors.insert(command.setting_id(), format!("{error:#}"));
                continue;
            }
        };
        for (previous, previous_binding) in &bindings {
            if conflicting(previous, previous_binding, &command, &binding) {
                let error = format!(
                    "快捷键冲突：“{}”与“{}”使用相同或互为前缀的按键",
                    previous.label, command.label
                );
                errors.insert(previous.setting_id(), error.clone());
                errors.insert(command.setting_id(), error);
            }
        }
        bindings.push((command, binding));
    }
    Ok((
        bindings.into_iter().map(|(_, binding)| binding).collect(),
        errors,
    ))
}
pub fn field_errors(
    preferences: &AppPreferences,
    cx: &App,
) -> Result<std::collections::BTreeMap<String, String>> {
    Ok(validate_bindings(preferences, cx)?.1)
}
pub fn compile(preferences: &AppPreferences, cx: &App) -> Result<Vec<KeyBinding>> {
    let (bindings, errors) = validate_bindings(preferences, cx)?;
    if let Some(error) = errors.values().next() {
        anyhow::bail!("{error}");
    }
    Ok(bindings)
}
pub fn apply(preferences: &AppPreferences, cx: &mut App) -> Result<()> {
    let bindings = compile(preferences, cx)?;
    let Some(source) = cx.try_global::<KeymapSource>() else {
        return Ok(());
    };
    // Configured global/editor commands replace matching inherited editor keys.
    // Scoped form bindings leave the source editor's Enter/Tab behavior intact.
    let definitions = commands::definitions()?;
    let reserved_defaults = compile(&AppPreferences::default(), cx)?;
    let managed_editor_actions: std::collections::BTreeSet<_> = definitions
        .iter()
        .filter(|definition| definition.context.as_deref() == Some("Editor"))
        .map(|definition| definition.action.as_str())
        .collect();
    let overriding_actions: std::collections::BTreeSet<_> = definitions
        .iter()
        .filter(|definition| {
            definition.context.is_none() || definition.context.as_deref() == Some("Editor")
        })
        .map(|definition| definition.action.as_str())
        .collect();
    let base: Vec<_> = source
        .editor_bindings
        .iter()
        .filter(|base| {
            if managed_editor_actions.contains(base.action().name()) {
                return false;
            }
            !bindings.iter().chain(&reserved_defaults).any(|binding| {
                overriding_actions.contains(binding.action().name())
                    && (base.keystrokes().starts_with(binding.keystrokes())
                        || binding.keystrokes().starts_with(base.keystrokes()))
            })
        })
        .cloned()
        .collect();
    let disabled = scoped_disabled_defaults(&definitions, preferences, &bindings)?;
    cx.clear_key_bindings();
    cx.bind_keys(base);
    cx.bind_keys(disabled);
    cx.bind_keys(bindings);
    Ok(())
}
fn scoped_disabled_defaults(
    definitions: &[commands::CommandDefinition],
    preferences: &AppPreferences,
    current: &[KeyBinding],
) -> Result<Vec<KeyBinding>> {
    let mut disabled = Vec::new();
    for command in definitions {
        let Some(context) = command
            .context
            .as_deref()
            .filter(|context| *context != "Editor")
        else {
            continue;
        };
        let default = command.default_shortcut();
        if default.is_empty() || default == command.effective_shortcut(&preferences.keybindings) {
            continue;
        }
        let predicate = Some(KeyBindingContextPredicate::parse(context)?.into());
        let shadow = KeyBinding::load(
            default,
            Box::new(DisabledShortcut),
            predicate,
            false,
            None,
            &DummyKeyboardMapper,
        )?;
        // A key reassigned to another explicit command must remain available.
        if !current
            .iter()
            .any(|binding| binding.keystrokes() == shadow.keystrokes())
        {
            disabled.push(shadow);
        }
    }
    Ok(disabled)
}
#[cfg(test)]
pub fn bind_test_defaults(cx: &mut App) -> Result<()> {
    cx.bind_keys(compile(&AppPreferences::default(), cx)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::Ide, session::Session};
    use std::sync::Arc;
    #[gpui::test]
    fn dev_cpp_function_keys_dispatch_while_editing(cx: &mut gpui::TestAppContext) {
        use gpui::Focusable;
        use std::{cell::Cell, rc::Rc};
        let calls = Rc::new(Cell::new([0; 3]));
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            init(&AppPreferences::default(), cx).unwrap();
            let compiled = calls.clone();
            cx.on_action(move |_: &crate::app::CompileDocument, cx| {
                let mut counts = compiled.get();
                counts[0] += 1;
                compiled.set(counts);
                cx.stop_propagation();
            });
            let ran = calls.clone();
            cx.on_action(move |_: &crate::app::RunCompiled, cx| {
                let mut counts = ran.get();
                counts[1] += 1;
                ran.set(counts);
                cx.stop_propagation();
            });
            let combined = calls.clone();
            cx.on_action(move |_: &crate::app::CompileAndRun, cx| {
                let mut counts = combined.get();
                counts[2] += 1;
                combined.set(counts);
                cx.stop_propagation();
            });
        });
        let (editor, cx) = cx.add_window_view(editor::Editor::single_line);
        cx.update(|window, cx| window.focus(&editor.focus_handle(cx), cx));
        for (key, expected) in [("f9", [1, 0, 0]), ("f10", [1, 1, 0]), ("f11", [1, 1, 1])] {
            cx.simulate_keystrokes(key);
            assert_eq!(
                calls.get(),
                expected,
                "{key} must dispatch exactly its configured command"
            );
        }
    }
    #[gpui::test]
    fn rebinding_disables_old_keys_and_rejects_conflicts(cx: &mut gpui::TestAppContext) {
        let mut preferences = AppPreferences::default();
        preferences.language_server.enabled = false;
        preferences
            .keybindings
            .insert(commands::CommandId("file.new".into()), "ctrl-alt-n".into());
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            init(&preferences, cx).unwrap();
        });
        let language = Arc::new(language::Language::new(
            toml::from_str(include_str!("../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let directory = std::env::temp_dir().join(format!("altaed-keys-{}", uuid::Uuid::new_v4()));
        let (ide, cx) = cx.add_window_view(|window, cx| {
            let mut session = Session::default();
            session.ensure_document();
            let mut ide = Ide::new(session, directory, language, window, cx);
            ide.set_preferences(preferences.clone(), cx);
            ide
        });
        let default =
            commands::default_shortcuts().unwrap()[&commands::CommandId("file.new".into())].clone();
        cx.simulate_keystrokes(&default);
        assert_eq!(ide.read_with(cx, |ide, _| ide.document_count()), 1);
        cx.simulate_keystrokes("ctrl-alt-n");
        assert_eq!(ide.read_with(cx, |ide, _| ide.document_count()), 2);
        preferences
            .keybindings
            .insert(commands::CommandId("file.new".into()), "ctrl-alt-m".into());
        cx.update(|_, cx| apply(&preferences, cx).unwrap());
        cx.simulate_keystrokes("ctrl-alt-n");
        assert_eq!(ide.read_with(cx, |ide, _| ide.document_count()), 2);
        cx.simulate_keystrokes("ctrl-alt-m");
        assert_eq!(ide.read_with(cx, |ide, _| ide.document_count()), 3);
        preferences
            .keybindings
            .insert(commands::CommandId("file.new".into()), String::new());
        cx.update(|_, cx| apply(&preferences, cx).unwrap());
        cx.simulate_keystrokes("ctrl-alt-m");
        assert_eq!(ide.read_with(cx, |ide, _| ide.document_count()), 3);
        preferences
            .keybindings
            .insert(commands::CommandId("file.save".into()), "hyper-x".into());
        cx.update(|_, cx| {
            assert!(compile(&preferences, cx).is_err());
            assert!(
                field_errors(&preferences, cx)
                    .unwrap()
                    .contains_key("core.keybindings.file.save")
            );
        });
        preferences
            .keybindings
            .insert(commands::CommandId("file.save".into()), "ctrl-o".into());
        preferences
            .keybindings
            .insert(commands::CommandId("file.open".into()), "ctrl-o".into());
        cx.update(|_, cx| assert!(compile(&preferences, cx).is_err()));
        preferences
            .keybindings
            .remove(&commands::CommandId("file.save".into()));
        preferences
            .keybindings
            .remove(&commands::CommandId("file.open".into()));
        preferences
            .keybindings
            .insert(commands::CommandId("edit.undo".into()), "ctrl-alt-z".into());
        cx.update(|_, cx| {
            apply(&preferences, cx).unwrap();
            let map = cx.key_bindings();
            assert_eq!(
                map.borrow()
                    .bindings_for_action(&editor::actions::Undo)
                    .count(),
                1
            );
        });
        preferences
            .keybindings
            .insert(commands::CommandId("edit.undo".into()), String::new());
        cx.update(|_, cx| {
            apply(&preferences, cx).unwrap();
            assert_eq!(
                cx.key_bindings()
                    .borrow()
                    .bindings_for_action(&editor::actions::Undo)
                    .count(),
                0
            );
        });
    }
}
