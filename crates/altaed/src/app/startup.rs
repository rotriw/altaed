//! Keep onboarding outside the editor window. Its saved completion state also
//! records a deliberate choice to configure the toolchain later.
use super::*;
use crate::configuration::AppPreferences;
use anyhow::Result;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};

pub(super) struct Startup {
    pub session: Session,
    pub directory: PathBuf,
    pub language: Arc<Language>,
    pub preferences: AppPreferences,
}
pub(crate) fn launch(
    session: Session,
    directory: PathBuf,
    language: Arc<Language>,
    preferences: AppPreferences,
    cx: &mut App,
) -> Result<()> {
    let startup = Startup {
        session,
        directory,
        language,
        preferences,
    };
    if startup.preferences.toolchain.setup_complete {
        startup.open_ide(cx)
    } else {
        super::welcome::open(startup, cx)
    }
}
impl Startup {
    pub fn open_ide(&self, cx: &mut App) -> Result<()> {
        let bounds = Bounds::centered(None, size(px(1250.), px(800.)), cx);
        let session = self.session.clone();
        let directory = self.directory.clone();
        let language = self.language.clone();
        let preferences = self.preferences.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(900.), px(500.))),
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title("altaed");
                window.on_window_should_close(cx, |_, cx| {
                    cx.quit();
                    false
                });
                theme::setup_ui_font(window, cx);
                cx.new(|cx| {
                    let mut ide = Ide::new(session, directory, language, window, cx);
                    ide.set_preferences(preferences, cx);
                    ide
                })
            },
        )?;
        Ok(())
    }
}
