//! Bundle product-owned resources alongside the pinned editor assets. Resource
//! identities are stable and never depend on the development checkout path.
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;
pub const STATUS_SUCCESS: &str = "altaed/status-success.svg";
pub const WORDMARK: &str = "altaed/welcome-wordmark.svg";
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        match path {
            STATUS_SUCCESS => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/status-success.svg"
            )))),
            WORDMARK => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/welcome-wordmark.svg"
            )))),
            _ => assets::Assets.load(path),
        }
    }
    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths = assets::Assets.list(path)?;
        paths.extend(
            [STATUS_SUCCESS, WORDMARK]
                .into_iter()
                .filter(|asset| asset.starts_with(path))
                .map(Into::into),
        );
        Ok(paths)
    }
}
