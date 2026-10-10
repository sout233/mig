use crate::library::{Library, LyricLine, Track, parse_translated_lrc};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::{
    fs,
    io::BufReader,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum PlayMode {
    #[default]
    Sequential,
    Repeat,
    Shuffle,
}

impl PlayMode {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Sequential => Self::Repeat,
            Self::Repeat => Self::Shuffle,
            Self::Shuffle => Self::Sequential,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Sequential => "顺序播放",
            Self::Repeat => "单曲循环",
            Self::Shuffle => "随机播放",
        }
    }
}

pub(crate) enum Command {
    Play(Vec<Track>, usize),
    Queue(Vec<Track>),
    PlayNext(Vec<Track>, std::path::PathBuf),
    Toggle,
    Next,
    Previous,
    Seek(f64),
    Volume(f32),
    Mode(PlayMode),
    Save(Library),
}

pub(crate) enum Event {
    Started(Box<Track>, f64, Vec<LyricLine>),
    Position(f64),
    Playing(bool),
    Stopped,
    Error(String),
    SaveError(String),
    Loading(String),
    Source(String),
}

pub(crate) struct Player {
    sender: Sender<Command>,
    events: Receiver<Event>,
}

impl Player {
    pub(crate) fn new() -> Self {
        let (sender, commands) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        std::thread::spawn(move || Worker::new(events).run(commands));
        Self {
            sender,
            events: receiver,
        }
    }

    pub(crate) fn send(&self, command: Command) {
        let _ = self.sender.send(command);
    }

    pub(crate) fn drain(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
}

struct Preparation {
    receiver: Receiver<Result<crate::online::PreparedTrack, String>>,
    cancel: Arc<AtomicBool>,
}

struct Worker {
    events: Sender<Event>,
    output: Option<(OutputStream, OutputStreamHandle)>,
    sink: Option<Sink>,
    queue: Vec<Track>,
    ix: usize,
    next_track: Option<std::path::PathBuf>,
    mode: PlayMode,
    volume: f32,
    playing: bool,
    preparation: Option<Preparation>,
}

impl Worker {
    fn new(events: Sender<Event>) -> Self {
        Self {
            events,
            output: None,
            sink: None,
            queue: Vec::new(),
            ix: 0,
            next_track: None,
            mode: PlayMode::Sequential,
            volume: 0.65,
            playing: false,
            preparation: None,
        }
    }

