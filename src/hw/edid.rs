//! EDID parser and lookup.
use crate::model::EdidInfo;

const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

/// Parses a 128-byte (or longer) EDID block. `None` if the header or checksum is wrong.
pub fn parse(bytes: &[u8]) -> Option<EdidInfo> {
    if bytes.len() < 128 {
        return None;
    }
    let b = &bytes[..128];
    if b[..8] != HEADER {
        return None;
    }
    if b.iter().fold(0u8, |a, &x| a.wrapping_add(x)) != 0 {
        return None;
    }

    let manufacturer_id = pnp_id(u16::from_be_bytes([b[8], b[9]]))?;
    let manufacturer = maker_name(&manufacturer_id)
        .map(str::to_string)
        .unwrap_or_else(|| manufacturer_id.clone());
    let product_code = u16::from_le_bytes([b[10], b[11]]);
    let serial_number = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);

    let (week, year) = match (b[16], b[17]) {
        (_, 0) => (None, None),
        // 0xFF: the year is a model year, there is no week.
        (0xFF, y) => (None, Some(1990 + y as u16)),
        (0, y) => (None, Some(1990 + y as u16)),
        (w, y) if w <= 54 => (Some(w), Some(1990 + y as u16)),
        (_, y) => (None, Some(1990 + y as u16)),
    };

    let version = (b[18], b[19]);
    let input = b[20];
    let digital = input & 0x80 != 0;
    let bit_depth = if digital && (version.0, version.1) >= (1, 4) {
        match (input >> 4) & 0x07 {
            1 => Some(6),
            2 => Some(8),
            3 => Some(10),
            4 => Some(12),
            5 => Some(14),
            6 => Some(16),
            _ => None,
        }
    } else {
        None
    };
    let (width_cm, height_cm) = (b[21], b[22]);

    let mut native = None;
    let mut name = None;
    let mut serial_text = None;
    for d in 0..4 {
        let desc = &b[54 + d * 18..54 + (d + 1) * 18];
        let clock = u16::from_le_bytes([desc[0], desc[1]]);
        if clock != 0 {
            if native.is_none() {
                native = detailed_timing(desc);
            }
            continue;
        }
        match desc[3] {
            0xFC => name = descriptor_text(desc).or(name),
            0xFF => serial_text = descriptor_text(desc).or(serial_text),
            _ => {}
        }
    }
    let serial = serial_text.or(if serial_number != 0 {
        Some(serial_number.to_string())
    } else {
        None
    });

    Some(EdidInfo {
        manufacturer_id,
        manufacturer,
        product_code,
        name,
        serial,
        week,
        year,
        width_cm,
        height_cm,
        version,
        digital,
        bit_depth,
        native,
    })
}

/// The three-letter PNP id packed into 3 x 5 bits ('A' = 1).
fn pnp_id(word: u16) -> Option<String> {
    let mut s = String::with_capacity(3);
    for shift in [10, 5, 0] {
        let v = ((word >> shift) & 0x1F) as u8;
        if !(1..=26).contains(&v) {
            return None;
        }
        s.push((b'A' + v - 1) as char);
    }
    Some(s)
}

/// Width, height and refresh rate from an 18-byte detailed timing descriptor.
fn detailed_timing(d: &[u8]) -> Option<(u32, u32, f32)> {
    let clock_hz = u16::from_le_bytes([d[0], d[1]]) as f64 * 10_000.0;
    let h_active = d[2] as u32 | ((d[4] as u32 & 0xF0) << 4);
    let h_blank = d[3] as u32 | ((d[4] as u32 & 0x0F) << 8);
    let v_active = d[5] as u32 | ((d[7] as u32 & 0xF0) << 4);
    let v_blank = d[6] as u32 | ((d[7] as u32 & 0x0F) << 8);
    let total = (h_active + h_blank) as f64 * (v_active + v_blank) as f64;
    if h_active == 0 || v_active == 0 || total == 0.0 {
        return None;
    }
    let mut refresh = (clock_hz / total) as f32;
    // Interlaced modes list one field; the frame rate is half.
    if d[17] & 0x80 != 0 {
        refresh /= 2.0;
    }
    let refresh = (refresh * 100.0).round() / 100.0;
    Some((h_active, v_active, refresh))
}

