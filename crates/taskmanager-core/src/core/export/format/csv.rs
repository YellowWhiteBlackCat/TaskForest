//! CSV and self-contained HTML snapshot projections.
//!
//! Split from the parent JSON formatter by output format: the JSON side owns
//! the structured payload, this module owns the flat tabular renderings and
//! their escaping rules.

use crate::core::metrics::SystemSnapshot;
use crate::core::process::ProcessItem;

/// Escape one CSV field per RFC 4180: if it contains a comma, double-quote, or
/// any newline (CR/LF), wrap it in double quotes and double every internal
/// double-quote. Otherwise emit it verbatim.
fn csv_escape(field: &str, out: &mut String) {
    let needs_quoting = field
        .as_bytes()
        .iter()
        .any(|&b| b == b',' || b == b'"' || b == b'\n' || b == b'\r');
    if !needs_quoting {
        out.push_str(field);
        return;
    }
    out.push('"');
    for &b in field.as_bytes() {
        if b == b'"' {
            // Double the quote per RFC 4180 §2.7.
            out.push('"');
            out.push('"');
        } else {
            out.push(b as char);
        }
    }
    out.push('"');
}

/// Hand-rolled CSV of the user-visible process columns. One header row then one
/// row per process, in the order given. Columns:
/// `Name,PID,CPU%,Memory MB,User,Status,Threads,Disk R,Disk W`
///
/// `Memory MB` is `memory_bytes / 1024²` to two decimals. `Disk R` / `Disk W`
/// are the per-tick throughput totals in bytes (`disk_read_bytes` /
/// `disk_write_bytes`). Name / User / Status are RFC 4180-escaped.
pub fn processes_to_csv(procs: &[ProcessItem]) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mut out = String::with_capacity(procs.len() * 64 + 128);
    out.push_str("Name,PID,CPU%,Memory MB,User,Status,Threads,Disk R,Disk W\n");
    for p in procs {
        csv_escape(&p.name, &mut out);
        out.push(',');
        out.push_str(&p.pid.to_string());
        out.push(',');
        csv_f32(p.current_cpu_percentage(), &mut out);
        out.push(',');
        csv_mib(p.current_memory_bytes(), MB, &mut out);
        out.push(',');
        let user = p.current_user();
        csv_escape(user.as_deref().unwrap_or("—"), &mut out);
        out.push(',');
        csv_escape(&p.status, &mut out);
        out.push(',');
        csv_optional(p.current_threads(), &mut out);
        out.push(',');
        csv_optional(p.current_disk_read_bytes_per_sec(), &mut out);
        out.push(',');
        csv_optional(p.current_disk_write_bytes_per_sec(), &mut out);
        out.push('\n');
    }
    out
}

fn csv_optional<T: std::fmt::Display>(value: Option<T>, out: &mut String) {
    match value {
        Some(value) => out.push_str(&value.to_string()),
        None => out.push('—'),
    }
}

fn csv_f32(value: Option<f32>, out: &mut String) {
    match value {
        Some(value) if value.is_finite() => out.push_str(&format!("{value:.2}")),
        _ => out.push('—'),
    }
}

fn csv_mib(value: Option<u64>, mb: f64, out: &mut String) {
    match value {
        Some(value) => out.push_str(&format!("{:.2}", value as f64 / mb)),
        None => out.push('—'),
    }
}

/// Escape the five HTML-significant characters (`& < > " '`) per the OWASP
/// recommended encoding so a process name / user field can never inject markup.
/// Iterating by `char` (not byte) preserves multi-byte UTF-8 unchanged. Pure
/// (no I/O); unit-tested.
fn html_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

/// Format a byte count as mebibytes with one decimal, e.g. `1536 MiB`. Pure
/// helper for the HTML stats block; never used on the hot path.
fn fmt_mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

