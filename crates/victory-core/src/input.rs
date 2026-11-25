//! The keyboard's stand-in for a gamepad, and how to change it.
//!
//! A console title only listens to pads. On a PC with no pad connected the
//! layer turns keys into one ([`crate::Config`]'s `[input]` table, see
//! `victory.example.toml`); this module holds that table and nothing else.
//! The defaults are the keys every run so far used; a French keyboard can
//! bind `z`/`q` instead of `w`/`a`, and any key can be unbound.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// A gamepad control a key can stand in for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Menu,
    View,
    A,
    B,
    X,
    Y,
    Up,
    Down,
    Left,
    Right,
    LeftShoulder,
    RightShoulder,
    LeftStick,
    RightStick,
    /// The left trigger, fully pulled while the key is down.
    LeftTrigger,
    RightTrigger,
    /// The left stick, pushed along one axis.
    LeftUp,
    LeftDown,
    LeftLeft,
    LeftRight,
    /// The right stick, pushed along one axis.
    RightUp,
    RightDown,
    RightLeft,
    RightRight,
}

/// Every action, in the order `victory config` lists them.
pub const ACTIONS: [Action; 24] = [
    Action::Menu,
    Action::View,
    Action::A,
    Action::B,
    Action::X,
    Action::Y,
    Action::Up,
    Action::Down,
    Action::Left,
    Action::Right,
    Action::LeftShoulder,
    Action::RightShoulder,
    Action::LeftStick,
    Action::RightStick,
    Action::LeftTrigger,
    Action::RightTrigger,
    Action::LeftUp,
    Action::LeftDown,
    Action::LeftLeft,
    Action::LeftRight,
    Action::RightUp,
    Action::RightDown,
    Action::RightLeft,
    Action::RightRight,
];

impl Action {
    /// Reads an action's name as the config file spells it.
    pub fn parse(text: &str) -> Option<Self> {
        let name = text.trim().to_ascii_lowercase().replace('-', "_");
        ACTIONS
            .iter()
            .copied()
            .find(|a| a.as_str() == name.as_str())
    }

    /// The name the config file uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::View => "view",
            Self::A => "a",
            Self::B => "b",
            Self::X => "x",
            Self::Y => "y",
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::LeftShoulder => "lb",
            Self::RightShoulder => "rb",
            Self::LeftStick => "ls",
            Self::RightStick => "rs",
            Self::LeftTrigger => "lt",
            Self::RightTrigger => "rt",
            Self::LeftUp => "ls_up",
            Self::LeftDown => "ls_down",
            Self::LeftLeft => "ls_left",
            Self::LeftRight => "ls_right",
            Self::RightUp => "rs_up",
            Self::RightDown => "rs_down",
            Self::RightLeft => "rs_left",
            Self::RightRight => "rs_right",
        }
    }
}

/// A key, by Windows virtual-key code (letters and digits are their ASCII
/// codes in upper case, as `GetAsyncKeyState` takes them).
pub type Key = u8;

/// One key per action; a missing action is unbound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    keys: BTreeMap<&'static str, (Action, Key)>,
}

impl Default for Bindings {
    fn default() -> Self {
        let mut bindings = Self {
            keys: BTreeMap::new(),
        };
        for (action, key) in DEFAULTS {
            bindings.set(action, Some(key));
        }
        bindings
    }
}

impl Bindings {
    /// The key of `action`, if it is bound.
    pub fn key(&self, action: Action) -> Option<Key> {
        self.keys.get(action.as_str()).map(|&(_, key)| key)
    }

    /// Every binding, in action-name order.
    pub fn pairs(&self) -> impl Iterator<Item = (Action, Key)> + '_ {
        self.keys.values().copied()
    }

    fn set(&mut self, action: Action, key: Option<Key>) {
        match key {
            Some(key) => {
                self.keys.insert(action.as_str(), (action, key));
            }
            None => {
                self.keys.remove(action.as_str());
            }
        }
    }

    /// The defaults with `table`'s overrides applied. `""`, `none` or `off`
    /// unbinds a key; anything unknown is refused by name.
    pub fn from_table(table: &BTreeMap<String, String>) -> Result<Self, String> {
        let mut bindings = Self::default();
        for (name, text) in table {
            let action = Action::parse(name).ok_or_else(|| {
                format!(
                    "unknown action {name:?} in [input]; known: {}",
                    ACTIONS.map(Action::as_str).join(", ")
                )
            })?;
            let key = parse_key(text).map_err(|why| format!("[input] {name}: {why}"))?;
            bindings.set(action, key);
        }
        Ok(bindings)
    }
}

