//! Optional advice on a junk scan from a model running on this PC.
//!
//! Quadrant talks to a local model server (Ollama, LM Studio or anything
//! else with the OpenAI-style API) on the loopback address only. Nothing is
//! sent to the internet. Even so, only category names and sizes go into the
//! prompt: file names and paths stay out of it.

use std::time::Duration;

use serde_json::{json, Value};

use crate::model::{AiStatus, CleanScan, Risk};
use crate::util::fmt_bytes;
use crate::{Error, Result};

/// Where Ollama listens by default. LM Studio uses port 1234.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";

const STATUS_TIMEOUT: Duration = Duration::from_secs(4);
/// A local model may need to load into memory first, so this is generous.
const ADVICE_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_CATEGORIES: usize = 40;
const MAX_MODELS: usize = 200;

/// Check an endpoint the UI sent and return it in a standard form
/// (`http://host:port`). Only plain HTTP to this PC is allowed: `127.0.0.1`,
/// `localhost` or `[::1]`, with an optional port and nothing after it. This
/// is what keeps the "nothing leaves your machine" promise, so do not loosen
/// it.
pub fn local_endpoint(input: &str) -> Result<String> {
    let bad = || {
        Error::refused(
            "The model address must be on this PC, for example http://127.0.0.1:11434. Quadrant does not send scans to other computers.",
        )
    };
    let text = input.trim().trim_end_matches('/');
    if text.len() > 64 || text.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(bad());
    }
    let lower = text.to_ascii_lowercase();
    let rest = lower.strip_prefix("http://").ok_or_else(bad)?;
    let (host, port) = if let Some(after) = rest.strip_prefix("[::1]") {
        ("[::1]", after)
    } else if let Some(after) = rest.strip_prefix("127.0.0.1") {
        ("127.0.0.1", after)
    } else if let Some(after) = rest.strip_prefix("localhost") {
        // Always dial the address, so a hosts file entry cannot move it.
        ("127.0.0.1", after)
    } else {
        return Err(bad());
    };
    let port = match port {
        "" => 80u16,
        p => p
            .strip_prefix(':')
            .filter(|d| !d.is_empty() && d.len() <= 5 && d.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|d| d.parse::<u16>().ok())
            .filter(|n| *n != 0)
            .ok_or_else(bad)?,
    };
    Ok(format!("http://{host}:{port}"))
}

fn check_model(model: &str) -> Result<&str> {
    let model = model.trim();
    if model.is_empty() {
        return Err(Error::msg("Choose a model in Settings first."));
    }
    if model.len() > 200 || model.chars().any(|c| c.is_control()) {
        return Err(Error::refused("That model name is not valid."));
    }
    Ok(model)
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        // A local server must not be able to bounce the request elsewhere.
        .max_redirects(0)
        // Never route a loopback call through a system or environment proxy.
        .proxy(None)
        .build()
        .into()
}

/// `data[*].id` from an OpenAI-style model list.
fn extract_models(body: &Value) -> Vec<String> {
    let mut out: Vec<String> = body
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str))
                .map(str::trim)
                .filter(|id| !id.is_empty() && id.len() <= 200 && !id.chars().any(|c| c.is_control()))
                .map(str::to_string)
                .take(MAX_MODELS)
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

/// Whether a model server is answering at `endpoint`, and which models it
/// offers. An unreachable server is a normal state, not an error.
pub fn status(endpoint: &str) -> Result<AiStatus> {
    let endpoint = local_endpoint(endpoint)?;
    let models = agent(STATUS_TIMEOUT)
        .get(&format!("{endpoint}/v1/models"))
        .call()
        .ok()
        .filter(|resp| resp.status().is_success())
        .and_then(|mut resp| resp.body_mut().read_json::<Value>().ok())
        .map(|body| extract_models(&body));
    Ok(AiStatus { endpoint, reachable: models.is_some(), models: models.unwrap_or_default() })
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
Reply in plain UK English, under 220 words, as plain text with short paragraphs and no markdown. Cover: \
how much space can go, what is safe to clear now, what deserves a look first, and one suggested order to do it in.",
        total = fmt_bytes(scan.total_bytes),
        safe = fmt_bytes(scan.safe_bytes),
        list = lines.join("\n"),
    )
}

/// A friendly message for a non-success HTTP status.
fn status_message(code: u16, model: &str) -> String {
    match code {
        404 => format!("The model server does not have a model called {model}. Pick another in Settings."),
        500..=599 => {
            "The local model ran into a problem. It may have run out of memory. Try a smaller model.".into()
        }
        _ => format!("The local model server gave an unexpected reply (HTTP {code})."),
    }
}

