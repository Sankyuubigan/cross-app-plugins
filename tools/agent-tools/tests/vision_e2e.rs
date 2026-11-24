//! Сквозные тесты тула `vision`: реальный бинарь `agent-tools.exe` + стаб-сервер
//! на `127.0.0.1:0` (через `--server-url`). Проверяем: код возврата, что в
//! `/v1/chat/completions` ушёл base64/`image_url`, что ответ печатается в stdout.
//!
//! Стаб отвечает на `GET /health` (200) и `POST /v1/chat/completions` (200 + текст).
//! Сеть наружу не используется. Живой прогон с реальным llama-server — `#[ignore]`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "agent_tools_vision_e2e_{tag}_{}_{}_{nanos}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("создать временную директорию");
    dir
}

#[derive(Debug, Clone)]
struct Captured {
    method: String,
    path: String,
    body: String,
}

impl Captured {
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("тело — валидный JSON")
    }
}

struct Stub {
    base_url: String,
    requests: Arc<Mutex<Vec<Captured>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Stub {
    /// Стаб: /health → 200, /v1/chat/completions → 200 с текстом `reply`.
    fn start(reply: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        listener.set_nonblocking(true).expect("nonblock");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Some(cap) = read_request(&mut stream) {
                                let is_health = cap.method == "GET" && cap.path.contains("/health");
                                let body = if is_health {
                                    "{\"status\":\"ok\"}".to_string()
                                } else {
                                    // SSE-стрим (движок читает `data: {...}` построчно).
                                    // Один чанк с content + финальный [DONE].
                                    format!(
                                        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{reply}\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n"
                                    )
                                };
                                requests.lock().expect("reqs").push(cap);
                                let ctype = if is_health { "application/json" } else { "text/event-stream" };
                                let response = format!(
                                    "HTTP/1.1 200 OK\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                                    body.len()
                                );
                                let _ = stream.write_all(response.as_bytes());
                                let _ = stream.flush();
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Self {
            base_url: format!("http://{addr}/v1"),
            requests,
            stop,
            worker: Some(worker),
        }
    }

    fn requests(&self) -> Vec<Captured> {
        self.requests.lock().expect("reqs").clone()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Captured> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some(Captured {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn assert_code(&self, expected: i32) -> &Self {
        assert_eq!(
            self.code,
            Some(expected),
            "код неверный.\nstdout: {}\nstderr: {}",
            self.stdout,
            self.stderr
        );
        self
    }
    fn assert_stdout_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stdout.contains(needle),
            "stdout не содержит {needle:?}:\n{}",
            self.stdout
        );
        self
    }
}

fn bin() -> PathBuf {
    // target/<profile>/agent-tools.exe — cargo test кладёт рядом.
    let exe = std::env::current_exe().expect("current_exe");
    let dir = exe.parent().expect("parent");
    // Поднимаемся из deps/ к корню target.
    let target = dir.parent().unwrap_or(dir);
    let name = if cfg!(windows) {
        "agent-tools.exe"
    } else {
        "agent-tools"
    };
    target.join(name)
}

fn run_vision(args: &[&str]) -> Run {
    let out = Command::new(bin())
        .arg("vision")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("запуск agent-tools vision");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn make_png(dir: &PathBuf, name: &str, w: u32, h: u32) -> String {
    let p = dir.join(name);
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
    image::DynamicImage::ImageRgba8(img).save(&p).unwrap();
    p.to_string_lossy().to_string()
}

#[test]
fn image_request_carries_base64_and_prints_answer() {
    let stub = Stub::start("На картинке красный куб на сером фоне.");
    let dir = tmp_dir("img");
    let img = make_png(&dir, "shot.png", 8, 8);

    let run = run_vision(&[
        "--prompt",
        "что на картинке",
        "--image",
        &img,
        "--server-url",
        &stub.base_url,
        "--no-think",
    ]);
    run.assert_code(0).assert_stdout_contains("красный куб");

    let reqs = stub.requests();
    let chat = reqs
        .iter()
        .find(|r| r.path.contains("/v1/chat/completions"))
        .expect("был POST /v1/chat/completions");
    let json = chat.json();
    let content = &json["messages"][0]["content"];
    // content — массив частей: text + image_url (data URL base64).
    assert!(
        content.to_string().contains("image_url"),
        "в запросе нет image_url: {content}"
    );
    assert!(
        content.to_string().contains("base64,"),
        "в image_url нет base64 data-URL: {content}"
    );
}

#[test]
fn no_media_is_exit_2_without_request() {
    let stub = Stub::start("x");
    let run = run_vision(&["--prompt", "что", "--server-url", &stub.base_url]);
    run.assert_code(2);
    std::thread::sleep(Duration::from_millis(150));
    assert!(stub.requests().is_empty(), "при ошибке ввода запрос не уходит");
}

#[test]
fn missing_image_file_is_exit_2() {
    let stub = Stub::start("x");
    let run = run_vision(&[
        "--prompt",
        "что",
        "--image",
        "nope-does-not-exist.png",
        "--server-url",
        &stub.base_url,
    ]);
    run.assert_code(2);
}

#[test]
fn non_image_file_is_exit_2_not_backend() {
    // Файл есть, но не картинка → Media (код 2), не «уронит сервер» (код 3).
    let stub = Stub::start("x");
    let dir = tmp_dir("fake");
    let p = dir.join("fake.png");
    std::fs::write(&p, b"not a png").unwrap();
    let run = run_vision(&[
        "--prompt",
        "что",
        "--image",
        p.to_str().unwrap(),
        "--server-url",
        &stub.base_url,
    ]);
    run.assert_code(2);
}

#[test]
fn empty_model_output_is_exit_3() {
    // Стаб отвечает 200, но пустым content → Backend (код 3), не ложный 0.
    let stub = Stub::start("");
    let dir = tmp_dir("empty");
    let img = make_png(&dir, "shot.png", 4, 4);
    let run = run_vision(&[
        "--prompt",
        "что",
        "--image",
        &img,
        "--server-url",
        &stub.base_url,
        "--no-think",
    ]);
    run.assert_code(3);
}

/// Живой прогон: нужен реальный llama-server + LFM3-VL-2B. `#[ignore]`.
#[test]
#[ignore]
fn live_image_vlm() {
    let dir = tmp_dir("live");
    let img = make_png(&dir, "live.png", 64, 64);
    let run = run_vision(&["--prompt", "Опиши изображение", "--image", &img, "--no-think"]);
    run.assert_code(0);
}

/// Видео-e2e: генерим mp4 (testsrc) → 4 кадра уходят массивом image_url в одном
/// запросе. Если ffmpeg нет — skip (CI без ffmpeg не падает).
#[test]
fn video_request_sends_multiple_frames() {
    let dir = tmp_dir("vid");
    let mp4 = dir.join("clip.mp4");
    let made = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=64x64:rate=10",
            "-pix_fmt",
            "yuv420p",
            "-y",
            mp4.to_str().unwrap(),
        ])
        .status();
    if made.map(|s| !s.success()).unwrap_or(true) {
        eprintln!("ffmpeg недоступен — видео-e2e пропущен");
        return;
    }

    let stub = Stub::start("Анимация: куб вращается.");
    let run = run_vision(&[
        "--prompt",
        "есть ли артефакты",
        "--video",
        mp4.to_str().unwrap(),
        "--max-frames",
        "4",
        "--server-url",
        &stub.base_url,
        "--no-think",
    ]);
    run.assert_code(0).assert_stdout_contains("куб вращается");

    let chat = stub
        .requests()
        .into_iter()
        .find(|r| r.path.contains("/v1/chat/completions"))
        .expect("POST /v1/chat/completions");
    let json = chat.json();
    let content = json["messages"][0]["content"].to_string();
    // 4 кадра → 4 вхождения `"image_url":{` (ключа url; `type:image_url` не считаем).
    let count = content.matches("\"image_url\":{").count();
    assert_eq!(count, 4, "ожидалось 4 image_url (по кадру), получили {count}");
}