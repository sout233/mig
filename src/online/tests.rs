use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Mutex, atomic::Ordering},
    time::{Duration, Instant},
};

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[test]
fn playlist_links_extract_only_netease_playlist_ids() {
    for url in [
        "https://music.163.com/playlist?id=18434297757&uct2=U2FsdGVkX1+/fj0u5IwpQ+CrtcJNRTfNRV4vObptxZI=",
        "https://music.163.com/#/playlist?id=18434297757",
        "分享歌单 https://m.music.163.com/playlist/18434297757?userid=1",
        "18434297757",
    ] {
        assert_eq!(playlist::playlist_id(url).unwrap(), Some(18434297757));
    }
    assert_eq!(playlist::playlist_id("周杰伦").unwrap(), None);
    for invalid in [
        "0",
        "18446744073709551616",
        "https://example.test/playlist?id=1",
        "https://music.163.com/song?id=1",
        "https://music.163.com/playlist?id=bad",
    ] {
        assert!(playlist::playlist_id(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn playlist_hydrates_partial_tracks_in_id_order_and_falls_back_per_request() {
    let native = Server::new(|path| {
        let body = if path.starts_with("/api/v6/playlist/detail") {
            json!({"code":200,"playlist":{"id":7,"name":"测试歌单","trackCount":3,
                "trackIds":[{"id":3},{"id":1},{"id":2}],
                "tracks":[{"id":3,"name":"第三首","al":{"name":"专辑","picUrl":"https://example.test/cover"}}]}})
        } else {
            json!({"code":503})
        };

        (200, body.to_string().into_bytes(), String::new())
    });

    let fallback = Server::new(|path| {
        assert!(path.starts_with("/song/detail?ids=1%2C2"));
        (
            200,
            json!({"code":200,"songs":[{"id":2,"name":"第二首"},{"id":1,"name":"第一首"}]})
                .to_string()
                .into_bytes(),
            String::new(),
        )
    });

    let result = playlist::load_with(
        &http::Http::new(Arc::default()),
        7,
        &native.url,
        &fallback.url,
    )
    .unwrap();

    assert_eq!(result.info.total, 3);
    assert_eq!(
        result
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect::<Vec<_>>(),
        ["第三首", "第一首", "第二首"]
    );
    assert_eq!(
        result.tracks[0].online.as_ref().unwrap().music_info["img"],
        "https://example.test/cover"
    );
    assert_eq!(native.requests.lock().unwrap().len(), 2);
    assert_eq!(fallback.requests.lock().unwrap().len(), 1);
    let cancelled = http::Http::new(Arc::new(AtomicBool::new(true)));
    assert!(playlist::load_with(&cancelled, 7, &native.url, &fallback.url).is_err());
    assert_eq!(fallback.requests.lock().unwrap().len(), 1);
}

#[test]
fn playlist_detail_falls_back_without_truncating_an_empty_playlist() {
    let native = Server::new(|_| {
        (
            200,
            json!({"code":403}).to_string().into_bytes(),
            String::new(),
        )
    });

    let fallback = Server::new(|_| {
        (
            200,
            json!({"code":200,"playlist":{"id":7,"name":"空歌单","trackIds":[],"tracks":[]}})
                .to_string()
                .into_bytes(),
            String::new(),
        )
    });

    let result = playlist::load_with(
        &http::Http::new(Arc::default()),
        7,
        &native.url,
        &fallback.url,
    )
    .unwrap();

    assert_eq!(result.info.name, "空歌单");
    assert_eq!(result.info.total, 0);
    assert!(result.tracks.is_empty());
}

#[test]
#[ignore = "requires the public NetEase network endpoints"]
fn live_playlist_link_loads_all_track_ids_without_node() {
    let result = browse(
        "https://music.163.com/playlist?id=18434297757&uct2=ignored",
        SearchKind::Playlists,
        1,
        Arc::default(),
    )
    .unwrap();

    let BrowseResult::Playlist(result) = result else {
        panic!("expected playlist detail")
    };

    assert!(result.info.total > 10);
    assert_eq!(result.tracks.len(), result.info.total);
    assert!(
        result
            .tracks
            .iter()
            .all(|track| track.online.as_ref().unwrap().source == "wy")
    );
    eprintln!(
        "Loaded {} / {} tracks: {}",
        result.tracks.len(),
        result.info.total,
        result.info.name
    );
    let search = browse("周杰伦", SearchKind::Playlists, 1, Arc::default()).unwrap();
    let BrowseResult::Playlists { items, total } = search else {
        panic!("expected playlist results")
    };

    assert!(!items.is_empty() && total >= items.len());
}

impl Server {
    fn new(handler: impl Fn(&str) -> (u16, Vec<u8>, String) + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = stop.clone();
        let visited = requests.clone();
        let handler = Arc::new(handler);
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream.set_nonblocking(false).unwrap();
                    let handler = handler.clone();
                    let visited = visited.clone();
                    std::thread::spawn(move || {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut raw = Vec::new();
                        let mut buffer = [0; 1024];
                        while !raw.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                            let Ok(length) = stream.read(&mut buffer) else {
                                return;
                            };

                            if length == 0 {
                                return;
                            }
                            raw.extend_from_slice(&buffer[..length]);
                        }
                        let request = String::from_utf8_lossy(&raw);
                        let path = request.split_whitespace().nth(1).unwrap_or("/");
                        visited.lock().unwrap().push(path.into());
                        let (status, body, headers) = handler(path);

                        let _ = write!(
                            stream,
                            "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
                            body.len()
                        );

                        let _ = stream.write_all(&body);
                    });
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });

        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.thread.take().unwrap().join();
    }
}

fn temporary(label: &str) -> PathBuf {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let path = std::env::temp_dir().join(format!(
        "mig-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));

    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn cancelled_request_never_enters_network_or_javascript() {
    assert_eq!(
        search("测试", 1, Arc::new(AtomicBool::new(true)))
            .err()
            .as_deref(),
        Some("请求已取消")
    );
}

#[test]
fn embedded_lx_supports_crypto_buffers_compression_timers_and_promises() {
    let raw = r#"
      const {on,send,utils,EVENT_NAMES}=lx;
      if(typeof require !== 'undefined' || typeof process !== 'undefined') throw Error('Node leaked');
      on(EVENT_NAMES.request, async data => {
        if(utils.crypto.md5('abc') !== '900150983cd24fb0d6963f7d28e17f72') throw Error('md5');
        const encoded=utils.buffer.from('中文','utf8');
        if(utils.buffer.bufToString(await utils.zlib.inflate(await utils.zlib.deflate(encoded)),'utf8') !== '中文') throw Error('zlib');
        const encrypted=utils.crypto.aesEncrypt(Buffer.from('6bc1bee22e409f96e93d7e117393172a','hex'),'aes-128-cbc',
          Buffer.from('2b7e151628aed2a6abf7158809cf4f3c','hex'),Buffer.from('000102030405060708090a0b0c0d0e0f','hex'));
        if(!encrypted.toString('hex').startsWith('7649abac8119b246cee98e9b12e9197d')) throw Error('aes');
        if(utils.crypto.randomBytes(16).length !== 16) throw Error('random');
        const url=new URL('/path?q=中文','https://example.test/base');url.searchParams.set('n',3);
        if(url.searchParams.get('n') !== '3' || !url.href.includes('/path?')) throw Error('url');
        await new Promise(resolve=>setTimeout(resolve,5));
        return data.info.musicInfo.songmid;
      });
      send(EVENT_NAMES.inited,{sources:{wy:{actions:['musicUrl'],qualitys:['128k']}}});
    "#;

    let mut engine = script::Source::load(&http::Http::new(Arc::default()), raw).unwrap();
    assert_eq!(
        engine
            .invoke(&json!({"info":{"musicInfo":{"songmid":42}}}))
            .unwrap(),
        42
    );
}

#[test]
fn http_and_script_callbacks_preserve_status_json_raw_redirects_and_errors() {
    let server = Server::new(|path| match path {
        "/redirect" => (302, Vec::new(), "Location: /json\r\n".into()),
        "/json" => (
            200,
            br#"{"code":200,"text":"hello"}"#.to_vec(),
            "Content-Type: application/json\r\n".into(),
        ),
        _ => (503, b"offline".to_vec(), String::new()),
    });

    let http = http::Http::new(Arc::default());
    let response = http
        .fetch(&format!("{}/redirect", server.url), &json!({}))
        .unwrap();

    assert_eq!(response.body["text"], "hello");
    assert!(response.raw.starts_with(b"{"));
    assert_eq!(
        http.fetch(&format!("{}/bad", server.url), &json!({}))
            .err()
            .unwrap(),
        "HTTP 503"
    );

    let raw = format!(
        r#"lx.on('request',()=>new Promise((resolve,reject)=>lx.request('{}/redirect',{{}},(e,r,b)=>e?reject(e):resolve({{status:r.statusCode,text:b.text,raw:r.raw.toString()}}))));lx.send('inited',{{sources:{{wy:{{actions:['musicUrl'],qualitys:['128k']}}}}}});"#,
        server.url
    );

    let mut engine = script::Source::load(&http, &raw).unwrap();
    let value = engine.invoke(&json!({})).unwrap();
    assert_eq!(value["status"], 200);
    assert_eq!(value["text"], "hello");
    assert!(value["raw"].as_str().unwrap().contains("code"));
}

#[test]
fn pending_network_and_infinite_scripts_cancel_promptly() {
    let server = Server::new(|_| {
        std::thread::sleep(Duration::from_secs(2));
        (200, b"late".to_vec(), String::new())
    });

    let flag = Arc::new(AtomicBool::new(false));
    let cancel = flag.clone();

    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        cancel.store(true, Ordering::Relaxed);
    });

    let start = Instant::now();
    assert!(
        http::Http::new(flag)
            .fetch(&server.url, &json!({}))
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    canceller.join().unwrap();
    let flag = Arc::new(AtomicBool::new(false));
    let cancel = flag.clone();

    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        cancel.store(true, Ordering::Relaxed);
    });

    assert!(script::Source::load(&http::Http::new(flag), "while(true) {}").is_err());
    canceller.join().unwrap();
}

#[test]
fn search_metadata_cover_cache_and_invalid_images_are_preserved() {
    let http = http::Http::new(Arc::default());

    let body = json!({"code":200,"result":{"songCount":90,"songs":[{"id":42,"name":"歌名","duration":215678,
        "artists":[{"name":"歌手一"},{"name":"歌手二"}],"album":{"id":9,"name":"专辑","picUrl":"https://example.test/cover"}}]}});

    let result = service::parse_search(&http, body).unwrap();
    assert_eq!(result.total, 90);
    assert_eq!(result.tracks[0].artist, "歌手一、歌手二");
    assert_eq!(result.tracks[0].duration, Some(215.678));
    assert_eq!(
        result.tracks[0].online.as_ref().unwrap().music_info["songmid"],
        42
    );
    assert!(service::parse_search(&http, json!({"code":403})).is_err());
    assert!(
        service::parse_search(&http, json!({"code":200,"result":{"songCount":0}}))
            .unwrap()
            .tracks
            .is_empty()
    );

    let server = Server::new(|path| {
        (
            200,
            if path == "/cover" {
                b"\x89PNG\r\n\x1a\nfixture-image-more-than-32-bytes".to_vec()
            } else {
                b"<html>offline</html>".to_vec()
            },
            String::new(),
        )
    });

    let directory = temporary("covers");
    let url = format!("{}/cover", server.url);
    let file = service::cache_cover(&http, &url, &directory).unwrap();
    assert!(file.is_file());
    assert!(service::cache_cover(&http, &format!("{}/bad", server.url), &directory).is_err());
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    drop(server);
    assert_eq!(service::cache_cover(&http, &url, &directory).unwrap(), file);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_source_audio_advances_in_order_and_download_decodes_without_node() {
    let address = Arc::new(Mutex::new(String::new()));
    let endpoint = address.clone();
    let mut wav = vec![0u8; 2048];
    wav[..4].copy_from_slice(b"RIFF");
    wav[4..8].copy_from_slice(&2040u32.to_le_bytes());
    wav[8..16].copy_from_slice(b"WAVEfmt ");
    wav[16..20].copy_from_slice(&16u32.to_le_bytes());
    wav[20..22].copy_from_slice(&1u16.to_le_bytes());
    wav[22..24].copy_from_slice(&1u16.to_le_bytes());
    wav[24..28].copy_from_slice(&44100u32.to_le_bytes());
    wav[28..32].copy_from_slice(&88200u32.to_le_bytes());
    wav[32..34].copy_from_slice(&2u16.to_le_bytes());
    wav[34..36].copy_from_slice(&16u16.to_le_bytes());
    wav[36..40].copy_from_slice(b"data");
    wav[40..44].copy_from_slice(&2004u32.to_le_bytes());
    let expected = wav.clone();

    let server = Server::new(move |path| {
        if path == "/audio" {
            return (200, wav.clone(), "Content-Type: audio/wav\r\n".into());
        }
        if path == "/first.js" || path == "/second.js" {
            let suffix = if path == "/first.js" {
                "/bad"
            } else {
                "/audio"
            };

            let url = format!("{}{suffix}", endpoint.lock().unwrap());
            let script = format!(
                "lx.on('request',async info=>{{if(!['320k','128k'].includes(info.info.type)||info.info.musicInfo.songmid!==42)throw Error('contract');return info.info.type==='320k'?'{}/bad-high':'{url}'}});lx.send('inited',{{sources:{{fixture:{{actions:['musicUrl'],qualitys:['128k','320k']}}}}}});",
                endpoint.lock().unwrap()
            );

            return (200, script.into_bytes(), String::new());
        }
        (503, b"offline".to_vec(), String::new())
    });

    *address.lock().unwrap() = server.url.clone();

    let sources = [
        BuiltinSource {
            name: "first",
            url: Box::leak(format!("{}/first.js", server.url).into_boxed_str()),
        },
        BuiltinSource {
            name: "second",
            url: Box::leak(format!("{}/second.js", server.url).into_boxed_str()),
        },
    ];

    let mut track = service::netease_track(
        &json!({"id":42,"name":"fixture","album":{"picUrl":"https://example.test/cover"}}),
    );

    track.online.as_mut().unwrap().source = "fixture".into();
    let directory = temporary("prepare");

    let prepared = service::prepare(
        &http::Http::new(Arc::default()),
        &track,
        &sources,
        &directory,
    )
    .unwrap();

    assert_eq!(prepared.source, "second");
    assert_eq!(prepared.quality, "128k");
    assert_eq!(
        *server.requests.lock().unwrap(),
        [
            "/first.js",
            "/bad-high",
            "/bad",
            "/second.js",
            "/bad-high",
            "/audio"
        ]
    );
    assert_eq!(std::fs::read(&prepared.file).unwrap(), expected);

    let _decoder = rodio::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(&prepared.file).unwrap(),
    ))
    .unwrap();

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "requires live online services"]
fn live_search_covers_resolve_and_decode_without_node() {
    let result = search("晴天 周杰伦", 1, Arc::default()).expect("search");
    assert!(!result.tracks.is_empty());
    assert!(result.tracks.iter().all(|track| {
        track
            .online
            .as_ref()
            .is_some_and(|online| online.source == "wy")
    }));
    let covers = covers(&result.tracks[..2]).expect("covers");
    assert!(
        covers
            .iter()
            .all(|cover| cover.file.as_ref().is_some_and(|file| file.is_file()))
    );
    let prepared = prepare(&result.tracks[0], Arc::default()).expect("resolve");

    let _decoder = rodio::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(&prepared.file).unwrap(),
    ))
    .expect("decode");

    println!(
        "source={} quality={} file={}",
        prepared.source,
        prepared.quality,
        prepared.file.display()
    );
}
