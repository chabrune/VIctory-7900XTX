//! How the picture reaches the screen.
//!
//! A console title draws into a fixed 4K target and the console shows it;
//! on a PC the layer owns that last step, so the mode, the screen, the
//! vertical sync and the frame cap are its to decide. Defaults keep what
//! every measured run so far used: a plain window, no sync, no cap.
//!
//! Configured in `victory.toml` (`[display]`, see `victory.example.toml`);
//! environment variables take precedence (`VICTORY_WINDOW_MODE`,
//! `VICTORY_MONITOR`, `VICTORY_VSYNC`, `VICTORY_FPS_LIMIT`).

use std::sync::OnceLock;

/// How the title's window is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// A normal resizable window with its frame, fitted to the screen.
    #[default]
    Windowed,
    /// No frame at all, covering the chosen screen: the fastest way to
    /// fullscreen on a PC, and the one that lets overlays and alt-tab work.
    Borderless,
    /// Fullscreen as Windows 10 and 11 do it for flip-model swap chains: a
    /// window covering the screen that the system shows directly
    /// ("independent flip"), with the latency of the old exclusive mode,
    /// tearing and variable refresh. The same window as `borderless` today;
    /// DXGI's exclusive state is not used, because its messages to the
    /// title's window can deadlock against the title's own thread (a first
    /// try hung until the title's watchdog ended the run).
    Fullscreen,
}

impl Mode {
    /// Reads a mode's name; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "windowed" | "window" => Some(Self::Windowed),
            "borderless" | "fullscreen-borderless" => Some(Self::Borderless),
            "fullscreen" | "exclusive" => Some(Self::Fullscreen),
            _ => None,
        }
    }

    /// The name [`Mode::parse`] reads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windowed => "windowed",
            Self::Borderless => "borderless",
            Self::Fullscreen => "fullscreen",
        }
    }
}

/// The display settings of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Display {
    /// Windowed, borderless or exclusive fullscreen.
    pub mode: Mode,
    /// Which screen, in the system's order (0 is the main one).
    pub monitor: usize,
    /// Wait for the screen between frames (also removes tearing).
    pub vsync: bool,
    /// At most this many frames a second; `None` for no cap. A cap above
    /// the title's own rate changes nothing.
    pub frame_limit: Option<u32>,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            mode: Mode::Windowed,
            monitor: 0,
            vsync: false,
            frame_limit: None,
        }
    }
}

impl Display {
    /// How long one frame may take at the cap, if there is one.
    pub fn frame_period(&self) -> Option<std::time::Duration> {
        let limit = self.frame_limit?;
        (limit > 0).then(|| std::time::Duration::from_secs_f64(1.0 / f64::from(limit)))
    }
}

static SETTINGS: OnceLock<Display> = OnceLock::new();

/// The settings of this process: [`set`] if the front end called it, else
/// the configuration file and the environment, else the defaults.
pub fn settings() -> &'static Display {
    SETTINGS.get_or_init(|| crate::Config::load().map_or_else(|_| Display::default(), |c| c.display))
}

/// Fixes the settings of this process (`true` if this call was the first).
pub fn set(display: Display) -> bool {
    SETTINGS.set(display).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_round_trip() {
        for mode in [Mode::Windowed, Mode::Borderless, Mode::Fullscreen] {
            assert_eq!(Mode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(Mode::parse("  FullScreen "), Some(Mode::Fullscreen));
        assert_eq!(Mode::parse("window"), Some(Mode::Windowed));
        assert_eq!(Mode::parse("sideways"), None);
    }

    #[test]
    fn the_defaults_are_what_every_run_measured_so_far_used() {
        let d = Display::default();
        assert_eq!(d.mode, Mode::Windowed);
        assert_eq!((d.monitor, d.vsync, d.frame_limit), (0, false, None));
        assert_eq!(d.frame_period(), None);
        assert_eq!(
            Display {
                frame_limit: Some(60),
                ..d
            }
            .frame_period(),
            Some(std::time::Duration::from_secs_f64(1.0 / 60.0))
        );
        // A cap of zero is no cap, not a division by zero.
        assert_eq!(
            Display {
                frame_limit: Some(0),
                ..d
            }
            .frame_period(),
            None
        );
    }
}
