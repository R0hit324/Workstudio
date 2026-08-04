use rand::Rng;

pub fn random_id() -> String {
    let mut rng = rand::rng();
    let n: u64 = rng.random();
    format!("{:016x}", n)
}

/// Primary LAN IPv4 address of this device ("" if unavailable).
pub fn local_ip() -> String {
    local_ip_address::local_ip()
        .ok()
        .map(|ip| ip.to_string())
        .unwrap_or_default()
}

pub fn color_from_hex(hex: &str) -> ratatui::style::Color {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return ratatui::style::Color::Rgb(0x3d, 0x8e, 0xf0);
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0x3d);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0x8e);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0xf0);
    ratatui::style::Color::Rgb(r, g, b)
}

pub fn to_hex(color: ratatui::style::Color) -> String {
    match color {
        ratatui::style::Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => "#3d8ef0".into(),
    }
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