/// The keys every run so far used (see the README's table).
pub const DEFAULTS: [(Action, Key); 24] = [
    (Action::Menu, 0x1B),          // Esc
    (Action::View, 0x09),          // Tab
    (Action::A, 0x0D),             // Enter
    (Action::B, 0x08),             // Backspace
    (Action::X, b'X'),
    (Action::Y, b'Y'),
    (Action::Up, 0x26),
    (Action::Down, 0x28),
    (Action::Left, 0x25),
    (Action::Right, 0x27),
    (Action::LeftShoulder, b'Q'),
    (Action::RightShoulder, b'E'),
    (Action::LeftStick, b'F'),
    (Action::RightStick, b'H'),
    (Action::LeftTrigger, b'Z'),
    (Action::RightTrigger, b'C'),
    (Action::LeftUp, b'W'),
    (Action::LeftDown, b'S'),
    (Action::LeftLeft, b'A'),
    (Action::LeftRight, b'D'),
    (Action::RightUp, b'I'),
    (Action::RightDown, b'K'),
    (Action::RightLeft, b'J'),
    (Action::RightRight, b'L'),
];

/// The keys known by name (virtual-key code, the names that read as it; the
/// first name is the one [`key_name`] gives). Punctuation keys are left out
/// on purpose: their codes (`VK_OEM_*`) sit on different characters from
/// one keyboard layout to the next, so a `;` in the file could not say which
/// key it means.
const NAMED: &[(Key, &[&str])] = &[
    (0x08, &["backspace"]),
    (0x09, &["tab"]),
    (0x0D, &["enter", "return"]),
    (0x10, &["shift"]),
    (0x11, &["ctrl", "control"]),
    (0x12, &["alt"]),
    (0x13, &["pause"]),
    (0x14, &["capslock", "caps"]),
    (0x1B, &["escape", "esc"]),
    (0x20, &["space", "spacebar"]),
    (0x21, &["pageup", "pgup"]),
    (0x22, &["pagedown", "pgdn"]),
    (0x23, &["end"]),
    (0x24, &["home"]),
    (0x25, &["left", "arrowleft"]),
    (0x26, &["up", "arrowup"]),
    (0x27, &["right", "arrowright"]),
    (0x28, &["down", "arrowdown"]),
    (0x2D, &["insert", "ins"]),
    (0x2E, &["delete", "del"]),
    (0x60, &["numpad0"]),
    (0x61, &["numpad1"]),
    (0x62, &["numpad2"]),
    (0x63, &["numpad3"]),
    (0x64, &["numpad4"]),
    (0x65, &["numpad5"]),
    (0x66, &["numpad6"]),
    (0x67, &["numpad7"]),
    (0x68, &["numpad8"]),
    (0x69, &["numpad9"]),
    (0xA0, &["lshift", "leftshift"]),
    (0xA1, &["rshift", "rightshift"]),
    (0xA2, &["lctrl", "leftctrl"]),
    (0xA3, &["rctrl", "rightctrl"]),
];

/// Reads a key's name: one letter or digit, or one of the named keys
/// `Ok(None)` for the names that mean "no key".
pub fn parse_key(text: &str) -> Result<Option<Key>, String> {
    let name = text.trim();
    let lower = name.to_ascii_lowercase();
    if matches!(lower.as_str(), "" | "none" | "off" | "unbound") {
        return Ok(None);
    }
    if let Some(&(key, _)) = NAMED.iter().find(|(_, names)| names.contains(&lower.as_str())) {
        return Ok(Some(key));
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        return Ok(Some(0x6F + n));
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        // Only letters and digits have a virtual-key code equal to their
        // character; `%` would be the left arrow's code, `;` no key at all.
        (Some(c), None) if c.is_ascii_alphanumeric() => Ok(Some(c.to_ascii_uppercase() as u8)),
        (Some(c), None) => Err(format!(
            "{c:?}: punctuation keys move from one keyboard layout to another, so they \
             cannot be named by their character; use a letter, a digit or a named key"
        )),
        _ => Err(format!(
            "{name:?} is not a key: use one letter or digit (a, 5) or a name such as \
             escape, tab, enter, space, up, f1, numpad0"
        )),
    }
}

