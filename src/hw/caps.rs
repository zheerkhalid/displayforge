//! Parser for MCCS capabilities strings.
//!
//! Monitors answer a capabilities request with an ASCII string such as
//! `(prot(monitor)type(LCD)model(X)cmds(01 02)vcp(10 12 60(0F 11))mccs_ver(2.1))`.
//! Real strings are often sloppy: the outer parentheses may be missing,
//! there can be stray whitespace, line breaks or NULs, hex codes may run
//! together (`vcp(101216)`), and the string may be cut short. The parser
//! below takes what it can and never fails.
use crate::model::Capabilities;

/// Parses a capabilities string such as
/// `(prot(monitor)type(LCD)model(X)cmds(01 02)vcp(10 12 60(0F 11))mccs_ver(2.1))`.
/// Never fails: unknown or broken parts are skipped.
pub fn parse(text: &str) -> Capabilities {
    let clean: String = text
        .chars()
        .map(|c| if c == '\0' || c.is_control() { ' ' } else { c })
        .collect();
    let body = strip_outer(clean.trim());
    let mut caps = Capabilities::default();
    for (name, content) in sections(body) {
        let value = content.split_whitespace().collect::<Vec<_>>().join(" ");
        match name.to_ascii_lowercase().as_str() {
            "prot" => caps.protocol = non_empty(value),
            "type" => caps.kind = non_empty(value),
            "model" => caps.model = non_empty(value),
            "mccs_ver" => caps.mccs_version = non_empty(value),
            "vcp" => parse_vcp(content, &mut caps),
            _ => {}
        }
    }
    caps
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Removes one pair of parentheses around the whole string, if they match.
fn strip_outer(s: &str) -> &str {
    let b = s.as_bytes();
    if b.first() != Some(&b'(') {
        return s;
    }
    // Find the parenthesis that closes the first one.
    let mut depth = 0i32;
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    // Only strip when it closes at the very end; otherwise the
                    // first section has no name (malformed) and we keep it all.
                    return if s[i + 1..].trim().is_empty() {
                        &s[1..i]
                    } else {
                        s
                    };
                }
            }
            _ => {}
        }
    }
    // Truncated: the outer parenthesis never closes.
    &s[1..]
}

/// Splits `name(content)name2(content2)` into pairs at the top level. A
/// section cut off by the end of the string gets the rest as content.
fn sections(s: &str) -> Vec<(&str, &str)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // Skip anything that cannot start a name.
        while i < b.len() && !is_name_byte(b[i]) {
            if b[i] == b'(' {
                // A nameless group: skip it whole.
                i = matching(b, i).map(|e| e + 1).unwrap_or(b.len());
            } else {
                i += 1;
            }
        }
        let start = i;
        while i < b.len() && is_name_byte(b[i]) {
            i += 1;
        }
        let name = &s[start..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'(' {
            continue;
        }
        let end = matching(b, i);
        let content_end = end.unwrap_or(b.len());
        if !name.is_empty() {
            out.push((name, &s[i + 1..content_end]));
        }
        i = end.map(|e| e + 1).unwrap_or(b.len());
    }
    out
}

fn is_name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The index of the parenthesis that closes the one at `open`.
fn matching(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, &c) in b.iter().enumerate().skip(open) {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parses the inside of `vcp(...)`: codes, each optionally followed by a
/// parenthesised list of allowed values.
fn parse_vcp(content: &str, caps: &mut Capabilities) {
    let b = content.as_bytes();
    let mut i = 0;
    let mut last: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_hexdigit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                i += 1;
            }
            for code in hex_bytes(&content[start..i]) {
                caps.vcp.entry(code).or_default();
                last = Some(code);
            }
        } else if c == b'(' {
            let end = matching(b, i);
            let inner = &content[i + 1..end.unwrap_or(b.len())];
            if let Some(code) = last {
                let values = top_level_values(inner);
                let list = caps.vcp.entry(code).or_default();
                for v in values {
                    if !list.contains(&v) {
                        list.push(v);
                    }
                }
            }
            last = None;
            i = end.map(|e| e + 1).unwrap_or(b.len());
        } else {
            i += 1;
        }
    }
}

