//! Browser glue: the canvas, and game files handed over by the page.
//!
//! The page (`crates/gfa-web/index.html`) owns the folder picker and the
//! IndexedDB cache, since both are plain DOM work. It passes each file it
//! wants to Rust with [`gfa_add_file`], then calls [`gfa_files_done`]. The
//! files never leave the user's browser.

use std::cell::RefCell;

use gfa_assets::{AssetSource, MemSource};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::wasm_bindgen;
use winit::platform::web::WindowAttributesExtWebSys;
use winit::window::WindowAttributes;

type Sink = Box<dyn Fn(Box<dyn AssetSource>)>;

thread_local! {
    static CANVAS_ID: RefCell<String> = RefCell::new(String::from("gfa-canvas"));
    static FILES: RefCell<Option<MemSource>> = const { RefCell::new(None) };
    static SINK: RefCell<Option<Sink>> = RefCell::new(None);
}

pub fn set_canvas_id(id: &str) {
    CANVAS_ID.with(|c| *c.borrow_mut() = id.to_string());
}

/// Where finished file sets go: the event loop.
pub fn set_data_sink(sink: impl Fn(Box<dyn AssetSource>) + 'static) {
    SINK.with(|s| *s.borrow_mut() = Some(Box::new(sink)));
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
                    config.level = n;
                }
            }
            Some(("game", v)) => {
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