    fn run(&mut self, commands: Receiver<Command>) {
        loop {
            match commands.recv_timeout(Duration::from_millis(200)) {
                Ok(command) => self.handle(command),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.cancel_preparation();
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }

            let prepared = self
                .preparation
                .as_ref()
                .and_then(|preparation| preparation.receiver.try_recv().ok());

            if let Some(result) = prepared {
                self.preparation = None;
                match result {
                    Ok(prepared) => {
                        let _ = self.events.send(Event::Source(format!(
                            "{} · {}",
                            prepared.source, prepared.quality
                        )));

                        let result = self.open_track(Some(prepared));
                        self.finish_start(result);
                    }
                    Err(error) => {
                        let _ = self.events.send(Event::Error(error));
                    }
                }
            }
            if let Some(sink) = &self.sink {
                let _ = self
                    .events
                    .send(Event::Position(sink.get_pos().as_secs_f64()));

                if self.playing && sink.empty() {
                    if self.mode == PlayMode::Sequential
                        && self.ix + 1 >= self.queue.len()
                        && self.next_track.is_none()
                    {
                        self.playing = false;
                        let _ = self.events.send(Event::Playing(false));
                    } else {
                        self.ix = self.advance_index(true);
                        self.start();
                    }
                }
            }
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Play(queue, ix) => {
                if ix < queue.len() {
                    self.queue = queue;
                    self.next_track = None;
                    self.ix = ix;
                    self.start();
                }
            }
            Command::Queue(queue) => self.update_queue(queue),
            Command::PlayNext(queue, path) => {
                self.update_queue(queue);
                self.next_track = self
                    .queue
                    .iter()
                    .any(|track| track.path == path)
                    .then_some(path);
            }
            Command::Toggle => {
                if self.preparation.is_some() {
                    self.cancel_preparation();
                    let _ = self.events.send(Event::Playing(false));
                    return;
                }
                if let Some(sink) = &self.sink {
                    if sink.empty() {
                        self.start();
                    } else {
                        self.playing = !self.playing;
                        if self.playing {
                            sink.play();
                        } else {
                            sink.pause();
                        }
                        let _ = self.events.send(Event::Playing(self.playing));
                    }
                } else if !self.queue.is_empty() {
                    self.start();
                }
            }
            Command::Next if !self.queue.is_empty() => {
                self.ix = self.advance_index(false);
                self.start();
            }
            Command::Previous if !self.queue.is_empty() => {
                self.next_track = None;
                self.ix = (self.ix + self.queue.len() - 1) % self.queue.len();
                self.start();
            }
            Command::Seek(seconds) => {
                if let Some(sink) = &self.sink
                    && let Err(error) = sink.try_seek(Duration::from_secs_f64(seconds.max(0.0)))
                {
                    let _ = self
                        .events
                        .send(Event::Error(format!("无法跳转播放位置：{error}")));
                }
            }
            Command::Volume(volume) => {
                self.volume = volume.clamp(0.0, 1.0);
                if let Some(sink) = &self.sink {
                    sink.set_volume(self.volume);
                }
            }
            Command::Mode(mode) => self.mode = mode,
            Command::Save(library) => {
                if let Err(error) = library.save() {
                    let _ = self
                        .events
                        .send(Event::SaveError(format!("音乐库未保存：{error}")));
                }
            }
            _ => {}
        }
    }

    fn advance_index(&mut self, automatic: bool) -> usize {
        self.next_track
            .take()
            .and_then(|path| self.queue.iter().position(|track| track.path == path))
            .unwrap_or_else(|| next_index(self.ix, self.queue.len(), self.mode, automatic))
    }

    fn update_queue(&mut self, queue: Vec<Track>) {
        if self
            .next_track
            .as_ref()
            .is_some_and(|path| !queue.iter().any(|track| &track.path == path))
        {
            self.next_track = None;
        }
        let current = self.queue.get(self.ix).map(|track| track.path.clone());
        let retained = current.and_then(|path| queue.iter().position(|track| track.path == path));
        let next = self.ix.min(queue.len().saturating_sub(1));
        self.queue = queue;
        if self.queue.is_empty() {
            self.cancel_preparation();
            if let Some(sink) = self.sink.take() {
                sink.stop();
            }
            self.playing = false;
            self.ix = 0;
            let _ = self.events.send(Event::Stopped);
        } else if let Some(ix) = retained {
            self.ix = ix;
        } else {
            self.ix = next;
            self.start();
        }
    }

    fn start(&mut self) {
        self.cancel_preparation();
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.playing = false;
        let _ = self.events.send(Event::Playing(false));
        let _ = self.events.send(Event::Loading(format!(
            "正在加载“{}”",
            self.queue[self.ix].title
        )));

        if self.queue[self.ix].online.is_some() {
            let (sender, receiver) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            let track = self.queue[self.ix].clone();
            std::thread::spawn(move || {
                let _ = sender.send(crate::online::prepare(&track, flag));
            });
            self.preparation = Some(Preparation { receiver, cancel });
        } else {
            let result = self.open_track(None);
            self.finish_start(result);
        }
    }

