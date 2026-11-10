//! Optional Gemini advice on a junk scan.
//!
//! Only category names and sizes are sent. File names and paths never leave
//! the machine. The API key travels in a header, never in the URL, and never
//! appears in an error.

use std::time::Duration;

use serde_json::{json, Value};

use crate::model::{AiStatus, CleanScan, Risk};
use crate::util::fmt_bytes;
use crate::{Error, Result};

/// The Gemini model used for advice. Change it here and nowhere else.
pub const GEMINI_MODEL: &str = "gemini-3.1-flash-lite-preview";

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CATEGORIES: usize = 40;

/// Whether a key is saved, and a hint for showing it.
pub fn status() -> AiStatus {
    let key = crate::secrets::get_api_key().ok().flatten();
    AiStatus {
        has_key: key.is_some(),
        key_hint: key.as_deref().map(key_hint).unwrap_or_default(),
        model: GEMINI_MODEL.to_string(),
        legacy_key_file: crate::secrets::legacy_key_file().is_some(),
    }
}

/// The last four characters of the key, for display only.
fn key_hint(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    chars[chars.len().saturating_sub(4)..].iter().collect()
}

/// Remove anything that could identify the person or a file: any word that
/// looks like a path, and the user and machine names. Category text is fixed
/// in the app, so this is a second line of defence, not the first.
fn scrub(text: &str, names: &[String]) -> String {
    let mut out: String = text
        .split_whitespace()
        .map(|w| if w.contains('\\') || w.contains('/') || w.contains(":\\") { "[path]" } else { w })
        .collect::<Vec<_>>()
        .join(" ");
    for n in names.iter().filter(|n| n.chars().count() >= 3) {
        out = replace_ignore_case(&out, n, "[name]");
    }
    out
}

fn replace_ignore_case(haystack: &str, needle: &str, with: &str) -> String {
    let lower_h = haystack.to_lowercase();
    let lower_n = needle.to_lowercase();
    // Lowercasing can change byte lengths for some scripts; fall back to an
    // exact replace in that case rather than slicing at a bad boundary.
    if lower_h.len() != haystack.len() || lower_n.len() != needle.len() {
        return haystack.replace(needle, with);
    }
    let mut out = String::with_capacity(haystack.len());
    let mut rest = 0;
    while let Some(pos) = lower_h[rest..].find(&lower_n) {
        out.push_str(&haystack[rest..rest + pos]);
        out.push_str(with);
        rest += pos + needle.len();
    }
    out.push_str(&haystack[rest..]);
    out
}

