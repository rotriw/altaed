use crate::{
    document::TestCase,
    problem::{ImportBatch, Problem, ProblemId, ProblemInput, ProblemOutput},
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProblemPayload {
    name: String,
    group: String,
    url: String,
    #[serde(default)]
    interactive: bool,
    memory_limit: u64,
    time_limit: u64,
    tests: Vec<SamplePayload>,
    test_type: String,
    input: IoPayload,
    output: IoPayload,
    #[serde(default)]
    languages: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    batch: Option<ImportBatch>,
    #[serde(flatten)]
    additional: BTreeMap<String, serde_json::Value>,
}
#[derive(Deserialize)]
struct SamplePayload {
    input: String,
    output: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IoPayload {
    #[serde(rename = "type")]
    kind: String,
    file_name: Option<String>,
    pattern: Option<String>,
}
fn required(value: Option<String>, field: &str) -> Result<String> {
    let value = value.with_context(|| format!("缺少 {field}"))?;
    ensure!(!value.trim().is_empty(), "{field} 不能为空");
    Ok(value)
}
pub fn parse_problem(bytes: &[u8]) -> Result<Problem> {
    ensure!(
        bytes.len() <= super::server::MAX_BODY_BYTES,
        "导入数据超过 2 MiB"
    );
    let data: ProblemPayload = serde_json::from_slice(bytes).context("题目 JSON 格式无效")?;
    ensure!(
        !data.name.trim().is_empty() && data.name.len() <= 4096,
        "题目名称为空或过长"
    );
    ensure!(
        data.group.len() <= 4096 && data.url.len() <= 8192,
        "题目分组或 URL 过长"
    );
    let url = url::Url::parse(&data.url).context("题目 URL 无效")?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "题目 URL 必须是无账号信息的 HTTP(S) 地址"
    );
    ensure!(data.tests.len() <= 256, "单题样例数量过多");
    let input = match data.input.kind.as_str() {
        "stdin" => ProblemInput::Stdin,
        "file" => ProblemInput::File {
            file_name: required(data.input.file_name, "input.fileName")?,
        },
        "regex" => ProblemInput::Regex {
            pattern: required(data.input.pattern, "input.pattern")?,
        },
        _ => anyhow::bail!("未知的题目输入类型：{}", data.input.kind),
    };
    let output = match data.output.kind.as_str() {
        "stdout" => ProblemOutput::Stdout,
        "file" => ProblemOutput::File {
            file_name: required(data.output.file_name, "output.fileName")?,
        },
        _ => anyhow::bail!("未知的题目输出类型：{}", data.output.kind),
    };
    if let Some(batch) = &data.batch {
        ensure!(
            !batch.id.is_empty() && batch.id.len() <= 256 && batch.size > 0,
            "题目批次信息无效"
        );
    }
    Ok(Problem {
        id: ProblemId::default(),
        title: data.name,
        group: data.group,
        url: data.url,
        time_limit_ms: (data.time_limit > 0).then_some(data.time_limit),
        memory_limit_mb: (data.memory_limit > 0).then_some(data.memory_limit),
        interactive: data.interactive,
        input,
        output,
        test_type: data.test_type,
        samples: data
            .tests
            .into_iter()
            .map(|sample| TestCase {
                input: sample.input,
                expected_output: Some(sample.output),
                ..Default::default()
            })
            .collect(),
        languages: data.languages,
        batch: data.batch,
        additional: data.additional,
    })
}
