//! IDE commands hosted in the common native title bar.
use super::*;
use gpui::AnyElement;
use ui::prelude::*;
impl Ide {
    pub(super) fn render_title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.weak_entity();
        let settings = ui::PopoverMenu::new("global-quick-settings")
            .trigger(
                crate::panel_style::button("global-settings-trigger", "设置", cx)
                    .icon(ui::IconName::ChevronDown)
                    .icon_position(ui::IconPosition::End)
                    .icon_size(ui::IconSize::Small)
                    .tooltip(ui::Tooltip::text("全局快捷设置")),
            )
            .menu(move |window, cx| {
                let owner = owner.clone();
                Some(ui::ContextMenu::build(window, cx, move |mut menu, _, _| {
                    for (label, category) in [
                        ("颜色主题", "core.editor"),
                        ("编译环境", "core.toolchain"),
                        ("快捷键", "core.keybindings"),
                    ] {
                        let owner = owner.clone();
                        menu = menu.entry(label, None, move |_, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.show_settings_category(
                                    settings_window::Scope::Application,
                                    Some(category.into()),
                                    cx,
                                )
                            });
                        });
                    }
                    let settings_owner = owner.clone();
                    menu.separator()
                        .entry("全部设置…", None, move |_, cx| {
                            let _ = settings_owner.update(cx, |this, cx| {
                                this.show_settings(settings_window::Scope::Application, cx)
                            });
                        })
                        .separator()
                        .item(ui::ContextMenuEntry::new("登录（尚未开放）").disabled(true))
                }))
            })
            .anchor(gpui::Corner::TopRight);
        let open = crate::panel_style::button("title-open-file", "打开文件…", cx).on_click(
            cx.listener(|this, _, window, cx| this.open_document(&OpenDocument, window, cx)),
        );
        self.window_chrome.update(cx, |chrome, _| {
            chrome.content(open.into_any_element(), settings.into_any_element())
        });
        self.window_chrome.clone().into_any_element()
    }
}
