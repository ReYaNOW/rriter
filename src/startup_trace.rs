//! Startup stage timing, printed to stderr when `RRITER_STARTUP_TRACE` is set (and not "0").
//! Created as the first statement of `main` so `t0` is the process start as seen by Rust code.

use std::time::Instant;

/// The logo PNG shared by the early decode thread and the renderer's texture upload.
pub(crate) const LOGO_PNG: &[u8] = include_bytes!("icons/icon.png");

pub(crate) struct StartupTrace {
    t0: Instant,
    enabled: bool,
    first_frame_done: bool,
    logo: Option<std::thread::JoinHandle<Option<image::RgbaImage>>>,
}

/// The welcome logo is drawn at 110 logical px; half the 816 px source (one mip level) keeps
/// it sharp up to scale 3.7 and cuts the GL upload + mipmap chain on the first frame by 4x.
const LOGO_TEXTURE_SIZE: u32 = 408;

fn decode_logo(png: &[u8]) -> Option<image::RgbaImage> {
    let decoded = image::load_from_memory(png).ok()?;
    Some(if decoded.width() > LOGO_TEXTURE_SIZE || decoded.height() > LOGO_TEXTURE_SIZE {
        image::imageops::thumbnail(&decoded, LOGO_TEXTURE_SIZE, LOGO_TEXTURE_SIZE)
    } else {
        decoded.into_rgba8()
    })
}

impl StartupTrace {
    pub(crate) fn new() -> Self {
        let enabled = std::env::var_os("RRITER_STARTUP_TRACE").is_some_and(|value| value != "0");
        Self { t0: Instant::now(), enabled, first_frame_done: false, logo: None }
    }

    /// A trace that never prints (tests, callers without a startup).
    pub(crate) fn disabled() -> Self {
        Self { t0: Instant::now(), enabled: false, first_frame_done: true, logo: None }
    }

    /// Decodes the logo on a helper thread while the main thread initializes the GL driver.
    pub(crate) fn start_logo_decode(&mut self, png: &'static [u8]) {
        self.logo = std::thread::Builder::new()
            .name("rriter-logo-decode".to_string())
            .spawn(move || decode_logo(png))
            .ok();
    }

    /// Joins the helper decode (a panicked thread gives `None`); decodes inline when none was started.
    pub(crate) fn take_logo(&mut self, png: &'static [u8]) -> Option<image::RgbaImage> {
        match self.logo.take() {
            Some(handle) => handle.join().ok().flatten(),
            None => decode_logo(png),
        }
    }

    pub(crate) fn mark(&self, stage: &str) {
        if self.enabled {
            eprintln!("startup: {stage} +{:.2}ms", self.t0.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// Marks the first frame with real content; later calls do nothing.
    pub(crate) fn first_frame(&mut self) {
        if !self.first_frame_done {
            self.first_frame_done = true;
            self.mark("first-frame");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_logo, LOGO_PNG, LOGO_TEXTURE_SIZE};

    #[test]
    fn startup_logo_is_downsampled_to_the_upload_size() {
        let logo = decode_logo(LOGO_PNG).expect("embedded logo should decode");
        assert_eq!(logo.dimensions(), (LOGO_TEXTURE_SIZE, LOGO_TEXTURE_SIZE));
    }
}
