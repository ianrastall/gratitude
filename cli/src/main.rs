//! Console driver for the Show Player core. Reads one command per line
//! from stdin (UTF-8) and writes every event and every reply as one JSON line
//! on stdout. The test scripts in `tests/` speak this protocol.
//!
//! Usage: abharness [--data <dir>] [--ao <driver>] [--interval <ms>] [--engine <libmpv-2.dll>]
//! Commands: open <path> | close | play | pause | seek <s> | rel <s>
//!           chapter <n> | next | prev | vol <0-100> | mute <0|1>
//!           status | chapters | sleep <ms> | settle | quit
//! End of input is the same as quit.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use show_player_core::player::{
    DEFAULT_SAVE_INTERVAL, ErrorCode, EventKind, EventSink, LogLevel, Player, PlayerConfig, PlayerEvent, State,
};

static OUTPUT: Mutex<()> = Mutex::new(());
static LAST_EVENT_MS: AtomicU64 = AtomicU64::new(0);

fn start_time() -> Instant {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn elapsed_ms() -> u64 {
    start_time().elapsed().as_millis() as u64
}

fn line(text: &str) {
    let _guard = OUTPUT.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{text}");
    let _ = out.flush();
}

fn json(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

struct Printer;

impl EventSink for Printer {
    fn event(&mut self, e: &PlayerEvent) {
        LAST_EVENT_MS.store(elapsed_ms(), Ordering::Relaxed);
        let mut text = format!(
            "{{\"t\":{},\"ev\":\"{}\",\"gen\":{},\"state\":\"{}\",\"pos\":{:.3},\"dur\":{:.3},\"chapter\":{}",
            elapsed_ms(),
            e.kind.name(),
            e.generation,
            e.state.name(),
            e.position,
            e.duration,
            e.chapter
        );
        if e.request_id != 0 {
            text += &format!(",\"request\":{}", e.request_id);
        }
        if e.error != ErrorCode::None || e.engine_error != 0 {
            text += &format!(",\"error\":{},\"engine_error\":{}", e.error as i32, e.engine_error);
        }
        if e.kind == EventKind::VolumeChanged {
            text += &format!(",\"volume\":{:.1},\"mute\":{}", e.volume, i32::from(e.mute));
        }
        text.push('}');
        line(&text);
    }

    fn log(&mut self, level: LogLevel, message: &str) {
        line(&format!(
            "{{\"t\":{},\"ev\":\"log\",\"level\":{},\"message\":{}}}",
            elapsed_ms(),
            level as i32,
            json(message)
        ));
    }
}

fn reply(command: &str, status: i32) {
    line(&format!("{{\"t\":{},\"ev\":\"reply\",\"command\":{},\"status\":{}}}", elapsed_ms(), json(command), status));
}

fn status_code(result: Result<(), ErrorCode>) -> i32 {
    result.err().map_or(0, |code| code as i32)
}

fn print_status(player: &Player, chapters: bool) {
    let s = player.snapshot();
    let book = s.book.as_deref();
    if chapters {
        let list: Vec<String> = book
            .map(|b| {
                b.chapters
                    .iter()
                    .map(|c| format!("{{\"start\":{:.3},\"end\":{:.3},\"title\":{}}}", c.start, c.end, json(&c.title)))
                    .collect()
            })
            .unwrap_or_default();
        line(&format!(
            "{{\"t\":{},\"ev\":\"chapters\",\"gen\":{},\"list\":[{}]}}",
            elapsed_ms(),
            s.generation,
            list.join(",")
        ));
        return;
    }
    let chapter = book.and_then(|b| usize::try_from(s.chapter).ok().and_then(|i| b.chapters.get(i)));
    let field = |f: fn(&show_player_core::player::BookInfo) -> &str| json(book.map_or("", f));
    line(&format!(
        "{{\"t\":{},\"ev\":\"status\",\"gen\":{},\"state\":\"{}\",\"pos\":{:.3},\"dur\":{:.3},\"chapter\":{},\
         \"chapter_count\":{},\"chapter_title\":{},\"chapter_start\":{:.3},\"error\":{},\"engine_error\":{},\
         \"volume\":{:.1},\"mute\":{},\"save_failed\":{},\"was_finished\":{},\"file_size\":{},\"path\":{},\
         \"title\":{},\"author\":{},\"comment\":{},\"codec\":{}}}",
        elapsed_ms(),
        s.generation,
        s.state.name(),
        s.position,
        s.duration,
        s.chapter,
        book.map_or(0, |b| b.chapters.len()),
        json(chapter.map_or("", |c| c.title.as_str())),
        chapter.map_or(0.0, |c| c.start),
        s.error as i32,
        s.engine_error,
        s.volume,
        i32::from(s.mute),
        i32::from(s.save_failed),
        i32::from(s.was_finished),
        book.map_or(0, |b| b.file_size),
        field(|b| &b.path),
        field(|b| &b.title),
        field(|b| &b.author),
        field(|b| &b.comment),
        field(|b| &b.codec),
    ));
}

/// Blocks until the player has left Loading and has been quiet for a
/// moment, so a script can read the result of the commands before it.
fn settle(player: &Player) {
    let begin = elapsed_ms();
    loop {
        std::thread::sleep(Duration::from_millis(20));
        let now = elapsed_ms();
        let quiet = now.saturating_sub(LAST_EVENT_MS.load(Ordering::Relaxed)) >= 150 && now - begin >= 150;
        if (quiet && player.snapshot().state != State::Loading) || now - begin > 10_000 {
            return;
        }
    }
}

fn default_engine() -> PathBuf {
    show_player_core::default_engine_path()
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../third_party/libmpv/libmpv-2.dll")))
}

fn main() {
    start_time();
    let mut data_dir = None;
    let mut audio_output = None;
    let mut engine_path = None;
    let mut save_interval = DEFAULT_SAVE_INTERVAL;
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [option, value] = args.as_slice() {
        let result = match option.as_str() {
            "--inspect-show" => Some(
                show_player_core::shows::inspect(std::path::Path::new(value), "")
                    .and_then(|d| serde_json::to_string_pretty(&d).map_err(|e| e.to_string())),
            ),
            "--resolve-etree" => Some(
                show_player_core::shows::torrent_url(&show_player_core::shows::Recording {
                    source: "etree".into(),
                    key: format!("etree-{value}"),
                    ..Default::default()
                })
                .and_then(|d| serde_json::to_string_pretty(&d).map_err(|e| e.to_string())),
            ),
            "--resolve-archive" => Some(
                show_player_core::shows::torrent_url(&show_player_core::shows::Recording {
                    source: "Archive.org".into(),
                    key: value.clone(),
                    ..Default::default()
                })
                .and_then(|d| serde_json::to_string_pretty(&d).map_err(|e| e.to_string())),
            ),
            _ => None,
        };
        if let Some(result) = result {
            match result {
                Ok(text) => println!("{text}"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            return;
        }
    }
    for pair in args.chunks(2) {
        match pair {
            [option, value] if option == "--data" => data_dir = Some(PathBuf::from(value)),
            [option, value] if option == "--ao" => audio_output = Some(value.clone()),
            [option, value] if option == "--engine" => engine_path = Some(PathBuf::from(value)),
            [option, value] if option == "--interval" => {
                save_interval =
                    value.parse().ok().filter(|&ms| ms > 0).map_or(DEFAULT_SAVE_INTERVAL, Duration::from_millis)
            }
            _ => {
                eprintln!("unknown option");
                std::process::exit(2);
            }
        }
    }
    let Some(data_dir) = data_dir.or_else(show_player_core::default_data_dir) else {
        reply("create", ErrorCode::InvalidArgument as i32);
        std::process::exit(1);
    };
    let config =
        PlayerConfig { data_dir, engine_path: engine_path.unwrap_or_else(default_engine), audio_output, save_interval };
    let player = match Player::start(config, Box::new(Printer)) {
        Ok(player) => player,
        Err(e) => {
            reply("create", e.code as i32);
            std::process::exit(1);
        }
    };
    reply("create", 0);

    let mut next_request = 0u64;
    for text in std::io::stdin().lock().lines() {
        let Ok(text) = text else { break };
        let text = text.trim_end_matches('\r');
        if text.is_empty() {
            continue;
        }
        let (name, arg) = text.split_once(' ').unwrap_or((text, ""));
        let number: f64 = arg.trim().parse().unwrap_or(0.0);
        let integer: i32 = arg.trim().parse().unwrap_or(0);
        let status = match name {
            "quit" => break,
            "status" | "chapters" => {
                print_status(&player, name == "chapters");
                continue;
            }
            "settle" => {
                settle(&player);
                continue;
            }
            "sleep" => {
                std::thread::sleep(Duration::from_millis(u64::try_from(integer).unwrap_or(0)));
                continue;
            }
            "open" => {
                next_request += 1;
                status_code(player.open(arg, next_request))
            }
            "close" => status_code(player.close_book()),
            "play" => status_code(player.play()),
            "pause" => status_code(player.pause()),
            "seek" => status_code(player.seek_absolute(number)),
            "rel" => status_code(player.seek_relative(number)),
            "chapter" => status_code(player.select_chapter(integer)),
            "next" => status_code(player.next_chapter()),
            "prev" => status_code(player.previous_chapter()),
            "vol" => status_code(player.set_volume(number)),
            "mute" => status_code(player.set_mute(integer != 0)),
            _ => ErrorCode::Unsupported as i32,
        };
        reply(name, status);
    }

    let status = status_code(player.shutdown(Some(Duration::from_secs(3))));
    reply("shutdown", status);
    if status != 0 {
        // The position is already durable; let the process end.
        std::process::exit(1);
    }
}
