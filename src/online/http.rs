use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(super) struct Http {
    pub cancel: Arc<AtomicBool>,
    pub stopped: Arc<AtomicBool>,
    pub request_cancel: Arc<AtomicBool>,
}

pub(super) struct Response {
    pub status: u16,
    pub headers: Value,
    pub raw: Vec<u8>,
    pub body: Value,
}

struct Network {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
}

fn network() -> Result<&'static Network, String> {
    static NETWORK: OnceLock<Result<Network, String>> = OnceLock::new();
    NETWORK
        .get_or_init(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;

            let client = {
                let _guard = runtime.enter();
                reqwest::Client::builder()
                    .user_agent("lx-music-desktop/2.0.0")
                    .redirect(reqwest::redirect::Policy::limited(5))
                    .build()
                    .map_err(|e| e.to_string())?
            };

            Ok(Network { runtime, client })
        })
        .as_ref()
        .map_err(Clone::clone)
}

impl Http {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        Self {
            cancel,
            stopped: Arc::default(),
            request_cancel: Arc::default(),
        }
    }

    pub fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed)
            || self.stopped.load(Ordering::Relaxed)
            || self.request_cancel.load(Ordering::Relaxed)
        {
            Err("请求已取消".into())
        } else {
            Ok(())
        }
    }

    pub fn request(&self, url: &str, options: &Value) -> Result<Response, String> {
        self.check()?;
        let network = network()?;
        let url = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("仅支持 HTTP/HTTPS".into());
        }

        let method = options["method"]
            .as_str()
            .unwrap_or("GET")
            .parse::<reqwest::Method>()
            .map_err(|e| e.to_string())?;

        let mut request = network.client.request(method, url);
        if let Some(headers) = options["headers"].as_object() {
            for (name, value) in headers {
                request = request.header(name, text(value));
            }
        }
        if let Some(form) = options["form"].as_object() {
            let pairs: Vec<_> = form
                .iter()
                .map(|(key, value)| (key.clone(), text(value)))
                .collect();

            request = request.form(&pairs);
        } else if let Some(body) = options.get("body").filter(|body| !body.is_null()) {
            request = if let Some(string) = body.as_str() {
                request.body(string.to_owned())
            } else if let Some(bytes) = body.as_array() {
                request.body(
                    bytes
                        .iter()
                        .map(|byte| byte.as_u64().unwrap_or(0) as u8)
                        .collect::<Vec<_>>(),
                )
            } else {
                request.json(body)
            };
        }

        let timeout =
            Duration::from_millis(options["timeout"].as_u64().unwrap_or(12000).clamp(1, 60000));

        let max = options["maxBytes"]
            .as_u64()
            .unwrap_or(4 * 1024 * 1024)
            .min(100 * 1024 * 1024) as usize;

        network.runtime.block_on(async {
            let fetch = async {
                let mut response = request
                    .timeout(timeout)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;

                let status = response.status().as_u16();

                let headers = response
                    .headers()
                    .iter()
                    .map(|(key, value)| {
                        (key.to_string(), json!(value.to_str().unwrap_or_default()))
                    })
                    .collect::<serde_json::Map<_, _>>();

                let mut raw = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
                    if raw.len() + chunk.len() > max {
                        return Err("响应超过大小限制".into());
                    }
                    raw.extend_from_slice(&chunk);
                }

                let body = serde_json::from_slice(&raw)
                    .unwrap_or_else(|_| json!(String::from_utf8_lossy(&raw)));

                Ok(Response {
                    status,
                    headers: Value::Object(headers),
                    raw,
                    body,
                })
            };

            let cancelled = async {
                loop {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    if self.check().is_err() {
                        break;
                    }
                }
            };

            tokio::select! { result = fetch => result, _ = cancelled => Err("请求已取消".into()) }
        })
    }

    pub fn fetch(&self, url: &str, options: &Value) -> Result<Response, String> {
        let response = self.request(url, options)?;
        if response.status >= 400 {
            Err(format!("HTTP {}", response.status))
        } else {
            Ok(response)
        }
    }
}

pub(super) fn text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

pub(super) fn key(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub(super) fn write_cache(path: &Path, bytes: &[u8]) -> Result<(), String> {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| {
        let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
        for chunk in bytes.chunks(8192) {
            file.write_all(chunk).map_err(|e| e.to_string())?;
        }
        drop(file);
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

pub(super) fn deadline(start: Instant, seconds: u64) -> Result<(), String> {
    if start.elapsed() > Duration::from_secs(seconds) {
        Err("在线请求超时".into())
    } else {
        Ok(())
    }
}
