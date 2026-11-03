use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A native testlib-style checker. Arguments are passed directly, never through
/// a shell; the three snapshot paths precede these optional extra arguments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutableChecker {
    pub program: PathBuf,
    pub timeout_ms: u64,
    pub arguments: Vec<String>,
}
impl Default for ExecutableChecker {
    fn default() -> Self {
        Self {
            program: PathBuf::new(),
            timeout_ms: 2_000,
            arguments: Vec::new(),
        }
    }
}
impl ExecutableChecker {
    /// Import validates a native executable without running it.
    pub fn validate_program(path: &std::path::Path) -> anyhow::Result<PathBuf> {
        use anyhow::{Context, ensure};
        use std::io::Read;
        let path = std::fs::canonicalize(path).context("无法定位比较器文件")?;
        let mut file = std::fs::File::open(&path).context("无法读取比较器文件")?;
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "比较器必须是普通可执行文件");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                metadata.permissions().mode() & 0o111 != 0,
                "比较器没有执行权限"
            );
        }
        let mut magic = [0; 4];
        file.read_exact(&mut magic)
            .context("比较器不是有效的可执行文件")?;
        #[cfg(target_os = "macos")]
        let native = matches!(
            magic,
            [0xcf, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xcf]
                | [0xce, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xce]
                | [0xca, 0xfe, 0xba, 0xbe]
                | [0xbe, 0xba, 0xfe, 0xca]
                | [0xca, 0xfe, 0xba, 0xbf]
                | [0xbf, 0xba, 0xfe, 0xca]
        );
        #[cfg(target_os = "windows")]
        let native = magic.starts_with(b"MZ");
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let native = magic == *b"\x7fELF";
        ensure!(
            native,
            "请选择适用于当前系统的原生可执行文件，不支持源码或脚本"
        );
        Ok(path)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Checker {
    #[default]
    Trimmed,
    Exact,
    Integers,
    Real(RealPrecision),
    Tokens,
    YesNo,
    Float {
        absolute: f64,
        relative: f64,
    },
    Executable(ExecutableChecker),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RealPrecision {
    Four,
    Six,
    Nine,
}
impl RealPrecision {
    pub fn epsilon(self) -> f64 {
        match self {
            Self::Four => 1e-4,
            Self::Six => 1e-6,
            Self::Nine => 1e-9,
        }
    }
}
impl Checker {
    /// Shared order and identities for the native selector and settings form.
    pub fn builtins() -> [(&'static str, Self); 8] {
        [
            ("trimmed", Self::Trimmed),
            ("exact", Self::Exact),
            ("ncmp", Self::Integers),
            ("rcmp4", Self::Real(RealPrecision::Four)),
            ("rcmp6", Self::Real(RealPrecision::Six)),
            ("rcmp9", Self::Real(RealPrecision::Nine)),
            ("tokens", Self::Tokens),
            ("nyesno", Self::YesNo),
        ]
    }
    pub fn mode_id(&self) -> &'static str {
        match self {
            Self::Trimmed => "trimmed",
            Self::Exact => "exact",
            Self::Integers => "ncmp",
            Self::Real(RealPrecision::Four) => "rcmp4",
            Self::Real(RealPrecision::Six) => "rcmp6",
            Self::Real(RealPrecision::Nine) => "rcmp9",
            Self::Tokens => "tokens",
            Self::YesNo => "nyesno",
            Self::Float { .. } => "float",
            Self::Executable(_) => "executable",
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Trimmed => "忽略行末空格和文末换行".into(),
            Self::Exact => "严格一致".into(),
            Self::Integers => "ncmp · 匹配 64 位整数".into(),
            Self::Real(precision) => format!(
                "{} · 匹配浮点数，最大误差 {}",
                self.mode_id(),
                match precision {
                    RealPrecision::Four => "1e-4",
                    RealPrecision::Six => "1e-6",
                    RealPrecision::Nine => "1e-9",
                }
            ),
            Self::Tokens => "wcmp · 匹配字符串".into(),
            Self::YesNo => "nyesno · 匹配 YES 和 NO，大小写不敏感".into(),
            Self::Float { .. } => "自定义浮点容差".into(),
            Self::Executable(checker) => format!(
                "比较器 · {}",
                checker
                    .program
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
        }
    }

    pub fn float_default() -> Self {
        const DEFAULT_TOLERANCE: f64 = 1e-6;
        Self::Float {
            absolute: DEFAULT_TOLERANCE,
            relative: DEFAULT_TOLERANCE,
        }
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Self::Executable(checker) = self {
            anyhow::ensure!(
                !checker.program.as_os_str().is_empty(),
                "请选择比较器可执行文件"
            );
            anyhow::ensure!(checker.timeout_ms > 0, "比较器超时必须大于零");
        }
        if let Self::Float { absolute, relative } = self {
            anyhow::ensure!(
                absolute.is_finite() && relative.is_finite() && *absolute >= 0. && *relative >= 0.,
                "浮点容差必须是非负有限数"
            );
        }
        Ok(())
    }
    pub fn accepts(&self, expected: &str, actual: &[u8]) -> bool {
        if matches!(self, Self::Executable(_)) {
            return false;
        }
        if matches!(self, Self::Trimmed) {
            return normalized_lines(expected.as_bytes()).eq(normalized_lines(actual));
        }
        if matches!(self, Self::Exact) {
            return expected.as_bytes() == actual;
        }
        let Ok(actual) = std::str::from_utf8(actual) else {
            return false;
        };
        let mut expected_tokens = expected.split_whitespace();
        let mut actual_tokens = actual.split_whitespace();
        if self.validate().is_err() {
            return false;
        }
        // Streaming avoids allocating an entry for every token in large files.
        loop {
            let (expected, actual) = match (expected_tokens.next(), actual_tokens.next()) {
                (None, None) => return true,
                (Some(expected), Some(actual)) => (expected, actual),
                _ => return false,
            };
            let accepted = match self {
                Self::Integers => match (expected.parse::<i64>(), actual.parse::<i64>()) {
                    (Ok(a), Ok(b)) => a == b,
                    _ => false,
                },
                Self::YesNo => {
                    (expected.eq_ignore_ascii_case("YES") || expected.eq_ignore_ascii_case("NO"))
                        && expected.eq_ignore_ascii_case(actual)
                }
                Self::Real(precision) => match (expected.parse::<f64>(), actual.parse::<f64>()) {
                    (Ok(expected), Ok(actual)) => {
                        let epsilon = precision.epsilon() + 1e-15;
                        let lower = expected * (1.0 - epsilon);
                        let upper = expected * (1.0 + epsilon);
                        expected.is_finite()
                            && actual.is_finite()
                            && ((expected - actual).abs() <= epsilon
                                || (lower.min(upper)..=lower.max(upper)).contains(&actual))
                    }
                    _ => false,
                },
                Self::Float { absolute, relative } => {
                    match (expected.parse::<f64>(), actual.parse::<f64>()) {
                        (Ok(a), Ok(b)) => {
                            a.is_finite()
                                && b.is_finite()
                                && ((a - b).abs() <= *absolute
                                    || (a - b).abs() / a.abs().max(b.abs()) <= *relative)
                        }
                        (Err(_), Err(_)) => expected == actual,
                        _ => false,
                    }
                }
                _ => expected == actual,
            };
            if !accepted {
                return false;
            }
        }
    }
}

// Strip only ASCII horizontal line tails, then terminal empty lines. Interior
// blank lines and leading spaces remain significant; CRLF and LF align.
fn normalized_lines(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let end = bytes
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .map_or(0, |index| index + 1);
    bytes[..end].split(|byte| *byte == b'\n').map(|line| {
        let end = line
            .iter()
            .rposition(|byte| !matches!(byte, b' ' | b'\t' | b'\r'))
            .map_or(0, |index| index + 1);
        &line[..end]
    })
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
    #[test]
    fn builtin_numeric_and_yes_no_rules_keep_their_boundaries() {
        assert_eq!(Checker::builtins().len(), 8);
        assert!(Checker::Integers.accepts(
            "9223372036854775807 -9223372036854775808",
            b"9223372036854775807\n-9223372036854775808"
        ));
        assert!(!Checker::Integers.accepts("9223372036854775807", b"9223372036854775806"));
        assert!(!Checker::Integers.accepts("9223372036854775808", b"9223372036854775808"));
        assert!(!Checker::Integers.accepts("1.0", b"1"));
        assert!(Checker::YesNo.accepts("YES NO yes", b"yes\nNo YES"));
        assert!(!Checker::YesNo.accepts("anything", b"ANYTHING"));
        assert!(!Checker::YesNo.accepts("YES NO", b"yes"));
        for precision in [RealPrecision::Four, RealPrecision::Six, RealPrecision::Nine] {
            let checker = Checker::Real(precision);
            assert!(checker.accepts(
                "1",
                format!("{}", 1.0 + precision.epsilon() * 0.9).as_bytes()
            ));
            assert!(!checker.accepts(
                "1",
                format!("{}", 1.0 + precision.epsilon() * 2.0).as_bytes()
            ));
            assert!(checker.accepts(
                "-1000",
                format!("{}", -1000.0 * (1.0 + precision.epsilon() * 0.9)).as_bytes()
            ));
            assert!(!checker.accepts("YES", b"YES"));
            assert!(!checker.accepts("NaN", b"NaN"));
            assert!(!checker.accepts("inf", b"inf"));
        }
        assert!(!Checker::Trimmed.accepts("a\n\nb", b"a\nb"));
        assert!(Checker::Trimmed.accepts("a \r\nb\t\r\n\r\n", b"a\nb"));
    }
    #[test]
    fn trailing_space_policy_preserves_other_differences() {
        for (expected, actual) in [
            ("1 \t\n2\t", "1\n2"),
            ("x \r\n", "x\r\n"),
            ("", " \t"),
            ("x\n\n", "x"),
            ("x\r\n", "x"),
        ] {
            assert!(Checker::Trimmed.accepts(expected, actual.as_bytes()));
            assert!(!Checker::Exact.accepts(expected, actual.as_bytes()));
        }
        for (expected, actual) in [("1 2", "1  2"), ("x", " x")] {
            assert!(!Checker::Trimmed.accepts(expected, actual.as_bytes()));
        }
    }
}
