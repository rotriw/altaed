//! Activity contributions describe navigation, not storage providers. A future
//! cloud provider supplies its own pane without changing document identities or
//! making this view responsible for synchronization/network operations.
use super::*;

pub(super) use crate::workbench_layout::ACTIVITY_BAR_WIDTH;

struct ActivityContribution {
    id: &'static str,
    label: &'static str,
    icon: IconName,
    command: Option<fn() -> Box<dyn gpui::Action>>,
}
const ACTIVITIES: &[ActivityContribution] = &[
    ActivityContribution {
        id: "core.resources",
        label: "资源",
        icon: IconName::Copy,
        command: Some(|| Box::new(ToggleDocuments)),
    },
    ActivityContribution {
        id: "cloud.storage",
        label: "云存储尚未接入",
        icon: IconName::CloudDownload,
        command: None,
    },
];

pub(crate) struct SidebarState {
    files_expanded: bool,
    drafts_expanded: bool,
}
impl Default for SidebarState {
    fn default() -> Self {
        Self {
            files_expanded: true,
            drafts_expanded: true,
        }
    }
}

impl Ide {
    pub(super) fn render_activity_bar(&self, cx: &Context<Self>) -> AnyElement {
        let c = crate::panel_style::palette(cx);
        div()
            .w(px(ACTIVITY_BAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(c.subtle)
            .border_r_1()
            .border_color(c.line)
            .py(px(3.))
            .children(ACTIVITIES.iter().map(|activity| {
                let active = activity.id == "core.resources" && self.layout.documents_visible;
                let icon = Icon::new(activity.icon)
                    .size(IconSize::Custom(gpui::rems(1.25)))
                    .color(Color::Custom(c.muted));
                div()
                    .relative()
                    .h(px(40.))
                    .w_full()
                    .flex_shrink_0()
                    .when(active, |row| {
                        row.child(
                            div()
                                .absolute()
                                .left_0()
                                .top(px(10.))
                                .bottom(px(10.))
                                .w(px(1.))
                                .bg(c.accent),
                        )
                    })
                    .child(if let Some(command) = activity.command {
                        crate::panel_style::form_button(activity.id, "", 40., c)
                            .w_full()
                            .text_color(if active { c.accent } else { c.muted })
                            .tooltip(move |_, cx| {
                                Tooltip::for_action(activity.label, &ToggleDocuments, cx)
                            })
                            .child(icon)
                            .on_click(move |_, window, cx| window.dispatch_action(command(), cx))
                            .into_any_element()
                    } else {
                        div()
                            .id(activity.id)
                            .h_full()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(c.muted)
                            .opacity(0.55)
                            .child(icon)
                            .tooltip(Tooltip::text(activity.label))
                            .into_any_element()
                    })
            }))
            .child(div().flex_1())
            .child(
                crate::panel_style::form_button("activity-settings", "", 40., c)
                    .w_full()
                    .child(
                        Icon::new(IconName::Settings)
                            .size(IconSize::Custom(gpui::rems(1.25)))
                            .color(Color::Custom(c.muted)),
                    )
                    .tooltip(|_, cx| Tooltip::for_action("软件设置", &OpenSettings, cx))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)),
            )
            .into_any_element()
    }

