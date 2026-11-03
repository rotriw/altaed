"""Small, checked extension points kept separate from the pinned Zed source.

No upstream workspace is initialized by altaed. Signature help uses the native
editor popover through a provider, just as completion already does upstream.
"""
from pathlib import Path
from patch_windows_resources import apply as patch_windows_resources


def replace_once(path: Path, before: str, after: str):
    # An output that is a substring of the input cannot identify an applied
    # patch (e.g. D3DCompile also matches D3DCompileFromFile). Require delimiters.
    if after in before:
        raise ValueError("Patch output must not be a substring of its input")
    source = path.read_text(encoding="utf-8")
    if after in source:
        return
    if source.count(before) != 1:
        raise RuntimeError(f"Pinned editor extension no longer applies: {path}")
    path.write_text(source.replace(before, after, 1), encoding="utf-8", newline="\n")


def apply(snapshot: Path):
    patch_windows_resources(snapshot, replace_once)
    editor = snapshot / "crates/editor/src/editor.rs"
    replace_once(editor,
        "    completion_provider: Option<Rc<dyn CompletionProvider>>,",
        "    completion_provider: Option<Rc<dyn CompletionProvider>>,\n    signature_help_provider: Option<Rc<dyn SignatureHelpProvider>>,")
    replace_once(editor,
        "            completion_provider: project.clone().map(|project| Rc::new(project) as _),",
        "            completion_provider: project.clone().map(|project| Rc::new(project) as _),\n            signature_help_provider: None,")
    replace_once(editor, "    pub fn set_completion_provider(&mut self, provider: Option<Rc<dyn CompletionProvider>>) {",
        "    pub fn set_signature_help_provider(&mut self, provider: Option<Rc<dyn SignatureHelpProvider>>) {\n        self.signature_help_provider = provider;\n    }\n\n    pub fn set_completion_provider(&mut self, provider: Option<Rc<dyn CompletionProvider>>) {")
    replace_once(snapshot / "crates/editor/src/signature_help.rs", "    pub fn auto_signature_help_enabled(&self, cx: &App) -> bool {", """    pub fn set_auto_signature_help(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.auto_signature_help = Some(enabled);
        if !enabled {
            self.hide_signature_help(cx, SignatureHelpHiddenBy::AutoClose);
        }
    }

    pub fn auto_signature_help_enabled(&self, cx: &App) -> bool {""")
    replace_once(editor, "pub trait CompletionProvider {", """pub trait SignatureHelpProvider {
    fn signature_help(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        cx: &mut App,
    ) -> Task<Option<Vec<project::lsp_command::SignatureHelp>>>;
}

pub trait CompletionProvider {""")
    replace_once(editor, "(editor_settings.auto_signature_help\n                    || editor_settings.show_signature_help_after_edits)",
        "(this.auto_signature_help_enabled(cx)\n                    || editor_settings.show_signature_help_after_edits)")
    replace_once(editor, "if editor_settings.show_signature_help_after_edits || editor_settings.auto_signature_help {",
        "if editor_settings.show_signature_help_after_edits || self.auto_signature_help_enabled(cx) {")
    replace_once(snapshot / "crates/editor/src/signature_help.rs", """        let Some(lsp_store) = self.project().map(|p| p.read(cx).lsp_store()) else {
            return;
        };
        let lsp_task = lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.signature_help(&buffer, buffer_position, cx)
        });""", """        let lsp_task = if let Some(provider) = &self.signature_help_provider {
            provider.signature_help(&buffer, buffer_position, cx)
        } else if let Some(lsp_store) = self.project().map(|p| p.read(cx).lsp_store()) {
            lsp_store.update(cx, |lsp_store, cx| {
                lsp_store.signature_help(&buffer, buffer_position, cx)
            })
        } else {
            return;
        };""")


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot", type=Path)
    apply(parser.parse_args().snapshot)