/// The text of a display descriptor (up to 13 bytes, ends at a line feed).
fn descriptor_text(d: &[u8]) -> Option<String> {
    let raw = &d[5..18];
    let end = raw.iter().position(|&c| c == 0x0A).unwrap_or(raw.len());
    let text: String = raw[..end]
        .iter()
        .filter(|c| c.is_ascii_graphic() || **c == b' ')
        .map(|&c| c as char)
        .collect();
    let text = text.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// A readable maker name for a PNP manufacturer id, when known.
pub fn maker_name(id: &str) -> Option<&'static str> {
    let name = match id.to_ascii_uppercase().as_str() {
        "ACI" => "ASUS",
        "ACR" => "Acer",
        "AOC" => "AOC",
        "APP" => "Apple",
        "AUO" => "AU Optronics",
        "AUS" => "ASUS",
        "BNQ" => "BenQ",
        "BOE" => "BOE",
        "CMN" => "Innolux",
        "CMO" => "Chi Mei",
        "CSO" => "CSOT",
        "DEL" => "Dell",
        "EIZ" => "EIZO",
        "ENC" => "EIZO",
        "FUS" => "Fujitsu",
        "GBT" => "Gigabyte",
        "GSM" => "LG",
        "HKC" => "HKC",
        "HPN" => "HP",
        "HSD" => "HannStar",
        "HWP" => "HP",
        "IVM" => "iiyama",
        "IVO" => "InfoVision",
        "LEN" => "Lenovo",
        "LGD" => "LG Display",
        "LPL" => "LG Display",
        "MEI" => "Panasonic",
        "MSI" => "MSI",
        "NEC" => "NEC",
        "PHL" => "Philips",
        "SAM" => "Samsung",
        "SDC" => "Samsung Display",
        "SEC" => "Samsung",
        "SHP" => "Sharp",
        "SNY" => "Sony",
        "TCL" => "TCL",
        "TOS" => "Toshiba",
        "TSB" => "Toshiba",
        "VSC" => "ViewSonic",
        "XMI" => "Xiaomi",
        "HPQ" => "HP",
        "HIS" => "Hisense",
        "HEC" => "Hisense",
        "VIZ" => "Vizio",
        "MZI" => "Medion",
        "PNR" => "Planar",
        "WAC" => "Wacom",
        "ZOW" => "BenQ Zowie",
        "AOP" => "AOpen",
        "INN" => "Innolux",
        "SPT" => "Sceptre",
        "JVC" => "JVC",
        "HTC" => "HTC",
        "OVR" => "Oculus",
        "MEG" => "Meta",
        "VLV" => "Valve",
        _ => return None,
    };
    Some(name)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Packs a PNP id into the EDID's 2-byte big-endian form.
    fn pack_id(id: &str) -> [u8; 2] {
        let c: Vec<u16> = id.bytes().map(|b| (b - b'A' + 1) as u16).collect();
        ((c[0] << 10) | (c[1] << 5) | c[2]).to_be_bytes()
    }

    /// A synthetic 128-byte EDID with a valid checksum.
    pub(crate) fn sample(id: &str, product: u16, name: &str, serial: Option<&str>) -> Vec<u8> {
        let mut e = vec![0u8; 128];
        e[..8].copy_from_slice(&HEADER);
        e[8..10].copy_from_slice(&pack_id(id));
        e[10..12].copy_from_slice(&product.to_le_bytes());
        e[12..16].copy_from_slice(&0x0001_E240u32.to_le_bytes()); // 123456
        e[16] = 12; // week
        e[17] = 33; // 2023
        e[18] = 1;
        e[19] = 4;
        e[20] = 0x80 | (3 << 4) | 0x05; // digital, 10 bits, DisplayPort
        e[21] = 60;
        e[22] = 34;
        // Detailed timing: 2560x1440 @ ~59.95 Hz (CVT-RB: 241.5 MHz,
        // h blank 160, v blank 41).
        let d = &mut e[54..72];
        d[0..2].copy_from_slice(&24150u16.to_le_bytes());
        d[2] = (2560 & 0xFF) as u8;
        d[3] = 160;
        d[4] = ((2560 >> 8) as u8) << 4;
        d[5] = (1440 & 0xFF) as u8;
        d[6] = 41;
        d[7] = ((1440 >> 8) as u8) << 4;
        // Name descriptor.
        let mut text = |slot: usize, tag: u8, s: &str| {
            let d = &mut e[54 + slot * 18..54 + (slot + 1) * 18];
            d[3] = tag;
            let mut t = [0x20u8; 13];
            let bytes = s.as_bytes();
            t[..bytes.len()].copy_from_slice(bytes);
            if bytes.len() < 13 {
                t[bytes.len()] = 0x0A;
            }
            d[5..18].copy_from_slice(&t);
        };
        text(1, 0xFC, name);
        if let Some(s) = serial {
            text(2, 0xFF, s);
        } else {
            text(2, 0x10, "");
        }
        text(3, 0xFD, "");
        let sum = e[..127].iter().fold(0u8, |a, &x| a.wrapping_add(x));
        e[127] = 0u8.wrapping_sub(sum);
        e
    }

    #[test]
    fn parses_synthetic_edid() {
        let e = sample("DEL", 0xA0F1, "TEST U27", Some("XYZ0001"));
        let i = parse(&e).expect("valid");
        assert_eq!(i.manufacturer_id, "DEL");
        assert_eq!(i.manufacturer, "Dell");
        assert_eq!(i.product_code, 0xA0F1);
        assert_eq!(i.name.as_deref(), Some("TEST U27"));
        assert_eq!(i.serial.as_deref(), Some("XYZ0001"));
        assert_eq!(i.week, Some(12));
        assert_eq!(i.year, Some(2023));
        assert_eq!(i.version, (1, 4));
        assert!(i.digital);
        assert_eq!(i.bit_depth, Some(10));
        assert_eq!((i.width_cm, i.height_cm), (60, 34));
        let (w, h, hz) = i.native.unwrap();
        assert_eq!((w, h), (2560, 1440));
        assert!((hz - 59.95).abs() < 0.05, "{hz}");
        let diag = i.diagonal_inches().unwrap();
        assert!((diag - 27.2).abs() < 0.3, "{diag}");
    }

    #[test]
    fn numeric_serial_and_unknown_maker() {
        let e = sample("ZZZ", 7, "PLAIN", None);
        let i = parse(&e).unwrap();
        assert_eq!(i.manufacturer, "ZZZ");
        assert_eq!(i.serial.as_deref(), Some("123456"));
        // Longer input (extension blocks) is fine.
        let mut long = e.clone();
        long.extend_from_slice(&[0u8; 128]);
        assert!(parse(&long).is_some());
    }

    #[test]
    fn model_year() {
        let mut e = sample("SAM", 1, "X", None);
        e[16] = 0xFF;
        e[17] = 30;
        let sum = e[..127].iter().fold(0u8, |a, &x| a.wrapping_add(x));
        e[127] = 0u8.wrapping_sub(sum);
        let i = parse(&e).unwrap();
        assert_eq!(i.week, None);
        assert_eq!(i.year, Some(2020));
    }

    #[test]
    fn rejects_bad_input() {
        let good = sample("GSM", 1, "X", None);
        let mut bad_header = good.clone();
        bad_header[0] = 1;
        assert!(parse(&bad_header).is_none());
        let mut bad_sum = good.clone();
        bad_sum[127] = bad_sum[127].wrapping_add(1);
        assert!(parse(&bad_sum).is_none());
        assert!(parse(&good[..127]).is_none());
        assert!(parse(&[]).is_none());
    }

    #[test]
    fn pnp_and_makers() {
        assert_eq!(pnp_id(u16::from_be_bytes(pack_id("AUS"))).unwrap(), "AUS");
        assert_eq!(pnp_id(0), None);
        assert_eq!(maker_name("GSM"), Some("LG"));
        assert_eq!(maker_name("bnq"), Some("BenQ"));
        assert_eq!(maker_name("QQQ"), None);
    }
}
