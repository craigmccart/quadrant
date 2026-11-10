//! Health Check: one pass over space, privacy, speed and security, scored.
//!
//! The scoring is deliberately simple and written down here in one place, so
//! the number on the dial can always be explained. Nothing is weighted to
//! make the PC look worse than it is.

use std::time::Instant;

use crate::model::{
    Area, CleanOptions, CleanScan, Fact, HealthReport, Pillar, Progress, Risk, SecurityCheck,
    StartupItem, Status, SystemOverview,
};
use crate::util::{fmt_bytes, fmt_duration, now_unix};
use crate::{cleanup, security, startup, system, Ctx, Error, Result};

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// How much each pillar counts towards the overall score. Adds up to 1.
const WEIGHT_SPACE: f64 = 0.25;
const WEIGHT_PRIVACY: f64 = 0.20;
const WEIGHT_SPEED: f64 = 0.25;
const WEIGHT_SECURITY: f64 = 0.30;

/// Startup apps beyond this many start to cost points.
const STARTUP_COMFORTABLE: usize = 6;

fn status_for(score: u8) -> Status {
    match score {
        80..=100 => Status::Good,
        50..=79 => Status::Attention,
        _ => Status::Problem,
    }
}

fn to_score(penalty: f64) -> u8 {
    (100.0 - penalty).clamp(0.0, 100.0).round() as u8
}

