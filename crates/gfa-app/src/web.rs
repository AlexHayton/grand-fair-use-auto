//! Browser glue: the canvas, the game files the page hands over, and the
//! touch buttons.
//!
//! The page (`crates/gfa-web/index.html`) owns the pickers, the IndexedDB
//! cache and the touch buttons, since all are plain DOM work. For an archive
//! it calls [`gfa_unpack`], which reads only the members a level needs out
//! of the chosen `File`, a slice at a time; for a folder it asks
//! [`gfa_game_path`] which files to read. Either way it passes each file to
//! Rust with [`gfa_add_file`], then calls [`gfa_files_done`]. The files never
//! leave the user's browser.

use std::cell::RefCell;

use gfa_assets::{
    Archive, ArchiveError, AssetSource, GameIndex, MemSource, ReadAt, is_play_file,
    split_at_gtadata,
};
use gfa_sim::input::Buttons;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use winit::platform::web::WindowAttributesExtWebSys;
use winit::window::WindowAttributes;

type Sink = Box<dyn Fn(Box<dyn AssetSource>)>;
type TouchSink = Box<dyn Fn(Buttons, bool)>;

thread_local! {
    static CANVAS_ID: RefCell<String> = RefCell::new(String::from("gfa-canvas"));
    static FILES: RefCell<Option<MemSource>> = const { RefCell::new(None) };
    static SINK: RefCell<Option<Sink>> = RefCell::new(None);
    static TOUCH: RefCell<Option<TouchSink>> = RefCell::new(None);
}

pub fn set_canvas_id(id: &str) {
    CANVAS_ID.with(|c| *c.borrow_mut() = id.to_string());
}

/// Where finished file sets go: the event loop.
pub fn set_data_sink(sink: impl Fn(Box<dyn AssetSource>) + 'static) {
    SINK.with(|s| *s.borrow_mut() = Some(Box::new(sink)));
}

/// Where the touch buttons go: the event loop.
pub fn set_touch_sink(sink: impl Fn(Buttons, bool) + 'static) {
    TOUCH.with(|s| *s.borrow_mut() = Some(Box::new(sink)));
}

/// Options from the page's address: `?game=london69&level=2`.
pub fn config_from_url() -> crate::AppConfig {
    let mut config = crate::AppConfig::default();
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    for pair in search.trim_start_matches('?').split('&') {
        match pair.split_once('=') {
            Some(("level", v)) => {
                if let Ok(n) = v.parse() {
                    config.show_menu = false;
                    config.level = n;
                }
            }
            Some(("game", v)) => {
                config.show_menu = false;
                config.game = match v.to_ascii_lowercase().as_str() {
                    "london69" | "london" => gfa_assets::Dataset::London69,
                    "london61" => gfa_assets::Dataset::London61,
                    _ => gfa_assets::Dataset::Gta,
                }
            }
            _ => {}
        }
    }
    config
}

/// Draw into the page's canvas if it has one, else let winit append one.
pub fn attach_canvas(attrs: WindowAttributes) -> WindowAttributes {
    let id = CANVAS_ID.with(|c| c.borrow().clone());
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&id))
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
    match canvas {
        Some(c) => attrs
            .with_canvas(Some(c))
            .with_prevent_default(true)
            .with_focusable(true),
        None => attrs.with_append(true),
    }
}

/// Add one game file. `path` is relative to the folder that contains
/// `GTADATA/`, e.g. `GTADATA/NYC.CMP`.
#[wasm_bindgen]
pub fn gfa_add_file(path: &str, bytes: Vec<u8>) {
    FILES.with(|f| {
        f.borrow_mut()
            .get_or_insert_with(MemSource::new)
            .insert(path, bytes)
    });
}

/// The page has added every file: load the game from them.
#[wasm_bindgen]
pub fn gfa_files_done() {
    let Some(files) = FILES.with(|f| f.borrow_mut().take()) else {
        log::warn!("no game files were added");
        return;
    };
    SINK.with(|s| match s.borrow().as_ref() {
        Some(sink) => sink(Box::new(files)),
        None => log::error!("game files arrived before the game started"),
    });
}

