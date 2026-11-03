use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Checker {
    Exact,
    #[default]
    Tokens,
    Float {
        absolute: f64,
        relative: f64,
    },
}
impl Checker {
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Self::Float { absolute, relative } = self {
            anyhow::ensure!(
                absolute.is_finite() && relative.is_finite() && *absolute >= 0. && *relative >= 0.,
                "浮点容差必须是非负有限数"
            );
        }
        Ok(())
    }
    pub fn accepts(&self, expected: &str, actual: &[u8]) -> bool {
        if matches!(self, Self::Exact) {
            return expected.as_bytes() == actual;
        }
        let Ok(actual) = std::str::from_utf8(actual) else {
            return false;
        };
        let expected: Vec<_> = expected.split_whitespace().collect();
        let actual: Vec<_> = actual.split_whitespace().collect();
        if expected.len() != actual.len() || self.validate().is_err() {
            return false;
        }
        expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| match self {
                Self::Float { absolute, relative } => {
                    match (expected.parse::<f64>(), actual.parse::<f64>()) {
                        (Ok(a), Ok(b)) => {
                            a.is_finite()
                                && b.is_finite()
                                && ((a - b).abs() <= *absolute
                                    || (a - b).abs() / a.abs().max(b.abs()) <= *relative)
                        }
                        (Err(_), Err(_)) => *expected == actual,
                        _ => false,
                    }
                }
                _ => *expected == actual,
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checker_boundaries() {
        assert!(!Checker::Exact.accepts("", b"\n"));
        assert!(Checker::Tokens.accepts("1 2\n", b"1\n2 "));
        let checker = Checker::Float {
            absolute: 1e-6,
            relative: 1e-6,
        };
        assert!(checker.accepts("0 1e20 YES", b"0.0000001 1.0000001e20 YES"));
        for (expected, actual) in [
            ("NaN", "NaN"),
            ("inf", "inf"),
            ("YES", "yes"),
            ("1 2", "1"),
            ("1", "2"),
        ] {
            assert!(!checker.accepts(expected, actual.as_bytes()));
        }
    }
}