fn fact(label: &str, value: impl Into<String>) -> Fact {
    Fact { label: label.to_string(), value: value.into() }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Space: junk that is safe to clear, and how full the system drive is.
///
/// - 4 points off per GB of safe junk, at most 40.
/// - System drive under 25% free: 10 off. Under 15%: 25 off. Under 10%: 40 off.
fn space_pillar(junk: &CleanScan, sys: &SystemOverview) -> Pillar {
    let mut penalty = ((junk.safe_bytes as f64 / GB) * 4.0).min(40.0);

    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let drive = sys
        .drives
        .iter()
        .find(|d| d.mount.to_ascii_uppercase().starts_with(&system_drive.to_ascii_uppercase()));

    let mut facts = vec![
        fact("Safe to clear now", fmt_bytes(junk.safe_bytes)),
        fact("Worth a look first", fmt_bytes(junk.total_bytes.saturating_sub(junk.safe_bytes))),
    ];
    if let Some(d) = drive {
        if d.total_bytes > 0 {
            let free_pct = d.free_bytes as f64 / d.total_bytes as f64 * 100.0;
            penalty += match free_pct {
                p if p < 10.0 => 40.0,
                p if p < 15.0 => 25.0,
                p if p < 25.0 => 10.0,
                _ => 0.0,
            };
            facts.push(fact(
                &format!("Free on {}", d.mount.trim_end_matches('\\')),
                format!("{} ({free_pct:.0}%)", fmt_bytes(d.free_bytes)),
            ));
        }
    }

    let score = to_score(penalty);
    Pillar {
        id: "space".into(),
        label: "Space".into(),
        score,
        status: status_for(score),
        headline: fmt_bytes(junk.safe_bytes),
        caption: "of junk that is safe to clear".into(),
        facts,
        page: "junk".into(),
        action_label: "Review junk".into(),
    }
}

/// Privacy: traces of what has been opened or typed. Low stakes, so the
/// most this can cost is 30 points (3 per 100 traces).
fn privacy_pillar(privacy: &CleanScan) -> Pillar {
    let traces = privacy.total_items;
    let penalty = (traces as f64 / 100.0 * 3.0).min(30.0);
    let score = to_score(penalty);

    let mut facts: Vec<Fact> = privacy
        .categories
        .iter()
        .filter(|c| c.item_count > 0)
        .take(3)
        .map(|c| fact(&c.label, c.item_count.to_string()))
        .collect();
    if facts.is_empty() {
        facts.push(fact("Recent activity", "Nothing stored"));
    }

    Pillar {
        id: "privacy".into(),
        label: "Privacy".into(),
        score,
        status: status_for(score),
        headline: traces.to_string(),
        caption: if traces == 1 {
            "trace of recent activity".into()
        } else {
            "traces of recent activity".into()
        },
        facts,
        page: "privacy".into(),
        action_label: "Review traces".into(),
    }
}

/// Speed: how much starts with Windows, memory pressure, and time since the
/// last restart.
///
/// - 4 points off per startup app beyond six, at most 40.
/// - Memory over 85% in use: 15 off. Over 92%: 25 off.
/// - Up for more than 7 days: 10 off. More than 14 days: 15 off.
fn speed_pillar(startup: &[StartupItem], sys: &SystemOverview) -> Pillar {
    let enabled = startup.iter().filter(|s| s.enabled).count();
    let mut penalty = (enabled.saturating_sub(STARTUP_COMFORTABLE) as f64 * 4.0).min(40.0);

    let mut facts = vec![fact("Switched off", (startup.len() - enabled).to_string())];

    if sys.ram_total_bytes > 0 {
        let used_pct = sys.ram_used_bytes as f64 / sys.ram_total_bytes as f64 * 100.0;
        penalty += match used_pct {
            p if p > 92.0 => 25.0,
            p if p > 85.0 => 15.0,
            _ => 0.0,
        };
        facts.push(fact("Memory in use", format!("{used_pct:.0}%")));
    }
    if sys.uptime_secs > 0 {
        let days = sys.uptime_secs / 86_400;
        penalty += match days {
            d if d > 14 => 15.0,
            d if d > 7 => 10.0,
            _ => 0.0,
        };
        facts.push(fact("Since last restart", fmt_duration(sys.uptime_secs)));
    }

    let score = to_score(penalty);
    Pillar {
        id: "speed".into(),
        label: "Speed".into(),
        score,
        status: status_for(score),
        headline: enabled.to_string(),
        caption: if enabled == 1 {
            "app starts with Windows".into()
        } else {
            "apps start with Windows".into()
        },
        facts,
        page: "startup".into(),
        action_label: "Review startup".into(),
    }
}

/// Security: 25 points off per failed check, 10 per check needing attention.
/// Checks we could not read cost nothing.
fn security_pillar(checks: &[SecurityCheck]) -> Pillar {
    let known: Vec<&SecurityCheck> = checks.iter().filter(|c| c.status != Status::Unknown).collect();
    let passed = known.iter().filter(|c| c.status == Status::Good).count();
    let problems = known.iter().filter(|c| c.status == Status::Problem).count();
    let attention = known.iter().filter(|c| c.status == Status::Attention).count();
    let score = to_score(problems as f64 * 25.0 + attention as f64 * 10.0);

    let mut facts: Vec<Fact> = known
        .iter()
        .filter(|c| c.status != Status::Good)
        .take(3)
        .map(|c| fact(&c.label, c.summary.clone()))
        .collect();
    if facts.is_empty() && !known.is_empty() {
        facts.push(fact("Protection", "Everything checked is in order"));
    }
    let unknown = checks.len() - known.len();
    if unknown > 0 {
        facts.push(fact("Could not check", plural(unknown, "item", "items")));
    }

    // If nothing could be read we must not claim a clean bill of health.
    let status = if known.is_empty() { Status::Unknown } else { status_for(score) };

    Pillar {
        id: "security".into(),
        label: "Security".into(),
        score,
        status,
        headline: format!("{passed} of {}", known.len()),
        caption: "checks passed".into(),
        facts,
        page: "security".into(),
        action_label: "Review security".into(),
    }
}

/// Turn the raw findings into the four pillars and one overall score.
/// Pure, so it can be tested without touching the machine.
pub fn score(
    junk: &CleanScan,
    privacy: &CleanScan,
    startup: &[StartupItem],
    security: &[SecurityCheck],
    sys: &SystemOverview,
) -> (u8, Status, String, Vec<Pillar>) {
    let space = space_pillar(junk, sys);
    let priv_ = privacy_pillar(privacy);
    let speed = speed_pillar(startup, sys);
    let sec = security_pillar(security);

    let overall = (space.score as f64 * WEIGHT_SPACE
        + priv_.score as f64 * WEIGHT_PRIVACY
        + speed.score as f64 * WEIGHT_SPEED
        + sec.score as f64 * WEIGHT_SECURITY)
        .round() as u8;

    let pillars = vec![space, priv_, speed, sec];
    let needing = pillars
        .iter()
        .filter(|p| matches!(p.status, Status::Attention | Status::Problem))
        .count();

    // One real problem should never be hidden behind a high average.
    let any_problem = pillars.iter().any(|p| p.status == Status::Problem);
    let status = match status_for(overall) {
        Status::Good if any_problem => Status::Attention,
        other => other,
    };

    let summary = match (status, needing) {
        (Status::Good, 0) => "Your PC is in good shape.".to_string(),
        (Status::Good, _) => "Your PC is in good shape, with a little tidying to do.".to_string(),
        (_, 1) => "One area could do with attention.".to_string(),
        (_, n) => format!("{} areas could do with attention.", number_word(n)),
    };

    (overall, status, summary, pillars)
}

fn number_word(n: usize) -> &'static str {
    match n {
        0 => "No",
        2 => "Two",
        3 => "Three",
        4 => "Four",
        _ => "Several",
    }
}

