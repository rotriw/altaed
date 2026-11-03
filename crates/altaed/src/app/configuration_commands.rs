use super::*;

impl Ide {
    pub(super) fn reveal_configuration(
        &mut self,
        _: &RevealConfiguration,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.reveal_path(&self.directory.join(crate::configuration::FILE_NAME));
    }
    pub(super) fn reload_configuration(
        &mut self,
        _: &ReloadConfiguration,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = self.directory.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { crate::configuration::load(&directory) })
                .await;
            this.update(cx, |this, cx| {
                match result.and_then(|preferences| {
                    crate::keymap::compile(&preferences, cx)?;
                    Ok(preferences)
                }) {
                    Ok(preferences) => {
                        this.set_preferences(preferences, cx);
                        this.status = StatusMessage::General("已重新载入 config.json".into());
                    }
                    Err(error) => {
                        this.status = StatusMessage::General(format!("配置未载入：{error:#}"))
                    }
                }
                cx.notify();
            })
        })
        .detach_and_log_err(cx);
    }
}
