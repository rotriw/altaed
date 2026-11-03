//! Map the same typed switches into software defaults and per-document forms.
use super::schema::Values;
use crate::toolchain::{CompilerOptions, Optimization};
use anyhow::{Context, Result};

pub fn values(prefix: &str, options: &CompilerOptions) -> Values {
    [
        ("optimization", options.optimization.value().into()),
        ("warnings", options.warnings.to_string()),
        ("debug_info", options.debug_info.to_string()),
        ("address_sanitizer", options.address_sanitizer.to_string()),
        (
            "undefined_sanitizer",
            options.undefined_sanitizer.to_string(),
        ),
    ]
    .into_iter()
    .map(|(name, value)| (format!("{prefix}.{name}"), value))
    .collect()
}
pub fn apply(prefix: &str, values: &Values) -> Result<CompilerOptions> {
    let value = |name| {
        values
            .get(&format!("{prefix}.{name}"))
            .with_context(|| format!("缺少编译选项：{name}"))
    };
    Ok(CompilerOptions {
        optimization: Optimization::parse(value("optimization")?)?,
        warnings: value("warnings")?.parse()?,
        debug_info: value("debug_info")?.parse()?,
        address_sanitizer: value("address_sanitizer")?.parse()?,
        undefined_sanitizer: value("undefined_sanitizer")?.parse()?,
    })
}
