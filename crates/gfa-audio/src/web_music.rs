//! Browser-native `<audio>` (`HTMLAudioElement`) music/radio streaming.
//!
//! Decodes OGG Vorbis natively off the main thread via a `Blob` object URL,
//! avoiding synchronous full-track decoding into WASM linear memory. Falls back
//! to Kira's `StaticSoundData` when `canPlayType('audio/ogg; codecs="vorbis"')`
//! returns empty.

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Blob, BlobPropertyBag, HtmlAudioElement, Url};

pub(crate) struct WebMusic {
    el: HtmlAudioElement,
    object_url: Option<String>,
    on_meta: Option<Closure<dyn FnMut()>>,
    volume: f64,
}

impl WebMusic {
    /// Create a browser `<audio>` player if the browser supports OGG Vorbis.
    pub(crate) fn new(music_volume: u8) -> Option<Self> {
        let el = HtmlAudioElement::new().ok()?;
        let can_vorbis = !el.can_play_type("audio/ogg; codecs=\"vorbis\"").is_empty();
        if !can_vorbis {
            log::info!("browser <audio> does not report OGG Vorbis support; falling back to Kira");
            return None;
        }
        let volume = linear_volume(music_volume);
        el.set_preload("auto");
        el.set_volume(volume);
        log::info!("using browser-native <audio> streaming for OGG music/radio");
        Some(Self {
            el,
            object_url: None,
            on_meta: None,
            volume,
        })
    }

    pub(crate) fn set_volume(&mut self, music_volume: u8) {
        self.volume = linear_volume(music_volume);
        self.el.set_volume(self.volume);
    }

    pub(crate) fn stop(&mut self) {
        let _ = self.el.pause();
        self.el.set_onloadedmetadata(None);
        self.on_meta = None;
        self.el.set_src("");
        if let Some(url) = self.object_url.take() {
            let _ = Url::revoke_object_url(&url);
        }
    }

    /// Stream an OGG buffer via `URL.createObjectURL(new Blob([bytes], { type: "audio/ogg" }))`.
    /// Returns `false` if Blob/URL creation failed so the caller can fall back to Kira.
    pub(crate) fn play_ogg(&mut self, bytes: &[u8], looping: bool, start_secs: f64) -> bool {
        self.stop();

        let u8arr = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::of1(&u8arr.into());
        let opts = BlobPropertyBag::new();
        opts.set_type("audio/ogg");
        let Ok(blob) = Blob::new_with_u8_array_sequence_and_options(&parts, &opts) else {
            return false;
        };
        let Ok(url) = Url::create_object_url_with_blob(&blob) else {
            return false;
        };

        self.el.set_loop(looping);
        self.el.set_volume(self.volume);

        if start_secs > 0.0 {
            let el_clone = self.el.clone();
            let cb = Closure::<dyn FnMut()>::new(move || {
                let dur = el_clone.duration();
                if dur.is_finite() && dur > 0.5 {
                    el_clone.set_current_time(start_secs % dur);
                }
                let _ = el_clone.play();
            });
            self.el
                .set_onloadedmetadata(Some(cb.as_ref().unchecked_ref()));
            self.on_meta = Some(cb);
        }

        self.el.set_src(&url);
        self.object_url = Some(url);
        let _ = self.el.play();
        true
    }
}

impl Drop for WebMusic {
    fn drop(&mut self) {
        self.stop();
    }
}

fn linear_volume(level: u8) -> f64 {
    f64::from(level.min(7)) / 7.0
}