/// An on-screen button went down (`down`) or up: `up`, `down`, `left`,
/// `right`, `fire`, `enter`, `jump`, `special`, `prev` or `next`.
#[wasm_bindgen]
pub fn gfa_touch(button: &str, down: bool) {
    let Some(b) = crate::controls::touch_button(button) else {
        log::warn!("no touch button {button:?}");
        return;
    };
    TOUCH.with(|t| {
        if let Some(sink) = t.borrow().as_ref() {
            sink(b, down);
        }
    });
}

/// For a file in a chosen folder, its game path if a level needs it:
/// `"Games/GTA/GTADATA/NYC.CMP"` is `"GTADATA/NYC.CMP"`.
#[wasm_bindgen]
pub fn gfa_game_path(relative: &str) -> Option<String> {
    split_at_gtadata(relative)
        .map(|(_, path)| path)
        .filter(|p| is_play_file(p))
}

/// The `File` the player chose, read a slice at a time: the browser hands
/// back only the bytes asked for, so a 200 MB archive is never in memory.
struct BlobReader {
    blob: web_sys::Blob,
    size: u64,
}

impl ReadAt for BlobReader {
    fn size(&self) -> u64 {
        self.size
    }

    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, ArchiveError> {
        let io = |e: JsValue| ArchiveError::Io(e.as_string().unwrap_or_else(|| format!("{e:?}")));
        let slice = self
            .blob
            .slice_with_f64_and_f64(offset as f64, (offset + len as u64) as f64)
            .map_err(io)?;
        let buf = wasm_bindgen_futures::JsFuture::from(slice.array_buffer())
            .await
            .map_err(io)?;
        let bytes = js_sys::Uint8Array::new(&buf).to_vec();
        if bytes.len() != len {
            return Err(ArchiveError::Io(format!(
                "asked for {len} bytes, the browser gave {}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }
}

fn js_error(e: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&e.to_string()).into()
}

/// Unpack the files a level needs from the game archive the player chose:
/// a RAR 5 or ZIP `File` with `GTADATA/` somewhere inside. Resolves to
/// `[[path, bytes], ...]` with game paths (`GTADATA/NYC.CMP`), or rejects
/// with an `Error` whose message says what is wrong. `progress(done, total,
/// path)` is called before each file.
#[wasm_bindgen]
pub async fn gfa_unpack(
    file: web_sys::Blob,
    progress: js_sys::Function,
) -> Result<js_sys::Array, JsValue> {
    let reader = BlobReader {
        size: file.size() as u64,
        blob: file,
    };
    let archive = Archive::open(&reader).await.map_err(js_error)?;
    let game = GameIndex::new(&archive).map_err(js_error)?;
    let wanted: Vec<(String, usize)> = game
        .files()
        .filter(|(p, _)| is_play_file(p))
        .map(|(p, i)| (p.to_string(), i))
        .collect();
    if wanted.is_empty() {
        return Err(js_error(
            "the archive's GTADATA folder has no maps or mission scripts",
        ));
    }
    let total = wanted.len() as u32;
    let out = js_sys::Array::new();
    for (n, (path, i)) in wanted.iter().enumerate() {
        let _ = progress.call3(
            &JsValue::NULL,
            &JsValue::from(n as u32),
            &JsValue::from(total),
            &JsValue::from_str(path),
        );
        let bytes = archive
            .unpack(&reader, &archive.members[*i])
            .await
            .map_err(js_error)?;
        out.push(&js_sys::Array::of2(
            &JsValue::from_str(path),
            &js_sys::Uint8Array::from(bytes.as_slice()),
        ));
    }
    log::info!(
        "{} archive: {total} game files from {:?}",
        archive.format,
        game.root
    );
    Ok(out)
}

/// The page owns the touch controls. Show them once a chapter starts.
pub fn set_playing(playing: bool) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let _ = el.set_attribute("data-playing", if playing { "true" } else { "false" });
    }
}

/// Leave the title screen for the page that chooses the game data.
pub fn show_data_picker() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("data-button"))
        && let Some(button) = el.dyn_ref::<web_sys::HtmlElement>()
    {
        button.click();
    }
}
