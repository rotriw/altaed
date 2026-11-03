use super::*;
mod auto_scroll;
use ui::{Indicator, Tab, TabBar, TabPosition};

#[derive(Clone)]
struct DraggedDocumentTab {
    id: DocumentId,
    owner: gpui::EntityId,
    source_index: usize,
    label: String,
    lifetime: Arc<TabDragLifetime>,
}
struct TabDragLifetime;
struct DocumentTabDragPreview {
    label: String,
}
impl Render for DocumentTabDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .max_w(px(240.))
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().tab_active_background)
            .child(
                Label::new(self.label.clone())
                    .size(LabelSize::Small)
                    .truncate(),
            )
    }
}

impl Ide {
    pub(super) fn render_document_tabs(
        &self,
        snapshot: &Session,
        cx: &Context<Self>,
    ) -> AnyElement {
        // Reuse Zed's tab geometry and active/inactive edges. Execution belongs
        // beneath the code, so opening more files never competes with actions.
        TabBar::new("document-tabs")
            .track_scroll(&self.tabs_scroll)
            .children(
                snapshot
                    .documents
                    .iter()
                    .enumerate()
                    .map(|(index, document)| {
                        let active = index == snapshot.active;
                        let owner = cx.entity_id();
                        let target = document.id;
                        let dragged = DraggedDocumentTab {
                            id: document.id,
                            owner,
                            source_index: index,
                            label: self.document_label(index),
                            lifetime: Arc::new(TabDragLifetime),
                        };
                        let view = cx.weak_entity();
                        let position = if index == 0 {
                            TabPosition::First
                        } else if index + 1 == snapshot.documents.len() {
                            TabPosition::Last
                        } else {
                            TabPosition::Middle(index.cmp(&snapshot.active))
                        };
                        Tab::new(gpui::SharedString::from(format!(
                            "document-tab-{}",
                            document.id
                        )))
                        .position(position)
                        .toggle_state(active)
                        .when(!self.busy, |tab| {
                            tab.on_drag(dragged, move |dragged, _, window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    this.start_tab_auto_scroll(
                                        Arc::downgrade(&dragged.lifetime),
                                        window,
                                        cx,
                                    )
                                });
                                cx.new(|_| DocumentTabDragPreview {
                                    label: dragged.label.clone(),
                                })
                            })
                        })
                        .drag_over::<DraggedDocumentTab>(move |style, dragged, _, cx| {
                            if dragged.owner != owner || dragged.id == target {
                                return style;
                            }
                            let style = style.border_color(crate::panel_style::palette(cx).accent);
                            if dragged.source_index < index {
                                style.border_r_2()
                            } else {
                                style.border_l_2()
                            }
                        })
                        .on_drop(cx.listener(
                            move |this, dragged: &DraggedDocumentTab, window, cx| {
                                this.reorder_document(dragged, target, window, cx);
                            },
                        ))
                        .focusable()
                        .tab_index(0_isize)
                        .start_slot::<Indicator>(
                            document
                                .is_dirty()
                                .then(|| Indicator::dot().color(Color::Muted)),
                        )
                        .end_slot(
                            IconButton::new("close-tab", IconName::Close)
                                .style(ButtonStyle::Transparent)
                                .size(ButtonSize::None)
                                .icon_size(IconSize::Small)
                                .icon_color(Color::Muted)
                                .tab_index(0_isize)
                                .disabled(self.busy)
                                .tooltip(Tooltip::text("关闭文档"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    // The enclosing tab must not select its old
                                    // index after closing/removing this document.
                                    cx.stop_propagation();
                                    this.close_document_tab(target, window, cx);
                                })),
                        )
                        .child(
                            div()
                                .id("document-tab-label")
                                .max_w(px(200.))
                                .overflow_hidden()
                                .child(
                                    Label::new(self.document_label(index))
                                        .size(LabelSize::Small)
                                        .color(if active { Color::Default } else { Color::Muted })
                                        .truncate(),
                                )
                                .tooltip(Tooltip::text(
                                    document
                                        .path
                                        .as_ref()
                                        .map(|path| path.display().to_string())
                                        .unwrap_or_else(|| "未保存到磁盘".into()),
                                )),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.activate_document(index, window, cx);
                        }))
                        .on_key_down(cx.listener(
                            move |this, event: &gpui::KeyDownEvent, window, cx| {
                                // Enter/Space activate a focused control; user command
                                // shortcuts remain supplied by the configurable keymap.
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    this.activate_document(index, window, cx);
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .on_aux_click(cx.listener(
                            move |this, event: &gpui::ClickEvent, window, cx| {
                                if event.is_middle_click() && !this.busy {
                                    cx.stop_propagation();
                                    this.close_document_tab(target, window, cx);
                                }
                            },
                        ))
                        .map(|tab| {
                            div()
                                .debug_selector(move || format!("TAB-document-tab-{index}"))
                                .flex_none()
                                .child(tab)
                        })
                    }),
            )
            .child(
                div()
                    .id("tab-drop-end")
                    .debug_selector(|| "tab-drop-end".into())
                    .min_w(px(24.))
                    .h(Tab::container_height(cx))
                    .flex_grow()
                    .child("")
                    .drag_over::<DraggedDocumentTab>(|style, _, _, cx| {
                        style
                            .border_l_2()
                            .border_color(crate::panel_style::palette(cx).accent)
                    })
                    .on_drop(
                        cx.listener(|this, dragged: &DraggedDocumentTab, window, cx| {
                            if let Some(last) = this.session.documents.last() {
                                this.reorder_document(dragged, last.id, window, cx);
                            }
                        }),
                    ),
            )
            .into_any_element()
    }

    fn reorder_document(
        &mut self,
        dragged: &DraggedDocumentTab,
        target: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || dragged.owner != cx.entity_id() {
            return;
        }
        let Some(from) = self
            .session
            .documents
            .iter()
            .position(|document| document.id == dragged.id)
        else {
            return;
        };
        let Some(to) = self
            .session
            .documents
            .iter()
            .position(|document| document.id == target)
        else {
            return;
        };
        if from == to {
            return;
        }
        let active = self.session.active;
        // Apply one permutation to the document, its buffer/view, and its edit
        // subscription. ID-keyed tests, run results, and LSP bindings stay put.
        let document = self.session.documents.remove(from);
        self.session.documents.insert(to, document);
        let view = self.views.remove(from);
        self.views.insert(to, view);
        let edits = self.edits.remove(from);
        self.edits.insert(to, edits);
        self.session.active = if active == from {
            to
        } else if from < active && active <= to {
            active - 1
        } else if to <= active && active < from {
            active + 1
        } else {
            active
        };
        self.tabs_scroll.scroll_to_item(self.session.active);
        window.focus(&self.views[self.session.active].editor.focus_handle(cx), cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