fn identifying_names() -> Vec<String> {
    ["USERNAME", "COMPUTERNAME", "USERDOMAIN"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// The prompt: category-level data only (label, size, count, rating, note).
/// No ids, descriptions, groups, file names or paths.
fn build_prompt(scan: &CleanScan, names: &[String]) -> String {
    let mut lines = Vec::new();
    for c in scan.categories.iter().filter(|c| c.bytes > 0).take(MAX_CATEGORIES) {
        let rating = match c.risk {
            Risk::Safe => "safe",
            Risk::Review => "review",
        };
        lines.push(format!(
            "- {}: {}, {} items, rated {}. Note: {}",
            scrub(&c.label, names),
            fmt_bytes(c.bytes),
            c.item_count,
            rating,
            scrub(&c.note, names),
        ));
    }
    if lines.is_empty() {
        lines.push("- No junk was found.".to_string());
    }
    format!(
        "You are a Windows clean-up assistant. A scan by the Quadrant app found these junk categories \
(total {total}, of which {safe} is rated safe):\n{list}\n\n\
Reply in plain UK English, under 220 words, as plain text with short paragraphs and no markdown tables. Cover: \
how much space can go, what is safe to clear now, what deserves a look first, and one suggested order to do it in.",
        total = fmt_bytes(scan.total_bytes),
        safe = fmt_bytes(scan.safe_bytes),
        list = lines.join("\n"),
    )
}

/// A friendly message for a non-success HTTP status.
fn status_message(code: u16) -> String {
    match code {
        400 | 401 | 403 => "Gemini did not accept the API key. Check it in Settings.".into(),
        404 => format!(
            "Gemini does not recognise the model name {GEMINI_MODEL}. It may have been retired."
        ),
        429 => "Gemini is rate limiting requests. Try again in a minute.".into(),
        500..=599 => "Gemini is having trouble right now. Try again later.".into(),
        _ => format!("Gemini gave an unexpected reply (HTTP {code})."),
    }
}

/// `candidates[0].content.parts[*].text`, joined. None when empty or blocked.
fn extract_text(body: &Value) -> Option<String> {
    let parts = body.pointer("/candidates/0/content/parts")?.as_array()?;
    let text: String = parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Ask Gemini for short, plain advice on a junk scan. Blocks for up to 30
/// seconds. Uses the key from `crate::secrets`.
pub fn summarise(scan: &CleanScan) -> Result<String> {
    let key = crate::secrets::get_api_key()?
        .ok_or_else(|| Error::msg("Add a Gemini API key in Settings first."))?;
    let prompt = build_prompt(scan, &identifying_names());
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{GEMINI_MODEL}:generateContent"
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let body = json!({ "contents": [ { "parts": [ { "text": prompt } ] } ] });

    // Transport errors are replaced wholesale: their text is never shown.
    let mut resp = agent
        .post(&url)
        .header("x-goog-api-key", key.as_str())
        .send_json(&body)
        .map_err(|_| Error::msg("Could not reach Gemini. Check the internet connection."))?;
    drop(key);

    let code = resp.status().as_u16();
    if !(200..300).contains(&code) {
        return Err(Error::msg(status_message(code)));
    }
    let json: Value = resp
        .body_mut()
        .read_json()
        .map_err(|_| Error::msg("Gemini returned no advice."))?;
    extract_text(&json).ok_or_else(|| Error::msg("Gemini returned no advice."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Area, CleanCategory};

    // The real Gemini API is never called from a test.

    fn sample_scan() -> CleanScan {
        let cat = |id: &str, label: &str, bytes: u64, risk: Risk, note: &str| CleanCategory {
            id: id.into(),
            area: Area::Junk,
            group: r"C:\Users\SecretPerson\AppData".into(),
            label: label.into(),
            description: r"Lives in C:\Users\SecretPerson\AppData\Local\Temp\secret-file.tmp".into(),
            note: note.into(),
            risk,
            bytes,
            item_count: 12,
            needs_admin: false,
            default_selected: risk == Risk::Safe,
            to_recycle_bin: false,
            can_list: true,
        };
        CleanScan {
            area: Area::Junk,
            categories: vec![
                cat("user-temp", "Temporary files", 3 * 1024 * 1024 * 1024, Risk::Safe, "Apps recreate these."),
                cat(
                    r"C:\Users\SecretPerson\Downloads",
                    "Downloads",
                    500 * 1024 * 1024,
                    Risk::Review,
                    r"Check C:\Users\SecretPerson\Downloads\payslip.pdf before removing SecretPerson stuff on DESKTOP-HOST9.",
                ),
                cat("empty", "Nothing here", 0, Risk::Safe, "Empty."),
            ],
            total_bytes: 3 * 1024 * 1024 * 1024 + 500 * 1024 * 1024,
            safe_bytes: 3 * 1024 * 1024 * 1024,
            total_items: 24,
            scanned_at: 0,
            elapsed_ms: 0,
        }
    }

    #[test]
    fn prompt_carries_categories_only() {
        let names = vec!["SecretPerson".to_string(), "DESKTOP-HOST9".to_string()];
        let prompt = build_prompt(&sample_scan(), &names);
        assert!(prompt.contains("Temporary files: 3.0 GB, 12 items, rated safe"));
        assert!(prompt.contains("Downloads: 500.0 MB, 12 items, rated review"));
        assert!(prompt.contains("under 220 words"));
        assert!(!prompt.contains("Nothing here"), "empty categories are left out");
        for leak in ["SecretPerson", "secretperson", "DESKTOP-HOST9", "payslip", "secret-file", r"C:\", "AppData", "user-temp"] {
            assert!(!prompt.contains(leak), "prompt leaked {leak}: {prompt}");
        }
    }

    #[test]
    fn scrubs_paths_and_names() {
        let names = vec!["Craigish".to_string(), "ab".to_string()];
        assert_eq!(scrub(r"see C:\x\y and /etc/z for craigish", &names), "see [path] and [path] for [name]");
        // Names shorter than three characters are not scrubbed (too many
        // false hits on ordinary words).
        assert_eq!(scrub("about", &names), "about");
    }

    #[test]
    fn hints_last_four() {
        assert_eq!(key_hint("abcdefgh"), "efgh");
        assert_eq!(key_hint("abc"), "abc");
        assert_eq!(key_hint(""), "");
    }

    #[test]
    fn maps_statuses() {
        for c in [400, 401, 403] {
            assert_eq!(status_message(c), "Gemini did not accept the API key. Check it in Settings.");
        }
        assert!(status_message(404).contains(GEMINI_MODEL));
        assert_eq!(status_message(429), "Gemini is rate limiting requests. Try again in a minute.");
        assert!(status_message(503).contains("trouble"));
        assert!(status_message(418).contains("418"));
    }

    #[test]
    fn extracts_text() {
        let ok = json!({"candidates":[{"content":{"parts":[{"text":"Clear temp. "},{"text":"Then downloads."}]}}]});
        assert_eq!(extract_text(&ok).as_deref(), Some("Clear temp. Then downloads."));
        let blocked = json!({"promptFeedback":{"blockReason":"SAFETY"}});
        assert_eq!(extract_text(&blocked), None);
        let empty = json!({"candidates":[{"content":{"parts":[{"text":"  "}]}}]});
        assert_eq!(extract_text(&empty), None);
        let no_parts = json!({"candidates":[{"finishReason":"SAFETY"}]});
        assert_eq!(extract_text(&no_parts), None);
    }
}
