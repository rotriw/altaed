//! Bounded, line-aligned presentation of exact output differences. Checker
//! semantics remain independent: whitespace/float tolerance can still accept it.
use imara_diff::{Algorithm, diff, intern::InternedInput, sources::lines_with_terminator};
use std::ops::Range;

const PREVIEW_BYTES: usize = 128 * 1024;
const CONTEXT_BYTES: usize = 2048;
const MAX_ROWS: usize = 6000;
#[derive(Clone, Debug)]
pub struct DiffLine {
    pub number: usize,
    pub text: String,
    pub changed: Range<usize>,
}
#[derive(Clone, Debug)]
pub struct DiffRow {
    pub expected: Option<DiffLine>,
    pub actual: Option<DiffLine>,
    pub changed: bool,
}
pub struct OutputDiff {
    pub rows: Vec<DiffRow>,
    pub changes: Vec<usize>,
    pub truncated: bool,
    pub invalid_utf8: bool,
}
impl OutputDiff {
    pub fn between(expected: &str, actual: &[u8]) -> Self {
        let invalid_utf8 = std::str::from_utf8(actual).is_err();
        let actual = String::from_utf8_lossy(actual);
        let first = expected
            .bytes()
            .zip(actual.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(expected.len().min(actual.len()));
        let mut start = first.saturating_sub(CONTEXT_BYTES);
        while !expected.is_char_boundary(start) || !actual.is_char_boundary(start) {
            start -= 1;
        }
        if let Some(newline) = expected[..start].rfind('\n') {
            start = newline + 1;
        }
        // A very long line should not force an unbounded preview before a late mismatch.
        if first.saturating_sub(start) > PREVIEW_BYTES / 2 {
            start = first.saturating_sub(128);
            while !expected.is_char_boundary(start) || !actual.is_char_boundary(start) {
                start -= 1;
            }
        }
        let first_line = expected[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let (expected, expected_cut) = bounded(&expected[start..]);
        let (actual, actual_cut) = bounded(&actual[start..]);
        let left: Vec<_> = expected.split_inclusive('\n').collect();
        let right: Vec<_> = actual.split_inclusive('\n').collect();
        let input = InternedInput::new(
            lines_with_terminator(expected),
            lines_with_terminator(actual),
        );
        let mut hunks = Vec::new();
        diff(
            Algorithm::Histogram,
            &input,
            |before: Range<u32>, after: Range<u32>| hunks.push((before, after)),
        );
        let mut rows = Vec::new();
        let (mut l, mut r) = (0, 0);
        for (before, after) in hunks {
            while l < before.start as usize && r < after.start as usize {
                rows.push(row(
                    Some((l + first_line, left[l])),
                    Some((r + first_line, right[r])),
                    false,
                ));
                l += 1;
                r += 1;
            }
            let (le, re) = (before.end as usize, after.end as usize);
            while l < le || r < re {
                if l < le && r < re && !similar(left[l], right[r]) {
                    // Bounded look-ahead aligns an inserted line before a small
                    // replacement without quadratic pairing of large hunks.
                    let insertion = (1..=(re - r - 1).min(8))
                        .find(|offset| similar(left[l], right[r + offset]));
                    let deletion = (1..=(le - l - 1).min(8))
                        .find(|offset| similar(left[l + offset], right[r]));
                    if insertion.is_some_and(|offset| deletion.is_none_or(|other| offset <= other))
                    {
                        for _ in 0..insertion.unwrap_or_default() {
                            rows.push(row(None, Some((r + first_line, right[r])), true));
                            r += 1;
                        }
                    } else if let Some(count) = deletion {
                        for _ in 0..count {
                            rows.push(row(Some((l + first_line, left[l])), None, true));
                            l += 1;
                        }
                    }
                }
                rows.push(row(
                    (l < le).then(|| (l + first_line, left[l])),
                    (r < re).then(|| (r + first_line, right[r])),
                    true,
                ));
                l += (l < le) as usize;
                r += (r < re) as usize;
            }
        }
        while l < left.len() || r < right.len() {
            rows.push(row(
                left.get(l).map(|text| (l + first_line, *text)),
                right.get(r).map(|text| (r + first_line, *text)),
                false,
            ));
            l += 1;
            r += 1;
        }
        let truncated = start > 0 || expected_cut || actual_cut || rows.len() > MAX_ROWS;
        rows.truncate(MAX_ROWS);
        let changes = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.changed.then_some(index))
            .collect();
        Self {
            rows,
            changes,
            truncated,
            invalid_utf8,
        }
    }
}
fn bounded(text: &str) -> (&str, bool) {
    let mut end = text.len().min(PREVIEW_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], end < text.len())
}
fn row(left: Option<(usize, &str)>, right: Option<(usize, &str)>, changed: bool) -> DiffRow {
    let (lr, rr) = if changed {
        match (left, right) {
            (Some((_, a)), Some((_, b))) => character_change(a, b),
            _ => (
                0..left.map_or(0, |(_, s)| s.len()),
                0..right.map_or(0, |(_, s)| s.len()),
            ),
        }
    } else {
        (0..0, 0..0)
    };
    DiffRow {
        expected: left.map(|(number, text)| DiffLine {
            number,
            text: text.into(),
            changed: lr,
        }),
        actual: right.map(|(number, text)| DiffLine {
            number,
            text: text.into(),
            changed: rr,
        }),
        changed,
    }
}
fn character_change(a: &str, b: &str) -> (Range<usize>, Range<usize>) {
    let prefix = a
        .chars()
        .zip(b.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let suffix = a[prefix..]
        .chars()
        .rev()
        .zip(b[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    (prefix..a.len() - suffix, prefix..b.len() - suffix)
}
fn similar(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches(['\r', '\n']);
    let b = b.trim_end_matches(['\r', '\n']);
    if a == b {
        return true;
    }
    let (changed, _) = character_change(a, b);
    a.len().saturating_sub(changed.len()) * 2 >= a.len().max(b.len()).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aligns_insertions_and_marks_unicode_whitespace_and_missing_newlines() {
        let diff = OutputDiff::between(
            "same\n中文 42\nlast",
            "same\ninsert\n中文 43\nlast\n".as_bytes(),
        );
        assert!(!diff.rows[0].changed);
        assert!(diff.rows.iter().any(|row| {
            row.expected.is_none()
                && row
                    .actual
                    .as_ref()
                    .is_some_and(|line| line.text == "insert\n")
        }));
        let changed = diff
            .rows
            .iter()
            .find(|row| {
                row.expected
                    .as_ref()
                    .is_some_and(|line| line.text == "中文 42\n")
            })
            .unwrap();
        let line = changed.expected.as_ref().unwrap();
        assert_eq!(&line.text[line.changed.clone()], "2");
        assert!(diff.rows.last().unwrap().changed);
        assert!(OutputDiff::between("1 2\r\n", b"1  2\n").changes.len() > 0);
        assert!(OutputDiff::between("", b"\xff").invalid_utf8);
        assert!(OutputDiff::between("", b"").rows.is_empty());
    }
    #[test]
    fn large_equal_prefix_does_not_hide_the_first_mismatch() {
        let expected = format!("{}expected\n", "same\n".repeat(100_000));
        let actual = format!("{}actual\n", "same\n".repeat(100_000));
        let diff = OutputDiff::between(&expected, actual.as_bytes());
        assert!(diff.truncated && !diff.changes.is_empty());
        assert!(diff.rows.len() < 1000);
    }
}
