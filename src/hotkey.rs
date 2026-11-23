//! Hotkey strings such as "Ctrl+Alt+Up" <-> modifier flags and virtual-key codes.
//!
//! The modifier flags are those of `RegisterHotKey`; the key codes are
//! Windows virtual-key codes.

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;
pub const MOD_WIN: u32 = 0x8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keys {
    pub mods: u32,
    pub vk: u32,
}

/// Named keys other than letters, digits and F-keys: (canonical name, code).
const NAMED: &[(&str, u32)] = &[
    ("Up", 0x26),
    ("Down", 0x28),
    ("Left", 0x25),
    ("Right", 0x27),
    ("Home", 0x24),
    ("End", 0x23),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Space", 0x20),
    ("Plus", 0xBB),
    ("Minus", 0xBD),
    ("Comma", 0xBC),
    ("Period", 0xBE),
];

/// Other spellings accepted when parsing (lower case, no spaces).
const ALIASES: &[(&str, &str)] = &[
    ("arrowup", "Up"),
    ("arrowdown", "Down"),
    ("arrowleft", "Left"),
    ("arrowright", "Right"),
    ("pgup", "PageUp"),
    ("pgdn", "PageDown"),
    ("pgdown", "PageDown"),
    ("ins", "Insert"),
    ("del", "Delete"),
    ("spacebar", "Space"),
    ("+", "Plus"),
    ("=", "Plus"),
    ("equals", "Plus"),
    ("-", "Minus"),
    (",", "Comma"),
    (".", "Period"),
    ("dot", "Period"),
];

fn modifier(word: &str) -> Option<u32> {
    match word {
        "ctrl" | "control" | "ctl" => Some(MOD_CONTROL),
        "alt" => Some(MOD_ALT),
        "shift" => Some(MOD_SHIFT),
        "win" | "windows" | "super" | "meta" | "cmd" => Some(MOD_WIN),
        _ => None,
    }
}

/// The virtual-key code of a key name (any accepted spelling, lower case, no spaces).
fn key_code(word: &str) -> Option<u32> {
    let word = ALIASES
        .iter()
        .find(|(a, _)| *a == word)
        .map(|(_, c)| c.to_ascii_lowercase())
        .unwrap_or_else(|| word.to_string());
    let w = word.as_str();
    let b = w.as_bytes();
    if b.len() == 1 && b[0].is_ascii_alphabetic() {
        return Some(b[0].to_ascii_uppercase() as u32);
    }
    if b.len() == 1 && b[0].is_ascii_digit() {
        return Some(b[0] as u32);
    }
    if let Some(n) = w.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&n) && !w[1..].starts_with('0') {
            return Some(0x70 + n - 1);
        }
    }
    for prefix in ["numpad", "num"] {
        if let Some(rest) = w.strip_prefix(prefix) {
            if rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit() {
                return Some(0x60 + (rest.as_bytes()[0] - b'0') as u32);
            }
        }
    }
    NAMED
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(w))
        .map(|(_, c)| *c)
}

/// The canonical name of a virtual-key code, if it is one we use.
pub fn key_name(vk: u32) -> Option<String> {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => Some((vk as u8 as char).to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x70 + 1)),
        0x60..=0x69 => Some(format!("Num{}", vk - 0x60)),
        _ => NAMED
            .iter()
            .find(|(_, c)| *c == vk)
            .map(|(n, _)| n.to_string()),
    }
}