/// Self-contained HTML snapshot: a styled page with a key-system-stats table
/// and the user-visible process table. Every text field is HTML-escaped via
/// `html_escape` so a hostile process name (e.g. `<script>…`) renders as inert
/// text, never executable markup. No external resources (the stylesheet is
/// inlined) so the file is fully viewable offline / from disk. `procs` may be
/// empty — the table then has a header row + a "no processes" placeholder row.
///
/// Columns mirror [`processes_to_csv`]: Name, PID, CPU%, Memory, User, Status,
/// Threads, Disk R, Disk W (plus CPU time in seconds when populated).
pub fn processes_to_html(snap: &SystemSnapshot, procs: &[ProcessItem]) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mut out = String::with_capacity(procs.len() * 200 + 2048);
    // ── document head + inlined stylesheet ────────────────────────────────
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>Task Manager snapshot</title>\n<style>\n");
    out.push_str(
        "body{font-family:system-ui,Segoe UI,Roboto,sans-serif;margin:1.5rem;color:#222;}\n",
    );
    out.push_str("h1{font-size:1.2rem;margin:0 0 1rem;}\n");
    out.push_str("h2{font-size:1rem;margin:1.5rem 0 .5rem;}\n");
    out.push_str("table{border-collapse:collapse;width:100%;font-size:.85rem;}\n");
    out.push_str("th,td{border:1px solid #ddd;padding:4px 8px;text-align:left;}\n");
    out.push_str("th{background:#f4f4f4;}\n");
    out.push_str("td.num{text-align:right;font-variant-numeric:tabular-nums;}\n");
    out.push_str("caption{caption-side:top;text-align:left;color:#666;font-size:.8rem;padding-bottom:.25rem;}\n");
    out.push_str("</style>\n</head>\n<body>\n");
    out.push_str("<h1>Task Manager snapshot</h1>\n");

    // ── system stats block ────────────────────────────────────────────────
    out.push_str("<h2>System</h2>\n<table>\n");
    // CPU: brand + global utilization %. Brand is user/sysinfo-controlled text
    // → escaped.
    out.push_str("<tr><th>CPU</th><td>");
    html_escape(snap.cpu.brand.as_deref().unwrap_or("—"), &mut out);
    out.push_str(" &middot; ");
    match snap.cpu.current_global_usage_pct() {
        Some(usage) => out.push_str(&format!("{usage:.1}% global")),
        None => out.push_str("— global"),
    }
    out.push_str("</td></tr>\n");
    // Memory: used / total as MiB.
    out.push_str("<tr><th>Memory</th><td>");
    match (
        snap.memory.current_used_bytes(),
        snap.memory.current_total_bytes(),
        snap.memory.used_percentage_observed(),
    ) {
        (Some(used), Some(total), Some(percentage)) => out.push_str(&format!(
            "{} / {} ({percentage:.0}%)",
            fmt_mib(used),
            fmt_mib(total),
        )),
        (Some(used), _, _) => out.push_str(&format!("{} / — (—)", fmt_mib(used))),
        _ => out.push_str("— / — (—)"),
    }
    out.push_str("</td></tr>\n");
    // A measured zero means no swap is configured and remains hidden. Typed
    // unavailability is rendered explicitly instead of being confused with
    // that legitimate zero.
    match snap.memory.current_swap_total_bytes() {
        Some(0) => {}
        Some(total) => {
            out.push_str("<tr><th>Swap</th><td>");
            match (
                snap.memory.current_swap_used_bytes(),
                snap.memory.swap_percentage_observed(),
            ) {
                (Some(used), Some(percentage)) => out.push_str(&format!(
                    "{} / {} ({percentage:.0}%)",
                    fmt_mib(used),
                    fmt_mib(total),
                )),
                _ => out.push_str(&format!("— / {} (—)", fmt_mib(total))),
            }
            out.push_str("</td></tr>\n");
        }
        None => out.push_str("<tr><th>Swap</th><td>— / — (—)</td></tr>\n"),
    }
    // Uptime + counts.
    out.push_str("<tr><th>Uptime</th><td>");
    out.push_str(&format!("{}s", snap.uptime_secs));
    out.push_str("</td></tr>\n");
    out.push_str("<tr><th>Processes</th><td>");
    out.push_str(&snap.processes.to_string());
    out.push_str("</td></tr>\n");
    out.push_str("<tr><th>Threads</th><td>");
    out.push_str(
        &snap
            .threads
            .map_or_else(|| "—".to_owned(), |threads| threads.to_string()),
    );
    out.push_str("</td></tr>\n");
    out.push_str("</table>\n");

    // ── process table ─────────────────────────────────────────────────────
    out.push_str("<h2>Processes</h2>\n<table>\n");
    out.push_str("<caption>");
    out.push_str(&procs.len().to_string());
    out.push_str(" process(es)</caption>\n");
    out.push_str("<thead><tr><th>Name</th><th>PID</th><th>CPU%</th><th>Memory</th>");
    out.push_str("<th>User</th><th>Status</th><th>Threads</th><th>Disk R</th><th>Disk W</th>");
    out.push_str("<th>CPU time</th>");
    out.push_str("</tr></thead>\n<tbody>\n");
    if procs.is_empty() {
        // Single placeholder row so the table is never header-only (clearer
        // signal that the list was empty, not a rendering failure). colspan
        // covers all 10 columns.
        out.push_str("<tr><td colspan=\"10\"><em>No processes in this snapshot.</em></td></tr>\n");
    } else {
        for p in procs {
            let user = p.current_user();
            out.push_str("<tr><td>");
            html_escape(&p.name, &mut out);
            out.push_str("</td><td class=\"num\">");
            out.push_str(&p.pid.to_string());
            out.push_str("</td><td class=\"num\">");
            match p.current_cpu_percentage() {
                Some(value) if value.is_finite() => out.push_str(&format!("{value:.2}")),
                _ => out.push('—'),
            }
            out.push_str("</td><td class=\"num\">");
            match p.current_memory_bytes() {
                Some(value) => out.push_str(&format!("{:.2}", value as f64 / MB)),
                None => out.push('—'),
            }
            out.push_str("</td><td>");
            html_escape(user.as_deref().unwrap_or("—"), &mut out);
            out.push_str("</td><td>");
            html_escape(&p.status, &mut out);
            out.push_str("</td><td class=\"num\">");
            out.push_str(
                &p.current_threads()
                    .map_or_else(|| "—".to_owned(), |value| value.to_string()),
            );
            out.push_str("</td><td class=\"num\">");
            out.push_str(
                &p.current_disk_read_bytes_per_sec()
                    .map_or_else(|| "—".to_owned(), |value| value.to_string()),
            );
            out.push_str("</td><td class=\"num\">");
            out.push_str(
                &p.current_disk_write_bytes_per_sec()
                    .map_or_else(|| "—".to_owned(), |value| value.to_string()),
            );
            out.push_str("</td><td class=\"num\">");
            match p.current_cpu_time_secs() {
                Some(value) => out.push_str(&format!("{value}s")),
                None => out.push('—'),
            }
            out.push_str("</td></tr>\n");
        }
    }
    out.push_str("</tbody>\n</table>\n");
    out.push_str("</body>\n</html>\n");
    out
}
