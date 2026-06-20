//! Foveated / saccadic text intake — inspired by active sensing in reading
//! (Nuthmann & Henderson, 2010; Reichle et al., SWIFT/EMMA; SEAM, 2023).
//!
//! Models reading as sequential fixations: a high-resolution **fovea** (few tokens)
//! plus low-resolution **periphery** (compressed context). Packets feed the
//! hippocampus incrementally instead of dumping whole files into context.

use serde::{Deserialize, Serialize};

/// One fixation — a small packet for fast `experience()` encoding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FoveaPacket {
    pub fixation: u32,
    pub foveal: String,
    pub peripheral_before: String,
    pub peripheral_after: String,
    pub salience_hint: f32,
    pub source_uri: String,
    pub byte_offset: u64,
}

#[derive(Debug, Clone)]
pub struct FoveaConfig {
    /// Tokens in high-res fovea (human fovea ≈ 1–2 words; we use a few tokens).
    pub foveal_tokens: usize,
    /// Saccade step — tokens between fixation centers.
    pub saccade_step: usize,
    /// Max peripheral summary chars each side.
    pub peripheral_chars: usize,
}

impl Default for FoveaConfig {
    fn default() -> Self {
        Self {
            foveal_tokens: std::env::var("FLUCTLIGHT_FOVEA_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12),
            saccade_step: std::env::var("FLUCTLIGHT_SACCADE_STEP")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8),
            peripheral_chars: std::env::var("FLUCTLIGHT_PERIPHERY_CHARS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(80),
        }
    }
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace().map(String::from).collect()
}

fn peripheral_summary(tokens: &[String], start: usize, end: usize, max_chars: usize) -> String {
    if start >= end || start >= tokens.len() {
        return String::new();
    }
    let end = end.min(tokens.len());
    let s = tokens[start..end].join(" ");
    if s.len() <= max_chars {
        s
    } else if max_chars > 3 {
        format!("{}…", &s[..max_chars.saturating_sub(1)])
    } else {
        s
    }
}

/// Scan text into foveated packets (saccadic sequence).
pub fn scan_text(text: &str, source_uri: &str, cfg: &FoveaConfig) -> Vec<FoveaPacket> {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut packets = Vec::new();
    let mut center = 0usize;
    let mut fixation = 0u32;
    while center < tokens.len() {
        let foveal_end = (center + cfg.foveal_tokens).min(tokens.len());
        let foveal = tokens[center..foveal_end].join(" ");
        let peri_start = center.saturating_sub(cfg.foveal_tokens);
        let peri_before = peripheral_summary(&tokens, peri_start, center, cfg.peripheral_chars);
        let peri_after_end = (foveal_end + cfg.foveal_tokens).min(tokens.len());
        let peri_after =
            peripheral_summary(&tokens, foveal_end, peri_after_end, cfg.peripheral_chars);
        let salience = 0.45 + 0.1 * (fixation as f32).min(3.0);
        packets.push(FoveaPacket {
            fixation,
            foveal: foveal.clone(),
            peripheral_before: peri_before,
            peripheral_after: peri_after,
            salience_hint: salience.min(0.85),
            source_uri: source_uri.to_string(),
            byte_offset: center as u64,
        });
        fixation += 1;
        if foveal_end >= tokens.len() {
            break;
        }
        center += cfg.saccade_step.max(1);
    }
    packets
}

pub fn scan_file(
    path: &std::path::Path,
    cfg: &FoveaConfig,
) -> crate::error::Result<Vec<FoveaPacket>> {
    let text = std::fs::read_to_string(path).map_err(crate::error::Error::Io)?;
    Ok(scan_text(&text, &format!("file://{}", path.display()), cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saccadic_packets_cover_text() {
        let text = "The quick brown fox jumps over the lazy dog while agents read in fixations";
        let packets = scan_text(text, "test://doc", &FoveaConfig::default());
        assert!(!packets.is_empty());
        assert!(packets[0].foveal.contains("The"));
        assert!(packets.last().unwrap().foveal.len() > 0);
    }
}
