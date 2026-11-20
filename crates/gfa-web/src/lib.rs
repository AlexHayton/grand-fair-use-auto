//! Grand Fair Use Auto in the browser.
//!
//! Built with Trunk (`trunk serve` in this folder). The page provides a
//! `<canvas id="gfa-canvas">`; the game draws into it with WebGL2. The data
//! folder picker and IndexedDB cache arrive in milestone M2.

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    log::info!("{} {}", gfa_app::TITLE, env!("CARGO_PKG_VERSION"));
    gfa_app::run_web("gfa-canvas");
}