/// `choices[0].message.content`. None when empty.
fn extract_text(body: &Value) -> Option<String> {
    let text = body.pointer("/choices/0/message/content")?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Ask the local model for short, plain advice on a junk scan. Blocks until
/// the model answers, for up to five minutes.
pub fn summarise(scan: &CleanScan, endpoint: &str, model: &str) -> Result<String> {
    let endpoint = local_endpoint(endpoint)?;
    let model = check_model(model)?;
    let prompt = build_prompt(scan, &identifying_names());
    let body = json!({
        "model": model,
        "messages": [ { "role": "user", "content": prompt } ],
        "stream": false,
        "temperature": 0.3,
    });

    let mut resp = agent(ADVICE_TIMEOUT)
        .post(&format!("{endpoint}/v1/chat/completions"))
        .send_json(&body)
        .map_err(|_| {
            Error::msg(
                "Could not get an answer from the local model. Check that Ollama or LM Studio is running. A large model can also take too long.",
            )
        })?;

    let code = resp.status().as_u16();
    if !(200..300).contains(&code) {
        return Err(Error::msg(status_message(code, model)));
    }
    let json: Value =
        resp.body_mut().read_json().map_err(|_| Error::msg("The local model returned no advice."))?;
    extract_text(&json).ok_or_else(|| Error::msg("The local model returned no advice."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Area, CleanCategory};

    // No model server is called from a test, apart from one closed port.

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
    fn only_this_pc_is_allowed() {
        assert_eq!(local_endpoint("http://127.0.0.1:11434").unwrap(), "http://127.0.0.1:11434");
        assert_eq!(local_endpoint(" http://LOCALHOST:1234/ ").unwrap(), "http://127.0.0.1:1234");
        assert_eq!(local_endpoint("http://[::1]:8080").unwrap(), "http://[::1]:8080");
        assert_eq!(local_endpoint("http://127.0.0.1").unwrap(), "http://127.0.0.1:80");
        for bad in [
            "",
            "https://127.0.0.1:11434",
            "http://example.com",
            "http://192.168.1.10:11434",
            "http://0.0.0.0:11434",
            "http://127.0.0.1.example.com",
            "http://127.0.0.1@example.com",
            "http://localhost.example.com:80",
            "http://localhost:11434@example.com",
            "http://127.0.0.1:11434/v1",
            "http://127.0.0.1:11434?x=1",
            "http://127.0.0.1:11434#x",
            "http://127.0.0.1:0",
            "http://127.0.0.1:99999",
            "http://127.0.0.1:12a",
            "http://127.0.0.1:",
            "http://127.0.0.1 :80",
            "ftp://127.0.0.1",
            "127.0.0.1:11434",
            "http://[::1%25eth0]:80",
            "http://[::ffff:8.8.8.8]:80",
        ] {
            assert!(local_endpoint(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn model_names_are_checked() {
        assert_eq!(check_model(" qwen2.5:7b ").unwrap(), "qwen2.5:7b");
        assert!(check_model("").is_err());
        assert!(check_model("   ").is_err());
        assert!(check_model("a\u{7}b").is_err());
        assert!(check_model(&"x".repeat(201)).is_err());
    }

    #[test]
    fn unreachable_server_is_a_normal_state() {
        // Port 9 (discard) is not expected to hold a model server.
        let s = status("http://127.0.0.1:9").unwrap();
        assert!(!s.reachable);
        assert!(s.models.is_empty());
        assert!(status("http://example.com").is_err());
    }

    #[test]
    fn maps_statuses() {
        assert!(status_message(404, "tiny").contains("tiny"));
        assert!(status_message(500, "tiny").contains("problem"));
        assert!(status_message(418, "tiny").contains("418"));
    }

    #[test]
    fn extracts_models_and_text() {
        let list = json!({"data":[{"id":"b-model"},{"id":"a-model"},{"id":"a-model"},{"id":""},{"nope":1}]});
        assert_eq!(extract_models(&list), vec!["a-model".to_string(), "b-model".to_string()]);
        assert!(extract_models(&json!({"models":[]})).is_empty());

        let ok = json!({"choices":[{"message":{"role":"assistant","content":" Clear temp first. "}}]});
        assert_eq!(extract_text(&ok).as_deref(), Some("Clear temp first."));
        assert_eq!(extract_text(&json!({"choices":[{"message":{"content":"  "}}]})), None);
        assert_eq!(extract_text(&json!({"error":"x"})), None);
    }
}
