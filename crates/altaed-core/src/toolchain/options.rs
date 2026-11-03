//! Common GCC/Clang switches shared by execution and clangd. Advanced arguments
//! remain available; selecting an optimization level replaces only -O switches.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Optimization {
    #[default]
    Inherit,
    None,
    Debug,
    Level1,
    Level2,
    Level3,
    Size,
}
impl Optimization {
    pub const ALL: [Self; 7] = [
        Self::Inherit,
        Self::None,
        Self::Debug,
        Self::Level1,
        Self::Level2,
        Self::Level3,
        Self::Size,
    ];
    pub fn value(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::None => "none",
            Self::Debug => "debug",
            Self::Level1 => "level1",
            Self::Level2 => "level2",
            Self::Level3 => "level3",
            Self::Size => "size",
        }
    }
    pub fn argument(self) -> Option<&'static str> {
        match self {
            Self::Inherit => None,
            Self::None => Some("-O0"),
            Self::Debug => Some("-Og"),
            Self::Level1 => Some("-O1"),
            Self::Level2 => Some("-O2"),
            Self::Level3 => Some("-O3"),
            Self::Size => Some("-Os"),
        }
    }
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        Self::ALL
            .into_iter()
            .find(|option| option.value() == value)
            .ok_or_else(|| anyhow::anyhow!("未知的优化级别：{value}"))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CompilerOptions {
    pub optimization: Optimization,
    pub warnings: bool,
    pub debug_info: bool,
    pub address_sanitizer: bool,
    pub undefined_sanitizer: bool,
}
impl CompilerOptions {
    pub fn arguments(&self, additional: &[String]) -> Vec<String> {
        let optimization = self.optimization.argument();
        let mut arguments: Vec<_> = additional
            .iter()
            .filter(|argument| optimization.is_none() || !is_optimization(argument))
            .cloned()
            .collect();
        if let Some(option) = optimization {
            arguments.push(option.into());
        }
        if self.warnings {
            arguments.extend(["-Wall".into(), "-Wextra".into()]);
        }
        if self.debug_info {
            arguments.push("-g".into());
        }
        if self.address_sanitizer {
            arguments.push("-fsanitize=address".into());
        }
        if self.undefined_sanitizer {
            arguments.push("-fsanitize=undefined".into());
        }
        if self.address_sanitizer || self.undefined_sanitizer {
            arguments.push("-fno-omit-frame-pointer".into());
        }
        arguments
    }
}
fn is_optimization(argument: &str) -> bool {
    matches!(
        argument,
        "-O" | "-O0" | "-O1" | "-O2" | "-O3" | "-Og" | "-Os" | "-Oz" | "-Ofast"
    ) || argument
        .strip_prefix("-O")
        .is_some_and(|level| !level.is_empty() && level.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_optimization_preserves_other_flags_and_sanitizers_are_additive() {
        let additional = vec![
            "-O2".into(),
            "-O3".into(),
            "-DLOCAL".into(),
            "-Iincludes with spaces".into(),
        ];
        assert_eq!(
            CompilerOptions::default().arguments(&additional),
            additional
        );
        let options = CompilerOptions {
            optimization: Optimization::Debug,
            warnings: true,
            debug_info: true,
            address_sanitizer: true,
            undefined_sanitizer: true,
        };
        let arguments = options.arguments(&additional);
        assert!(!arguments.contains(&"-O2".into()) && !arguments.contains(&"-O3".into()));
        for expected in [
            "-Og",
            "-g",
            "-Wall",
            "-Wextra",
            "-fsanitize=address",
            "-fsanitize=undefined",
            "-fno-omit-frame-pointer",
            "-DLOCAL",
            "-Iincludes with spaces",
        ] {
            assert!(
                arguments.iter().any(|argument| argument == expected),
                "{expected}"
            );
        }
    }
}