/// Parses a hotkey such as "Ctrl+Alt+Up" or "ctrl + shift + page up".
/// Case and spaces do not matter. One key and at least one modifier are
/// needed (F13 to F24 may stand alone).
pub fn parse(text: &str) -> Result<Keys, String> {
    let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if squeezed.is_empty() {
        return Err("The shortcut is empty.".into());
    }
    let lower = squeezed.to_lowercase();
    // "Ctrl++" means Ctrl and the plus key.
    let (body, plus_key) = match lower.strip_suffix("++") {
        Some(rest) => (rest.to_string(), true),
        None if lower == "+" => (String::new(), true),
        None => (lower.clone(), false),
    };
    let mut parts: Vec<&str> = if body.is_empty() {
        Vec::new()
    } else {
        body.split('+').collect()
    };
    if plus_key {
        parts.push("plus");
    }
    let mut mods = 0;
    let mut vk: Option<u32> = None;
    for part in parts {
        if part.is_empty() {
            return Err(format!(
                "\"{}\" has an empty part between the + signs.",
                text.trim()
            ));
        }
        if let Some(m) = modifier(part) {
            if mods & m != 0 {
                return Err(format!("A modifier is repeated in \"{}\".", text.trim()));
            }
            mods |= m;
            continue;
        }
        match key_code(part) {
            Some(code) => {
                if vk.is_some() {
                    return Err("Use only one key together with Ctrl, Alt, Shift or Win.".into());
                }
                vk = Some(code);
            }
            None => return Err(format!("\"{part}\" is not a key DisplayForge knows.")),
        }
    }
    let vk = vk.ok_or("Add a key to the modifiers, for example Ctrl+Alt+Up.")?;
    let lone_ok = (0x7C..=0x87).contains(&vk); // F13..F24
    if mods == 0 && !lone_ok {
        return Err("Add at least one of Ctrl, Alt, Shift or Win to the key.".into());
    }
    Ok(Keys { mods, vk })
}

/// The canonical text of a hotkey, e.g. "Ctrl+Alt+Up".
pub fn format(keys: Keys) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (flag, name) in [
        (MOD_CONTROL, "Ctrl"),
        (MOD_ALT, "Alt"),
        (MOD_SHIFT, "Shift"),
        (MOD_WIN, "Win"),
    ] {
        if keys.mods & flag != 0 {
            parts.push(name.into());
        }
    }
    parts.push(key_name(keys.vk).unwrap_or_else(|| format!("Key{:02X}", keys.vk)));
    parts.join("+")
}

/// Cleans up a typed hotkey into its canonical form ("ctrl + alt + up" -> "Ctrl+Alt+Up").
pub fn normalize(text: &str) -> Result<String, String> {
    parse(text).map(format)
}

