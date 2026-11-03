//! Language mode is independent of the compiler's release version.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CppStandard {
    #[default]
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "c++98")]
    Cpp98,
    #[serde(rename = "c++03")]
    Cpp03,
    #[serde(rename = "c++11")]
    Cpp11,
    #[serde(rename = "c++14")]
    Cpp14,
    #[serde(rename = "c++17")]
    Cpp17,
    #[serde(rename = "c++20")]
    Cpp20,
    #[serde(rename = "c++23")]
    Cpp23,
    #[serde(rename = "c++26")]
    Cpp26,
}
impl CppStandard {
    pub const ALL: [Self; 9] = [
        Self::Default,
        Self::Cpp98,
        Self::Cpp03,
        Self::Cpp11,
        Self::Cpp14,
        Self::Cpp17,
        Self::Cpp20,
        Self::Cpp23,
        Self::Cpp26,
    ];

    pub fn value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Cpp98 => "c++98",
            Self::Cpp03 => "c++03",
            Self::Cpp11 => "c++11",
            Self::Cpp14 => "c++14",
            Self::Cpp17 => "c++17",
            Self::Cpp20 => "c++20",
            Self::Cpp23 => "c++23",
            Self::Cpp26 => "c++26",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "默认",
            Self::Cpp98 => "C++98",
            Self::Cpp03 => "C++03",
            Self::Cpp11 => "C++11",
            Self::Cpp14 => "C++14",
            Self::Cpp17 => "C++17",
            Self::Cpp20 => "C++20",
            Self::Cpp23 => "C++23",
            Self::Cpp26 => "C++26",
        }
    }
    pub fn argument(self) -> Option<String> {
        (self != Self::Default).then(|| format!("-std={}", self.value()))
    }
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        Self::ALL
            .into_iter()
            .find(|standard| standard.value() == value)
            .ok_or_else(|| anyhow::anyhow!("未知的 C++ 标准：{value}"))
    }
}