/// Run the whole Health Check. Read-only: nothing is removed or changed.
pub fn run(opts: CleanOptions, ctx: &Ctx) -> Result<HealthReport> {
    let started = Instant::now();

    // Each stage owns a slice of the progress bar. Sub-task messages are
    // passed through with that stage's fraction attached.
    let staged = |fraction: f32| {
        move |p: Progress| ctx.report(Progress { fraction: Some(fraction), ..p })
    };
    let check_cancel = || if ctx.cancelled() { Err(Error::Cancelled) } else { Ok(()) };

    ctx.step("Measuring junk files", 0.02);
    let junk_progress = staged(0.05);
    let junk = cleanup::scan(Area::Junk, opts, &ctx.with_progress(&junk_progress))?;
    check_cancel()?;

    ctx.step("Looking for traces of recent activity", 0.60);
    let privacy_progress = staged(0.62);
    let privacy = cleanup::scan(Area::Privacy, opts, &ctx.with_progress(&privacy_progress))?;
    check_cancel()?;

    ctx.step("Checking what starts with Windows", 0.72);
    let startup_items = startup::list();
    check_cancel()?;

    ctx.step("Checking security settings", 0.80);
    let checks = security::checks();
    check_cancel()?;

    ctx.step("Reading system details", 0.95);
    let sys = system::overview();

    let (overall, status, summary, pillars) =
        score(&junk, &privacy, &startup_items, &checks, &sys);

    let quick: Vec<_> = junk
        .categories
        .iter()
        .filter(|c| c.risk == Risk::Safe && c.default_selected && c.bytes > 0)
        .collect();
    let quick_clean_bytes = quick.iter().map(|c| c.bytes).sum();
    let quick_clean_ids = quick.iter().map(|c| c.id.clone()).collect();

    ctx.step("Done", 1.0);
    Ok(HealthReport {
        score: overall,
        status,
        summary,
        pillars,
        scanned_at: now_unix(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        junk,
        privacy,
        startup: startup_items,
        security: checks,
        quick_clean_ids,
        quick_clean_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Drive, Scope, StartupSource};

    fn scan(area: Area, safe_bytes: u64, review_bytes: u64, items: u64) -> CleanScan {
        CleanScan {
            area,
            categories: Vec::new(),
            total_bytes: safe_bytes + review_bytes,
            safe_bytes,
            total_items: items,
            scanned_at: 0,
            elapsed_ms: 0,
        }
    }

    fn system(free_pct: u64, ram_used_pct: u64, uptime_days: u64) -> SystemOverview {
        SystemOverview {
            host_name: String::new(),
            user_name: String::new(),
            os_name: String::new(),
            os_version: String::new(),
            os_build: String::new(),
            manufacturer: String::new(),
            model: String::new(),
            cpu_name: String::new(),
            cpu_cores_physical: 8,
            cpu_cores_logical: 16,
            gpus: Vec::new(),
            ram_total_bytes: 100,
            ram_used_bytes: ram_used_pct,
            uptime_secs: uptime_days * 86_400,
            boot_time: 0,
            is_elevated: false,
            drives: vec![Drive {
                mount: format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())),
                label: String::new(),
                file_system: "NTFS".into(),
                kind: "SSD".into(),
                total_bytes: 100,
                free_bytes: free_pct,
            }],
        }
    }

    fn startup_items(enabled: usize) -> Vec<StartupItem> {
        (0..enabled)
            .map(|i| StartupItem {
                id: i.to_string(),
                name: format!("App {i}"),
                command: String::new(),
                target: String::new(),
                source: StartupSource::RegistryRun,
                scope: Scope::User,
                location: String::new(),
                enabled: true,
                can_toggle: true,
                needs_admin: false,
            })
            .collect()
    }

    fn check(status: Status) -> SecurityCheck {
        SecurityCheck {
            id: "x".into(),
            label: "X".into(),
            status,
            summary: "s".into(),
            detail: String::new(),
            fix_page: None,
        }
    }

    #[test]
    fn a_clean_machine_scores_full_marks() {
        let (overall, status, summary, pillars) = score(
            &scan(Area::Junk, 0, 0, 0),
            &scan(Area::Privacy, 0, 0, 0),
            &startup_items(3),
            &[check(Status::Good), check(Status::Good)],
            &system(60, 40, 1),
        );
        assert_eq!(overall, 100);
        assert_eq!(status, Status::Good);
        assert_eq!(summary, "Your PC is in good shape.");
        assert!(pillars.iter().all(|p| p.score == 100));
    }

    #[test]
    fn junk_costs_four_points_a_gigabyte_capped_at_forty() {
        let five_gb = scan(Area::Junk, 5 * GB as u64, 0, 10);
        let huge = scan(Area::Junk, 500 * GB as u64, 0, 10);
        let sys = system(60, 40, 1);
        assert_eq!(space_pillar(&five_gb, &sys).score, 80);
        assert_eq!(space_pillar(&huge, &sys).score, 60);
    }

    #[test]
    fn a_nearly_full_drive_is_a_problem() {
        let p = space_pillar(&scan(Area::Junk, 10 * GB as u64, 0, 1), &system(5, 40, 1));
        assert_eq!(p.score, 20);
        assert_eq!(p.status, Status::Problem);
    }

    #[test]
    fn privacy_can_never_cost_more_than_thirty() {
        let p = privacy_pillar(&scan(Area::Privacy, 0, 0, 1_000_000));
        assert_eq!(p.score, 70);
        assert_eq!(p.status, Status::Attention);
    }

    #[test]
    fn startup_apps_beyond_six_cost_points() {
        let sys = system(60, 40, 1);
        assert_eq!(speed_pillar(&startup_items(6), &sys).score, 100);
        assert_eq!(speed_pillar(&startup_items(9), &sys).score, 88);
        assert_eq!(speed_pillar(&startup_items(100), &sys).score, 60);
    }

    #[test]
    fn disabled_startup_apps_do_not_count() {
        let mut items = startup_items(10);
        for item in items.iter_mut().take(6) {
            item.enabled = false;
        }
        assert_eq!(speed_pillar(&items, &system(60, 40, 1)).score, 100);
    }

    #[test]
    fn a_failed_security_check_shows_even_with_a_high_average() {
        let (_, status, _, pillars) = score(
            &scan(Area::Junk, 0, 0, 0),
            &scan(Area::Privacy, 0, 0, 0),
            &startup_items(0),
            &[check(Status::Problem), check(Status::Problem), check(Status::Problem)],
            &system(60, 40, 1),
        );
        let sec = pillars.iter().find(|p| p.id == "security").unwrap();
        assert_eq!(sec.score, 25);
        assert_eq!(sec.status, Status::Problem);
        assert_eq!(status, Status::Attention);
    }

    #[test]
    fn unreadable_security_checks_are_unknown_not_good() {
        let p = security_pillar(&[check(Status::Unknown), check(Status::Unknown)]);
        assert_eq!(p.status, Status::Unknown);
        assert_eq!(p.headline, "0 of 0");
    }
}
