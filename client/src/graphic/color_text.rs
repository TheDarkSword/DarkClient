//! Parsing of color text pasted into the overlay's color picker.

/// Parses `text` as a whole color, in any form egui's copy button writes or a
/// user is likely to paste:
///
/// * `r, g, b[, a]` — sRGB bytes (the picker's U8 mode);
/// * `0.123, 0.456, 0.789[, 1.000]` — linear floats (its float mode);
/// * `#RRGGBB` / `#RRGGBBAA` — hex.
///
/// Returns unmultiplied sRGB bytes, alpha `255` when absent; `None` for
/// anything else, such as a single number meant for one field.
pub fn parse_color(text: &str) -> Option<[u8; 4]> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        return parse_hex(hex);
    }

    let parts: Vec<&str> = text
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    if !(3..=4).contains(&parts.len()) {
        return None;
    }

    if parts.iter().any(|part| part.contains('.')) {
        let values: Vec<f32> = parts
            .iter()
            .map(|part| part.parse().ok().filter(|v| (0.0..=1.0).contains(v)))
            .collect::<Option<_>>()?;
        let gamma = egui::ecolor::gamma_u8_from_linear_f32;
        Some([
            gamma(values[0]),
            gamma(values[1]),
            gamma(values[2]),
            values.get(3).map_or(255, |a| (a * 255.0).round() as u8),
        ])
    } else {
        let values: Vec<u8> = parts
            .iter()
            .map(|part| part.parse().ok())
            .collect::<Option<_>>()?;
        Some([
            values[0],
            values[1],
            values[2],
            values.get(3).copied().unwrap_or(255),
        ])
    }
}

fn parse_hex(hex: &str) -> Option<[u8; 4]> {
    if !matches!(hex.len(), 6 | 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if hex.len() == 8 { byte(6)? } else { 255 },
    ])
}

#[cfg(test)]
mod tests {
    use super::parse_color;

    #[test]
    fn byte_triples_and_quads_parse() {
        assert_eq!(parse_color("255, 236, 124"), Some([255, 236, 124, 255]));
        assert_eq!(parse_color(" 10,20,30,40 "), Some([10, 20, 30, 40]));
        assert_eq!(parse_color("1 2 3"), Some([1, 2, 3, 255]));
    }

    #[test]
    fn linear_floats_convert_to_srgb_bytes() {
        assert_eq!(parse_color("1.000, 0.000, 1.000"), Some([255, 0, 255, 255]));
        assert_eq!(
            parse_color("0.000, 0.000, 0.000, 0.500"),
            Some([0, 0, 0, 128])
        );
    }

    #[test]
    fn hex_parses_with_optional_alpha() {
        assert_eq!(parse_color("#ff8800"), Some([255, 136, 0, 255]));
        assert_eq!(parse_color("#FF880080"), Some([255, 136, 0, 128]));
    }

    #[test]
    fn single_values_and_garbage_are_left_to_the_field() {
        assert_eq!(parse_color("128"), None);
        assert_eq!(parse_color("256, 0, 0"), None);
        assert_eq!(parse_color("1.5, 0.2, 0.3"), None);
        assert_eq!(parse_color("#ff88"), None);
        assert_eq!(parse_color("hello world foo"), None);
    }
}