/// How a key is written in the config file: the inverse of [`parse_key`].
pub fn key_name(key: Key) -> String {
    if key.is_ascii_uppercase() || key.is_ascii_digit() {
        return (key as char).to_string();
    }
    if let Some((_, names)) = NAMED.iter().find(|&&(k, _)| k == key) {
        return names[0].to_owned();
    }
    if (0x70..=0x87).contains(&key) {
        return format!("f{}", key - 0x6F);
    }
    format!("virtual key {key:#04x}")
}

static BINDINGS: OnceLock<Bindings> = OnceLock::new();

/// The bindings of this process: [`set`] if the front end called it, else
/// the configuration file, else the defaults.
pub fn settings() -> &'static Bindings {
    BINDINGS.get_or_init(|| {
        crate::Config::load().map_or_else(|_| Bindings::default(), |c| c.input)
    })
}

/// Fixes the bindings of this process (`true` if this call was the first).
pub fn set(bindings: Bindings) -> bool {
    BINDINGS.set(bindings).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, k)| ((*a).to_owned(), (*k).to_owned()))
            .collect()
    }

    #[test]
    fn the_defaults_are_the_keys_the_readme_lists() {
        let b = Bindings::default();
        assert_eq!(b.key(Action::A), Some(0x0D));
        assert_eq!(b.key(Action::LeftUp), Some(b'W'));
        assert_eq!(b.key(Action::LeftTrigger), Some(b'Z'));
        assert_eq!(b.pairs().count(), 24);
        for (action, _) in DEFAULTS {
            assert!(b.key(action).is_some(), "{} is bound", action.as_str());
        }
    }

    #[test]
    fn a_french_keyboard_moves_the_left_stick() {
        let b = Bindings::from_table(&table(&[
            ("ls_up", "z"),
            ("ls_left", "q"),
            ("ls_down", "s"),
            ("ls_right", "d"),
        ]))
        .unwrap();
        assert_eq!(b.key(Action::LeftUp), Some(b'Z'));
        assert_eq!(b.key(Action::LeftLeft), Some(b'Q'));
        // Untouched actions keep their default.
        assert_eq!(b.key(Action::Menu), Some(0x1B));
    }

    #[test]
    fn a_key_can_be_unbound_and_named_keys_are_read() {
        let b = Bindings::from_table(&table(&[
            ("menu", "off"),
            ("a", "space"),
            ("b", "Escape"),
            ("x", "F5"),
            ("y", "numpad4"),
        ]))
        .unwrap();
        assert_eq!(b.key(Action::Menu), None);
        assert_eq!(b.key(Action::A), Some(0x20));
        assert_eq!(b.key(Action::B), Some(0x1B));
        assert_eq!(b.key(Action::X), Some(0x74));
        assert_eq!(b.key(Action::Y), Some(0x64));
    }

    #[test]
    fn unknown_actions_and_keys_are_refused_by_name() {
        let why = Bindings::from_table(&table(&[("jump", "space")])).unwrap_err();
        assert!(why.contains("jump") && why.contains("menu"), "{why}");
        let why = Bindings::from_table(&table(&[("a", "ctrl+shift")])).unwrap_err();
        assert!(why.contains("a") && why.contains("ctrl+shift"), "{why}");
        assert_eq!(parse_key(""), Ok(None));
        assert_eq!(parse_key("A"), Ok(Some(b'A')));
        // Punctuation is refused: '%' would be the left arrow's code.
        assert!(parse_key(";").unwrap_err().contains("layout"));
        assert!(parse_key("%").is_err());
    }

    #[test]
    fn every_key_name_reads_back_as_its_key() {
        for key in 0..=255u8 {
            let name = key_name(key);
            if name.starts_with("virtual key") {
                continue;
            }
            assert_eq!(parse_key(&name), Ok(Some(key)), "{name}");
        }
        assert_eq!(key_name(0x25), "left", "not '%'");
        assert_eq!(key_name(0x7B), "f12");
        assert_eq!(key_name(b'W'), "W");
    }
}