/// Maps an egui `Key::name()` to our key name, for recording a hotkey.
/// Returns `None` for keys a hotkey cannot use.
pub fn from_egui_name(name: &str) -> Option<&'static str> {
    const LETTERS: [&str; 26] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
        "S", "T", "U", "V", "W", "X", "Y", "Z",
    ];
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    const FKEYS: [&str; 24] = [
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14",
        "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
    ];
    if let Some(k) = LETTERS
        .iter()
        .chain(DIGITS.iter())
        .chain(FKEYS.iter())
        .find(|k| k.eq_ignore_ascii_case(name))
    {
        return Some(*k);
    }
    let named = match name {
        "Up" | "ArrowUp" => "Up",
        "Down" | "ArrowDown" => "Down",
        "Left" | "ArrowLeft" => "Left",
        "Right" | "ArrowRight" => "Right",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "PageUp",
        "PageDown" => "PageDown",
        "Insert" => "Insert",
        "Delete" => "Delete",
        "Space" => "Space",
        // The "=/+" key is VK_OEM_PLUS on every layout.
        "Plus" | "+" | "Equals" | "=" => "Plus",
        "Minus" | "-" => "Minus",
        "Comma" | "," => "Comma",
        "Period" | "." => "Period",
        _ => return None,
    };
    Some(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_shortcuts() {
        assert_eq!(
            parse("Ctrl+Alt+Up").unwrap(),
            Keys {
                mods: MOD_CONTROL | MOD_ALT,
                vk: 0x26
            }
        );
        assert_eq!(parse("ctrl + alt + b").unwrap().vk, 0x42);
        assert_eq!(parse("Control+Shift+1").unwrap().vk, 0x31);
        assert_eq!(
            parse("Win+F5").unwrap(),
            Keys {
                mods: MOD_WIN,
                vk: 0x74
            }
        );
        assert_eq!(parse("Alt+Page Up").unwrap().vk, 0x21);
        assert_eq!(parse("Ctrl+ArrowDown").unwrap().vk, 0x28);
        assert_eq!(parse("Ctrl+Num7").unwrap().vk, 0x67);
        assert_eq!(parse("Ctrl+Plus").unwrap().vk, 0xBB);
        assert_eq!(parse("Ctrl++").unwrap().vk, 0xBB);
        assert_eq!(parse("Ctrl+Alt+-").unwrap().vk, 0xBD);
        assert_eq!(parse("Shift+Comma").unwrap().vk, 0xBC);
        assert_eq!(parse("Alt+Period").unwrap().vk, 0xBE);
        assert_eq!(parse("Ctrl+Space").unwrap().vk, 0x20);
        assert_eq!(parse("Ctrl+Del").unwrap().vk, 0x2E);
        assert_eq!(parse("F13").unwrap(), Keys { mods: 0, vk: 0x7C });
        assert_eq!(parse("f24").unwrap().vk, 0x87);
        assert_eq!(parse("Ctrl+Alt+End").unwrap().vk, 0x23);
    }

    #[test]
    fn rejects_with_plain_words() {
        for bad in [
            "",
            "   ",
            "Up",
            "A",
            "F12",
            "Ctrl",
            "Ctrl+Alt",
            "Ctrl+Up+Down",
            "Ctrl+Foo",
            "Ctrl++Up",
            "Ctrl+Ctrl+A",
            "Ctrl+F25",
            "Ctrl+F0",
            "Ctrl+F01",
            "Ctrl+Num10",
        ] {
            let e = parse(bad).expect_err(bad);
            assert!(e.len() > 10, "{bad}: {e}");
        }
        assert!(parse("Ctrl+Foo").unwrap_err().contains("foo"));
        assert!(parse("Up").unwrap_err().contains("Ctrl"));
    }

    #[test]
    fn formats_canonically_and_round_trips() {
        assert_eq!(normalize("alt+ctrl+up").unwrap(), "Ctrl+Alt+Up");
        assert_eq!(
            normalize("win + shift + pgdn").unwrap(),
            "Shift+Win+PageDown"
        );
        assert_eq!(normalize("CTRL+=").unwrap(), "Ctrl+Plus");
        assert_eq!(
            format(Keys {
                mods: MOD_CONTROL,
                vk: 0x07
            }),
            "Ctrl+Key07"
        );
        let mut all = Vec::new();
        for c in 'A'..='Z' {
            all.push(c.to_string());
        }
        for d in 0..10 {
            all.push(d.to_string());
            all.push(format!("Num{d}"));
        }
        for f in 1..=24 {
            all.push(format!("F{f}"));
        }
        for (n, _) in NAMED {
            all.push(n.to_string());
        }
        for key in all {
            for mods in ["Ctrl", "Alt+Shift", "Ctrl+Alt+Shift+Win"] {
                let text = format!("{mods}+{key}");
                let k = parse(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
                assert_eq!(format(k), text);
                assert_eq!(parse(&format(k)).unwrap(), k);
            }
        }
    }

    #[test]
    fn default_hotkeys_parse() {
        for h in crate::settings::default_hotkeys() {
            assert_eq!(normalize(&h.keys).unwrap(), h.keys);
        }
    }

    #[test]
    fn egui_names() {
        assert_eq!(from_egui_name("Up"), Some("Up"));
        assert_eq!(from_egui_name("ArrowLeft"), Some("Left"));
        assert_eq!(from_egui_name("A"), Some("A"));
        assert_eq!(from_egui_name("7"), Some("7"));
        assert_eq!(from_egui_name("F24"), Some("F24"));
        assert_eq!(from_egui_name("PageDown"), Some("PageDown"));
        assert_eq!(from_egui_name("Equals"), Some("Plus"));
        assert_eq!(from_egui_name("Minus"), Some("Minus"));
        assert_eq!(from_egui_name("Escape"), None);
        assert_eq!(from_egui_name("F25"), None);
        assert_eq!(from_egui_name("Tab"), None);
        // Every mapped name parses with a modifier.
        for n in [
            "Up", "Space", "Comma", "Period", "Delete", "Insert", "Home", "End", "Q", "0",
        ] {
            let ours = from_egui_name(n).unwrap();
            assert!(parse(&format!("Ctrl+{ours}")).is_ok(), "{n}");
        }
    }
}
