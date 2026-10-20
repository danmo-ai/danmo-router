//! Stateless registration captcha: answer + expiry sealed with `ENCRYPTION_KEY`.
//!
//! No database rows. Replay resistance is the caller's job (process-local set is enough
//! for typical single-instance `danmo-user`).

use crate::crypto::{decrypt_secret, encrypt_secret};
use crate::{AppError, AppResult};
use rand::Rng;
use std::fmt::Write as _;

const CODE_LEN: usize = 5;
const TTL_SECS: i64 = 300;
/// Ambiguous glyphs omitted (0/O, 1/I/L).
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone)]
pub struct CaptchaChallenge {
    /// Opaque sealed token; send back with the user's answer.
    pub token: String,
    /// Inline SVG image of the challenge text.
    pub image_svg: String,
}

/// Issue a fresh captcha. Does not touch storage.
pub fn issue(encryption_key: &[u8; 32]) -> AppResult<CaptchaChallenge> {
    let mut rng = rand::thread_rng();
    let mut code = String::with_capacity(CODE_LEN);
    for _ in 0..CODE_LEN {
        code.push(ALPHABET[rng.gen_range(0..ALPHABET.len())] as char);
    }

    let mut nonce = [0u8; 16];
    rng.fill(&mut nonce);
    let exp = chrono::Utc::now().timestamp() + TTL_SECS;
    let plaintext = format!("captcha-v1|{}|{exp}|{code}", hex::encode(nonce));
    let token = encrypt_secret(encryption_key, &plaintext).map_err(AppError::Other)?;
    let image_svg = render_svg(&code, &mut rng);

    Ok(CaptchaChallenge { token, image_svg })
}

/// Verify answer against a sealed token. Returns the challenge nonce (hex) on success
/// so the caller can mark it consumed for replay protection.
pub fn verify(encryption_key: &[u8; 32], token: &str, answer: &str) -> AppResult<String> {
    let token = token.trim();
    let answer = answer.trim();
    if token.is_empty() || answer.is_empty() {
        return Err(AppError::BadRequest("captcha required".into()));
    }

    let plaintext = decrypt_secret(encryption_key, token)
        .map_err(|_| AppError::BadRequest("captcha expired or invalid".into()))?;

    let parts: Vec<&str> = plaintext.splitn(4, '|').collect();
    if parts.len() != 4 || parts[0] != "captcha-v1" {
        return Err(AppError::BadRequest("captcha expired or invalid".into()));
    }
    let nonce = parts[1];
    let exp: i64 = parts[2]
        .parse()
        .map_err(|_| AppError::BadRequest("captcha expired or invalid".into()))?;
    let code = parts[3];

    if chrono::Utc::now().timestamp() > exp {
        return Err(AppError::BadRequest("captcha expired or invalid".into()));
    }

    if !constant_time_eq_ascii_ignore_case(answer.as_bytes(), code.as_bytes()) {
        return Err(AppError::BadRequest("captcha incorrect".into()));
    }

    Ok(nonce.to_string())
}

fn render_svg(code: &str, rng: &mut impl Rng) -> String {
    let width = 160;
    let height = 48;
    let mut svg = String::with_capacity(1024);
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="captcha">"#
    );
    svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#f1f5f9\"/>");

    for _ in 0..6 {
        let x1: i32 = rng.gen_range(0..width);
        let y1: i32 = rng.gen_range(0..height);
        let x2: i32 = rng.gen_range(0..width);
        let y2: i32 = rng.gen_range(0..height);
        let stroke = noise_color(rng);
        let _ = write!(
            svg,
            r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{stroke}" stroke-width="1"/>"#
        );
    }

    let n = code.chars().count().max(1) as f64;
    for (i, ch) in code.chars().enumerate() {
        let x = 18.0 + (i as f64) * ((width as f64 - 28.0) / n);
        let y = rng.gen_range(28.0..38.0);
        let rot = rng.gen_range(-28.0..28.0);
        let fill = text_color(rng);
        let _ = write!(
            svg,
            r#"<text x="{x:.1}" y="{y:.1}" fill="{fill}" font-family="ui-monospace,SFMono-Regular,Menlo,Consolas,monospace" font-size="22" font-weight="700" transform="rotate({rot:.1} {x:.1} {y:.1})">{ch}</text>"#
        );
    }

    svg.push_str("</svg>");
    svg
}

fn noise_color(rng: &mut impl Rng) -> &'static str {
    const COLORS: &[&str] = &["#94a3b8", "#cbd5e1", "#64748b", "#a5b4fc"];
    COLORS[rng.gen_range(0..COLORS.len())]
}

fn text_color(rng: &mut impl Rng) -> &'static str {
    const COLORS: &[&str] = &["#0f172a", "#1e3a8a", "#0f766e", "#9f1239"];
    COLORS[rng.gen_range(0..COLORS.len())]
}

fn constant_time_eq_ascii_ignore_case(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x.to_ascii_uppercase() ^ y.to_ascii_uppercase();
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_verify_roundtrip() {
        let key = [11u8; 32];
        let ch = issue(&key).unwrap();
        assert!(!ch.token.is_empty());
        assert!(ch.image_svg.contains("<svg"));
        assert!(ch.image_svg.contains("</svg>"));

        let plain = decrypt_secret(&key, &ch.token).unwrap();
        let code = plain.split('|').nth(3).unwrap();
        let nonce = verify(&key, &ch.token, code).unwrap();
        assert_eq!(nonce.len(), 32);
        assert!(verify(&key, &ch.token, code.to_ascii_lowercase().as_str()).is_ok());
        assert!(verify(&key, &ch.token, "WRONG").is_err());
        assert!(verify(&key, &ch.token, "").is_err());
    }

    #[test]
    fn rejects_tampered_token() {
        let key = [11u8; 32];
        let ch = issue(&key).unwrap();
        let mut bad = ch.token.clone();
        bad.push('x');
        assert!(verify(&key, &bad, "AAAAA").is_err());
        assert!(verify(&[12u8; 32], &ch.token, "AAAAA").is_err());
    }
}
