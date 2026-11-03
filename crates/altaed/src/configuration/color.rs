//! Serializable application color without a GPUI dependency.
use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RgbColor(u32);
impl RgbColor {
    pub const fn new(rgb: u32) -> Self {
        Self(rgb & 0x00ff_ffff)
    }
    pub fn parse(text: &str) -> Result<Self> {
        ensure!(
            text.len() == 7
                && text.starts_with('#')
                && text.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit),
            "颜色应使用 #RRGGBB 格式"
        );
        Ok(Self(u32::from_str_radix(&text[1..], 16)?))
    }
    pub fn optional(text: &str) -> Result<Option<Self>> {
        if text.trim().is_empty() {
            Ok(None)
        } else {
            Self::parse(text).map(Some)
        }
    }
    pub fn mix(self, other: Self, percent: u8) -> Self {
        let channel = |shift: u32| {
            let a = (self.0 >> shift) & 255;
            let b = (other.0 >> shift) & 255;
            (a * (100 - percent.min(100) as u32) + b * percent.min(100) as u32) / 100
        };
        Self((channel(16) << 16) | (channel(8) << 8) | channel(0))
    }
    pub fn rgb(self) -> u32 {
        self.0
    }
    fn luminance(self) -> f64 {
        let channel = |shift: u32| {
            let value = ((self.0 >> shift) & 255_u32) as f64 / 255.;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        channel(16) * 0.2126 + channel(8) * 0.7152 + channel(0) * 0.0722
    }
    pub fn contrast(self, other: Self) -> f64 {
        let a = self.luminance();
        let b = other.luminance();
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    pub fn readable_foreground(self) -> Self {
        let navy = Self::new(0x27313b);
        if self.contrast(navy) >= 4.5 {
            navy
        } else if self.contrast(Self::new(0xffffff)) >= self.contrast(Self::new(0)) {
            Self::new(0xffffff)
        } else {
            Self::new(0)
        }
    }
}
impl std::fmt::Display for RgbColor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "#{:06x}", self.0)
    }
}
impl Serialize for RgbColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for RgbColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors_validate_round_trip_and_keep_execution_text_readable() {
        for invalid in ["red", "#abc", "#ff000000", "#gg0000", "000000"] {
            assert!(RgbColor::parse(invalid).is_err());
        }
        for value in [0, 0xffffff, 0x777777, 0xe8f0fb, 0xe8f3ec, 0x315e91] {
            let background = RgbColor::new(value);
            let foreground = background.readable_foreground();
            let a = background.luminance();
            let b = foreground.luminance();
            assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5);
            let encoded = serde_json::to_string(&background).unwrap();
            assert_eq!(
                serde_json::from_str::<RgbColor>(&encoded).unwrap(),
                background
            );
        }
    }
}
