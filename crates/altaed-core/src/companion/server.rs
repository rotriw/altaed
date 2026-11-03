use super::{parse_problem, store_problem};
use crate::document::DocumentSettings;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, Ipv6Addr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

pub(super) const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
pub const DEFAULT_PORT: u16 = 27121;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CompanionSettings {
    pub enabled: bool,
    pub port: u16,
}
impl Default for CompanionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            port: DEFAULT_PORT,
        }
    }
}
impl CompanionSettings {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.port != 0, "导入端口必须在 1 到 65535 之间");
        Ok(())
    }
}
pub struct CompanionServer {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for CompanionServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl CompanionServer {
    /// Join only on a background executor, never from a window callback.
    pub fn shutdown(mut self) -> Result<()> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("导入服务线程异常退出"))?;
        }
        Ok(())
    }
    pub fn start(
        settings: &CompanionSettings,
        root: PathBuf,
        defaults: DocumentSettings,
    ) -> Result<(Self, Receiver<()>)> {
        settings.validate()?;
        let mut listeners = vec![
            TcpListener::bind((Ipv4Addr::LOCALHOST, settings.port)).with_context(|| {
                format!(
                    "无法监听导入端口 {}，请检查是否被其他 IDE 占用",
                    settings.port
                )
            })?,
        ];
        match TcpListener::bind((Ipv6Addr::LOCALHOST, settings.port)) {
            Ok(listener) => listeners.push(listener),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::AddrNotAvailable | std::io::ErrorKind::Unsupported
                ) =>
            {
                ()
            }
            Err(error) => return Err(error).context("无法监听 IPv6 本地导入地址"),
        }
        for listener in &listeners {
            listener.set_nonblocking(true)?;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let port = settings.port;
        let worker = thread::Builder::new()
            .name("companion-import".into())
            .spawn(move || {
                // A wake-up represents all durable receipts; coalesce notifications.
                notify(&sender);
                while !cancelled.load(Ordering::Relaxed) {
                    for listener in &listeners {
                        match listener.accept() {
                            Ok((stream, peer)) if peer.ip().is_loopback() => {
                                if let Err(error) = handle(stream, port, &root, &defaults, &sender)
                                {
                                    eprintln!("题目导入连接未完成：{error:#}");
                                }
                            }
                            Ok(_) => (),
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
                            Err(error) => {
                                eprintln!("导入监听失败：{error}");
                                return;
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(25));
                }
            })
            .context("无法启动导入服务")?;
        Ok((
            Self {
                stop,
                worker: Some(worker),
            },
            receiver,
        ))
    }
}
fn notify(sender: &SyncSender<()>) {
    // Full means a wake-up is already queued. Disconnection means the UI closed;
    // receipts remain durable for the next start in either case.
    let _ = sender.try_send(());
}
struct Request {
    method: String,
    origin: Option<String>,
    body: Vec<u8>,
}
fn read_request(stream: &mut TcpStream, port: u16) -> Result<Request> {
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
        ensure!(bytes.len() < MAX_HEADER_BYTES, "请求头过大");
        read_more(stream, &mut bytes, deadline)?;
    };
    ensure!(header_end <= MAX_HEADER_BYTES, "请求头过大");
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = httparse::Request::new(&mut headers);
    ensure!(
        request.parse(&bytes[..header_end])?.is_complete(),
        "请求头不完整"
    );
    ensure!(request.path == Some("/"), "导入路径必须为 /");
    let header = |name: &str| -> Result<Option<&str>> {
        let mut values = request
            .headers
            .iter()
            .filter(|header| header.name.eq_ignore_ascii_case(name));
        let value = values
            .next()
            .map(|header| std::str::from_utf8(header.value))
            .transpose()?;
        ensure!(values.next().is_none(), "重复的请求头：{name}");
        Ok(value)
    };
    let allowed_hosts = [
        format!("localhost:{port}"),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
    ];
    ensure!(
        header("host")?.is_some_and(|host| allowed_hosts
            .iter()
            .any(|allowed| host.eq_ignore_ascii_case(allowed))),
        "只接受本机导入请求"
    );
    let origin = header("origin")?.map(str::to_owned);
    if let Some(origin) = &origin {
        let parsed = url::Url::parse(origin)?;
        ensure!(
            matches!(parsed.scheme(), "chrome-extension" | "moz-extension")
                && parsed.host_str().is_some()
                && !origin.contains(['\r', '\n']),
            "只接受浏览器扩展发起的导入"
        );
    }
    ensure!(
        header("transfer-encoding")?.is_none() && header("content-encoding")?.is_none(),
        "不支持分块或压缩请求"
    );
    let method = request.method.context("缺少请求方法")?.to_owned();
    let content_length = if method == "POST" {
        ensure!(
            header("content-type")?.is_some_and(|value| value
                .split(';')
                .next()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))),
            "导入内容必须为 application/json"
        );
        header("content-length")?
            .context("缺少 Content-Length")?
            .parse::<usize>()?
    } else {
        0
    };
    ensure!(content_length <= MAX_BODY_BYTES, "导入数据超过 2 MiB");
    while bytes.len() - header_end < content_length {
        read_more(stream, &mut bytes, deadline)?;
    }
    Ok(Request {
        method,
        origin,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}
fn read_more(stream: &mut TcpStream, bytes: &mut Vec<u8>, deadline: Instant) -> Result<()> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .context("请求接收超时")?;
    stream.set_read_timeout(Some(remaining))?;
    let mut buffer = [0; 4096];
    let count = stream.read(&mut buffer)?;
    ensure!(count > 0, "连接提前结束");
    bytes.extend_from_slice(&buffer[..count]);
    Ok(())
}
fn respond(
    stream: &mut TcpStream,
    status: &str,
    origin: Option<&str>,
    body: serde_json::Value,
) -> Result<()> {
    let bytes = serde_json::to_vec(&body)?;
    let cors = origin.map(|origin| format!("Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Allow-Private-Network: true\r\n")).unwrap_or_default();
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n{cors}\r\n",
        bytes.len()
    )?;
    stream.write_all(&bytes)?;
    Ok(())
}
fn handle(
    mut stream: TcpStream,
    port: u16,
    root: &std::path::Path,
    defaults: &DocumentSettings,
    sender: &SyncSender<()>,
) -> Result<()> {
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
    let request = match read_request(&mut stream, port) {
        Ok(request) => request,
        Err(error) => {
            return respond(
                &mut stream,
                "400 Bad Request",
                None,
                serde_json::json!({"error":error.to_string()}),
            );
        }
    };
    let origin = request.origin.as_deref();
    match request.method.as_str() {
        "OPTIONS" | "GET" => respond(
            &mut stream,
            "200 OK",
            origin,
            serde_json::json!({"service":"altaed", "protocol":"competitive-companion", "version":1}),
        ),
        "POST" => match parse_problem(&request.body) {
            Err(error) => respond(
                &mut stream,
                "400 Bad Request",
                origin,
                serde_json::json!({"error":error.to_string()}),
            ),
            Ok(problem) => match store_problem(root, problem, defaults.clone()) {
                Ok(id) => {
                    notify(sender);
                    respond(
                        &mut stream,
                        "201 Created",
                        origin,
                        serde_json::json!({"documentId":id.to_string(), "status":"received"}),
                    )
                }
                Err(error) => {
                    eprintln!("无法保存导入题目：{error:#}");
                    respond(
                        &mut stream,
                        "503 Service Unavailable",
                        origin,
                        serde_json::json!({"error":"题目信息未能保存，请在 IDE 检查导入状态"}),
                    )
                }
            },
        },
        _ => respond(
            &mut stream,
            "405 Method Not Allowed",
            origin,
            serde_json::json!({"error":"只支持 GET、POST、OPTIONS"}),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn exchange(root: &std::path::Path, origin: Option<&str>, body: &[u8]) -> Result<String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let directory = root.to_owned();
        let (sender, _receiver) = mpsc::sync_channel(1);
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept()?;
            handle(
                stream,
                port,
                &directory,
                &DocumentSettings::default(),
                &sender,
            )
        });
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
        client.set_read_timeout(Some(Duration::from_secs(5)))?;
        let origin = origin
            .map(|value| format!("Origin: {value}\r\n"))
            .unwrap_or_default();
        write!(
            client,
            "POST / HTTP/1.1\r\nHost: localhost:{port}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )?;
        client.write_all(body)?;
        let mut response = String::new();
        client.read_to_string(&mut response)?;
        server
            .join()
            .map_err(|_| anyhow::anyhow!("test server panicked"))??;
        Ok(response)
    }
    #[test]
    fn http_acknowledges_durable_import_and_blocks_web_page_origins() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("altaed-http-import-{}", uuid::Uuid::new_v4()));
        let payload = include_bytes!("../../tests/fixtures/companion.json");
        let response = exchange(
            &root,
            Some("chrome-extension://cjnmckjndlpiamhfimnnjmnckgghkjbl"),
            payload,
        )?;
        assert!(response.starts_with("HTTP/1.1 201"), "{response}");
        assert_eq!(super::super::pending_documents(&root)?.len(), 1);
        assert!(exchange(&root, Some("https://example.com"), payload)?.starts_with("HTTP/1.1 400"));
        assert!(
            exchange(&root, None, b"{}")
                .unwrap()
                .starts_with("HTTP/1.1 400")
        );
        assert_eq!(super::super::pending_documents(&root)?.len(), 1);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
