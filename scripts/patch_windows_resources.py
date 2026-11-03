"""Keep Windows debug resources portable outside the build machine.

The pinned upstream compiles debug HLSL from an absolute checkout path at
runtime. Embed both entry sources and their shared header instead; release
continues to use upstream's build-time bytecode compilation.
"""
from pathlib import Path


def apply(snapshot: Path, replace_once):
    replace_once(snapshot / "crates/assets/Cargo.toml", "rust-embed.workspace = true", """rust-embed.workspace = true

# Windows development artifacts are distributed outside the source checkout.
[target.'cfg(target_os = "windows")'.dependencies]
rust-embed = { workspace = true, features = ["debug-embed"] }""")
    renderer = snapshot / "crates/gpui_windows/src/directx_renderer.rs"
    replace_once(renderer,
        "Fxc::{D3DCOMPILE_DEBUG, D3DCOMPILE_SKIP_OPTIMIZATION, D3DCompileFromFile},",
        "Fxc::{D3DCOMPILE_DEBUG, D3DCOMPILE_SKIP_OPTIMIZATION, D3DCompile},")
    replace_once(renderer, "core::{HSTRING, PCSTR},", "core::PCSTR,")
    replace_once(renderer, '''            use windows::Win32::Graphics::{
                Direct3D::ID3DInclude, Hlsl::D3D_COMPILE_STANDARD_FILE_INCLUDE,
            };

            let shader_name = if matches!(entry, ShaderModule::EmojiRasterization) {
                "color_text_raster.hlsl"
            } else {
                "shaders.hlsl"
            };''', '''            // Debug artifacts are also downloaded and run on other machines.
            // Keep shader inputs in the binary, independent of checkout paths.
            let source = if matches!(entry, ShaderModule::EmojiRasterization) {
                include_str!("color_text_raster.hlsl")
            } else {
                include_str!("shaders.hlsl")
            };
            let source = source.replace(
                "#include \\\"alpha_correction.hlsl\\\"",
                include_str!("alpha_correction.hlsl"),
            );''')
    replace_once(renderer, '''            let shader_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(&format!("src/{}", shader_name))
                .canonicalize()?;
''', "            // The source is embedded above; no filesystem lookup is needed.\n")
    replace_once(renderer, '''            // really dirty trick because winapi bindings are unhappy otherwise
            let include_handler = &std::mem::transmute::<usize, ID3DInclude>(
                D3D_COMPILE_STANDARD_FILE_INCLUDE as usize,
            );

            let ret = D3DCompileFromFile(
                &HSTRING::from(shader_path.to_str().unwrap()),
                None,
                include_handler,''', '''            let ret = D3DCompile(
                source.as_ptr().cast(),
                source.len(),
                PCSTR::null(),
                None,
                None,''')