    fn cancel_preparation(&mut self) {
        if let Some(preparation) = self.preparation.take() {
            preparation.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn finish_start(&mut self, result: Result<(Sink, Track, f64, Vec<LyricLine>), String>) {
        match result {
            Ok((sink, track, duration, lyrics)) => {
                sink.set_volume(self.volume);
                self.sink = Some(sink);
                self.playing = true;

                let _ = self
                    .events
                    .send(Event::Started(Box::new(track), duration, lyrics));

                let _ = self.events.send(Event::Playing(true));
            }
            Err(error) => {
                let _ = self.events.send(Event::Playing(false));
                let _ = self.events.send(Event::Error(error));
            }
        }
    }

    fn open_track(
        &mut self,
        prepared: Option<crate::online::PreparedTrack>,
    ) -> Result<(Sink, Track, f64, Vec<LyricLine>), String> {
        let mut track = self.queue[self.ix].clone();

        let local_lyrics = if prepared.is_none() {
            crate::local_metadata::enrich(&mut track)
        } else {
            Vec::new()
        };

        let path = prepared
            .as_ref()
            .map(|prepared| &prepared.file)
            .unwrap_or(&track.path);

        let file = fs::File::open(path)
            .map_err(|error| format!("无法打开“{}”：{error}。请重新导入文件。", track.title))?;

        let decoder = Decoder::new(BufReader::new(file))
            .map_err(|error| format!("无法解码“{}”：{error}", track.title))?;

        let duration = decoder
            .total_duration()
            .map(|value| value.as_secs_f64())
            .unwrap_or(track.duration.unwrap_or(0.0));

        if self.output.is_none() {
            self.output = Some(
                OutputStream::try_default()
                    .map_err(|error| format!("无法连接音频设备：{error}"))?,
            );
        }

        let sink =
            Sink::try_new(&self.output.as_ref().unwrap().1).map_err(|error| error.to_string())?;

        sink.append(decoder);

        let lyrics = prepared
            .map(|prepared| parse_translated_lrc(&prepared.lyric, &prepared.translated_lyric))
            .unwrap_or(local_lyrics);

        Ok((sink, track, duration, lyrics))
    }
}

fn next_index(ix: usize, len: usize, mode: PlayMode, automatic: bool) -> usize {
    if len <= 1 || (automatic && mode == PlayMode::Repeat) {
        return ix;
    }
    if mode == PlayMode::Shuffle {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as usize;

        (ix + 1 + seed % (len - 1)) % len
    } else {
        (ix + 1) % len
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_edits_preserve_playback_and_clear_emits_stop() {
        let (events, receiver) = mpsc::channel();
        let mut worker = Worker::new(events);
        let a = Track::from_path("a.mp3".into());
        let b = Track::from_path("b.mp3".into());
        let c = Track::from_path("c.mp3".into());
        worker.queue = vec![a.clone(), b.clone(), c.clone()];
        worker.ix = 1;
        worker.playing = true;
        worker.update_queue(vec![c, b, a]);
        assert_eq!(worker.ix, 1);
        assert!(worker.playing);
        assert!(receiver.try_recv().is_err());
        worker.update_queue(Vec::new());
        assert!(!worker.playing);
        assert!(matches!(receiver.try_recv(), Ok(Event::Stopped)));
    }

    #[test]
    fn explicit_next_overrides_shuffle_and_repeat_once() {
        let (events, _) = mpsc::channel();
        let mut worker = Worker::new(events);
        worker.queue = ["a.mp3", "b.mp3", "c.mp3"]
            .into_iter()
            .map(|path| Track::from_path(path.into()))
            .collect();
        for mode in [PlayMode::Shuffle, PlayMode::Repeat] {
            worker.mode = mode;
            worker.next_track = Some("c.mp3".into());
            assert_eq!(worker.advance_index(true), 2);
            assert!(worker.next_track.is_none());
        }
    }

    #[test]
    fn repeat_only_repeats_for_automatic_advance() {
        assert_eq!(next_index(1, 3, PlayMode::Repeat, true), 1);
        assert_eq!(next_index(1, 3, PlayMode::Repeat, false), 2);
        assert_eq!(next_index(2, 3, PlayMode::Sequential, false), 0);
    }

    #[test]
    fn shuffle_never_repeats_current_track_when_alternatives_exist() {
        for ix in 0..5 {
            assert_ne!(next_index(ix, 5, PlayMode::Shuffle, true), ix);
        }
    }
}