/// Hex values directly inside a value list; deeper nesting is ignored.
fn top_level_values(inner: &str) -> Vec<u16> {
    let b = inner.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            c if c.is_ascii_hexdigit() => {
                let start = i;
                while i < b.len() && b[i].is_ascii_hexdigit() {
                    i += 1;
                }
                if depth == 0 {
                    out.extend(hex_bytes(&inner[start..i]).map(u16::from));
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// A run of hex digits as bytes: `"10"` -> 0x10, `"101216"` -> 0x10 0x12
/// 0x16 (codes written without spaces), `"F"` -> 0x0F. An odd-length run
/// longer than one digit is ambiguous and skipped.
fn hex_bytes(run: &str) -> impl Iterator<Item = u8> + '_ {
    let ok = run.len() == 1 || run.len().is_multiple_of(2);
    let step = if run.len() == 1 { 1 } else { 2 };
    (0..if ok { run.len() } else { 0 })
        .step_by(step)
        .filter_map(move |i| u8::from_str_radix(&run[i..(i + step).min(run.len())], 16).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_string() {
        let s = "(prot(monitor)type(LCD)model(ACME 27Q)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(05 08 0B) 16 18 1A 52 60(0F 11 12) AC AE B2 B6 C6 C8 C9 D6(01 04 05) DF 62)mswhql(1)asset_eep(40)mccs_ver(2.1))";
        let c = parse(s);
        assert_eq!(c.protocol.as_deref(), Some("monitor"));
        assert_eq!(c.kind.as_deref(), Some("LCD"));
        assert_eq!(c.model.as_deref(), Some("ACME 27Q"));
        assert_eq!(c.mccs_version.as_deref(), Some("2.1"));
        assert!(c.supports(0x10) && c.supports(0x12) && c.supports(0x62));
        assert_eq!(c.values(0x60), &[0x0F, 0x11, 0x12]);
        assert_eq!(c.values(0x14), &[0x05, 0x08, 0x0B]);
        assert_eq!(c.values(0xD6), &[0x01, 0x04, 0x05]);
        assert!(c.values(0x10).is_empty());
        // cmds codes are not VCP codes.
        assert!(!c.supports(0xE3));
    }

    #[test]
    fn missing_outer_parens_whitespace_and_nuls() {
        let s = "  prot(monitor)\r\n type(lcd)  model( Wide View 24 )\n vcp( 10 12  14(01 05 08)\n 60(  01 03 11 ) )  mccs_ver(2.2)\0\0";
        let c = parse(s);
        assert_eq!(c.model.as_deref(), Some("Wide View 24"));
        assert_eq!(c.kind.as_deref(), Some("lcd"));
        assert_eq!(c.values(0x14), &[0x01, 0x05, 0x08]);
        assert_eq!(c.values(0x60), &[0x01, 0x03, 0x11]);
        assert_eq!(c.mccs_version.as_deref(), Some("2.2"));
    }

    #[test]
    fn unknown_sections_and_vcpname_are_skipped() {
        let s = "(prot(monitor)type(LED)model(PX-32)cmds(01 02 03 0C E3 F3)vcp(10 12 60(11 12 0F) DC(00 02 03 05) E0(00 01(02 03)))vcpname(10(Lum) DC(Picture Mode (Game)))mswhql(1)asset_eep(40)mpu(01)mccs_ver(2.2))";
        let c = parse(s);
        assert_eq!(c.model.as_deref(), Some("PX-32"));
        assert_eq!(c.values(0xDC), &[0x00, 0x02, 0x03, 0x05]);
        // Nested lists keep only the top-level values.
        assert_eq!(c.values(0xE0), &[0x00, 0x01]);
        assert_eq!(c.mccs_version.as_deref(), Some("2.2"));
        assert_eq!(c.vcp.len(), 5);
    }

    #[test]
    fn codes_without_spaces() {
        let c = parse("(vcp(101216 60(0F11))model(Q))");
        assert!(c.supports(0x10) && c.supports(0x12) && c.supports(0x16));
        assert_eq!(c.values(0x60), &[0x0F, 0x11]);
        assert_eq!(c.model.as_deref(), Some("Q"));
    }

    #[test]
    fn truncated_and_garbage_never_panic() {
        let c = parse("(prot(monitor)type(LCD)vcp(10 12 60(0F 11");
        assert!(c.supports(0x10));
        assert_eq!(c.values(0x60), &[0x0F, 0x11]);
        for s in [
            "",
            "(",
            ")",
            "((((",
            "))))",
            "vcp(",
            "vcp)",
            "()()()",
            "model",
            "\0\0\0",
            "vcp(ZZ 1 123 G0(()) 10",
            "(prot(monitor)model(ünïcödé ✓)vcp(10))",
            "vcp(10(((((",
        ] {
            let _ = parse(s);
        }
        let c = parse("(prot(monitor)model(ünïcödé ✓)vcp(10))");
        assert!(c.supports(0x10));
    }

    #[test]
    fn single_digit_codes_and_values() {
        let c = parse("vcp(2 4 10 14(1 5 8))");
        assert!(c.supports(0x02) && c.supports(0x04));
        assert_eq!(c.values(0x14), &[1, 5, 8]);
    }
}