    pub(super) fn render_resources(
        &self,
        snapshot: &Session,
        width: Pixels,
        cx: &Context<Self>,
    ) -> AnyElement {
        let c = crate::panel_style::palette(cx);
        let header_action = |id, icon, title, action: Box<dyn gpui::Action>| {
            let tip = action.boxed_clone();
            crate::panel_style::form_button(id, "", 28., c)
                .w(px(28.))
                .px_0()
                .child(
                    Icon::new(icon)
                        .size(IconSize::Small)
                        .color(Color::Custom(c.muted)),
                )
                .tooltip(move |_, cx| Tooltip::for_action(title, tip.as_ref(), cx))
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
        };
        div()
            .id("resources-sidebar")
            .debug_selector(|| "resources-sidebar".into())
            .w(width)
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(c.paper)
            .child(
                div()
                    .h(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pl(px(12.))
                    .pr(px(8.))
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(c.text)
                    .child("资源")
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(header_action(
                                "resource-new",
                                IconName::Plus,
                                "新建文件",
                                Box::new(NewDocument),
                            ))
                            .child(header_action(
                                "resource-open",
                                IconName::FolderOpen,
                                "打开文件",
                                Box::new(OpenDocument),
                            )),
                    ),
            )
            .child(
                div()
                    .id("resource-sections")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.resource_section(
                        "resource-files",
                        "打开的文件",
                        snapshot.documents.len(),
                        false,
                        cx,
                    ))
                    .when(self.sidebar.files_expanded, |list| {
                        list.children(snapshot.documents.iter().map(|document| {
                            let id = document.id;
                            self.resource_row(
                                document,
                                id == snapshot.documents[snapshot.active].id,
                                "resource-file",
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if this.busy {
                                        return;
                                    }
                                    if let Some(index) = this
                                        .session
                                        .documents
                                        .iter()
                                        .position(|document| document.id == id)
                                    {
                                        this.session.active = index;
                                        window
                                            .focus(&this.views[index].editor.focus_handle(cx), cx);
                                        cx.notify();
                                    }
                                },
                            ))
                        }))
                    })
                    .child(self.resource_section(
                        "resource-drafts",
                        "保留的草稿",
                        self.retained.len(),
                        true,
                        cx,
                    ))
                    .when(self.sidebar.drafts_expanded, |list| {
                        list.when(self.retained.is_empty(), |list| {
                            list.child(
                                div()
                                    .h(px(28.))
                                    .pl(px(24.))
                                    .text_size(px(11.))
                                    .text_color(c.muted)
                                    .child("暂无保留草稿"),
                            )
                        })
                        .children(self.retained.iter().map(|document| {
                            let id = document.id;
                            self.resource_row(document, false, "resource-draft", cx)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this.busy {
                                        return;
                                    }
                                    if let Some(index) =
                                        this.retained.iter().position(|document| document.id == id)
                                    {
                                        let document = this.retained.remove(index);
                                        this.append(document, window, cx);
                                    }
                                }))
                        }))
                    }),
            )
            .child(
                div()
                    .h(px(34.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px(px(12.))
                    .text_size(px(11.))
                    .text_color(c.muted)
                    .child("本地文件与草稿"),
            )
            .into_any_element()
    }

    fn resource_section(
        &self,
        id: &'static str,
        title: &'static str,
        count: usize,
        drafts: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let c = crate::panel_style::palette(cx);
        let expanded = if drafts {
            self.sidebar.drafts_expanded
        } else {
            self.sidebar.files_expanded
        };
        crate::panel_style::form_button(id, "", 28., c)
            .w_full()
            .pl(px(8.))
            .pr(px(10.))
            .justify_between()
            .text_size(px(11.))
            .text_color(c.text)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::Custom(gpui::rems(0.75)))
                        .color(Color::Custom(c.muted)),
                    )
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title)),
            )
            .child(div().text_color(c.muted).child(count.to_string()))
            .on_click(cx.listener(move |this, _, _, cx| {
                let expanded = if drafts {
                    &mut this.sidebar.drafts_expanded
                } else {
                    &mut this.sidebar.files_expanded
                };
                *expanded = !*expanded;
                cx.notify();
            }))
            .into_any_element()
    }

    fn resource_row(
        &self,
        document: &CodeDocument,
        active: bool,
        section: &'static str,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let c = crate::panel_style::palette(cx);
        let path = document
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "未保存到磁盘".into());
        crate::panel_style::form_button(
            gpui::SharedString::from(format!("{section}-{}", document.id)),
            "",
            28.,
            c,
        )
        .w_full()
        .pl(px(24.))
        .pr(px(12.))
        .justify_start()
        .gap(px(8.))
        .text_color(c.text)
        .when(active, |row| row.bg(c.selection))
        .child(
            Icon::new(IconName::Code)
                .size(IconSize::Custom(gpui::rems(0.875)))
                .color(Color::Muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(document.display_name()),
        )
        .when(document.is_dirty(), |row| {
            row.child(
                div()
                    .w(px(5.))
                    .h(px(5.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(c.accent),
            )
        })
        .tooltip(Tooltip::text(path))
    }
}
