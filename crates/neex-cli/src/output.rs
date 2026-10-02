//! Output - turns executor events into terminal / CI logs
//!
//! Modes:
//! - stream:  lines appear as they are produced, prefixed `project:task: `
//! - grouped: each task's lines are printed together when it finishes;
//!   on GitHub Actions successful tasks are folded into `::group::`
//!   blocks and failed tasks are printed unfolded so errors are visible

use neex_core::artifacts::{LogLine, Stream};
use neex_core::{Event, RunSummary, TaskStatus, TaskSummary};
use std::collections::HashMap;
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LogOrder {
    Auto,
    Stream,
    Grouped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputLogs {
    /// Everything, including output replayed from the cache
    Full,
    /// Live output only; cache hits print one status line
    NewOnly,
    /// Only the output of failed tasks
    ErrorsOnly,
    /// Status lines only
    None,
}

pub struct Printer {
    grouped: bool,
    github: bool,
    color: bool,
    logs: OutputLogs,
    buffers: HashMap<String, Vec<LogLine>>,
    colors: HashMap<String, &'static str>,
    width: usize,
}

const PALETTE: &[&str] = &["36", "35", "34", "33", "32", "96", "95", "94", "93", "92"];

impl Printer {
    pub fn new(order: LogOrder, logs: OutputLogs, task_ids: &[String]) -> Self {
        let github = std::env::var("GITHUB_ACTIONS")
            .map(|v| v == "true")
            .unwrap_or(false);
        let grouped = match order {
            LogOrder::Grouped => true,
            LogOrder::Stream => false,
            LogOrder::Auto => github,
        };
        let color = std::env::var_os("NO_COLOR").is_none()
            && (std::io::IsTerminal::is_terminal(&std::io::stdout()) || github);
        let colors = task_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), PALETTE[i % PALETTE.len()]))
            .collect();
        let width = task_ids.iter().map(|t| t.len()).max().unwrap_or(0).min(40);
        Self {
            grouped,
            github,
            color,
            logs,
            buffers: HashMap::new(),
            colors,
            width,
        }
    }

    fn paint(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{}m{}\x1b[0m", code, s)
        } else {
            s.to_string()
        }
    }

    fn prefix(&self, id: &str) -> String {
        let code = self.colors.get(id).copied().unwrap_or("36");
        self.paint(code, &format!("{:<w$} │", id, w = self.width))
    }

    fn print_line(&self, id: &str, line: &LogLine) {
        let text = format!("{} {}", self.prefix(id), line.text);
        match line.stream {
            Stream::Stdout => println!("{}", text),
            Stream::Stderr => eprintln!("{}", text),
        }
    }

    fn live_lines_visible(&self) -> bool {
        matches!(self.logs, OutputLogs::Full | OutputLogs::NewOnly)
    }

    pub fn handle(&mut self, event: &Event) {
        match event {
            Event::Started { id, command } => {
                if self.grouped {
                    self.buffers.entry(id.clone()).or_default();
                } else if self.logs != OutputLogs::None {
                    println!(
                        "{} {}",
                        self.prefix(id),
                        self.paint("2", &format!("$ {}", command))
                    );
                }
            }
            Event::Line { id, line } => {
                if self.grouped || self.logs == OutputLogs::ErrorsOnly {
                    self.buffers
                        .entry(id.clone())
                        .or_default()
                        .push(line.clone());
                } else if self.live_lines_visible() {
                    self.print_line(id, line);
                }
            }
            Event::Replay { id, logs } => {
                if self.logs == OutputLogs::Full {
                    if self.grouped {
                        self.buffers
                            .entry(id.clone())
                            .or_default()
                            .extend(logs.iter().cloned());
                    } else {
                        for l in logs {
                            self.print_line(id, l);
                        }
                    }
                }
            }
            Event::Finished { id, summary } => self.finish(id, summary),
            Event::Warning(w) => eprintln!("{} {}", self.paint("33", "warning:"), w),
        }
        let _ = std::io::stdout().flush();
    }

    fn finish(&mut self, id: &str, s: &TaskSummary) {
        let lines = self.buffers.remove(id).unwrap_or_default();
        let failed = matches!(s.status, TaskStatus::Failed { .. });
        let show_lines = match self.logs {
            OutputLogs::Full | OutputLogs::NewOnly => self.grouped,
            OutputLogs::ErrorsOnly => failed,
            OutputLogs::None => false,
        };
        let status = self.status_line(s);

        // GitHub renders workflow-command text literally: no ANSI codes there
        if self.github && self.grouped && !failed && show_lines && !lines.is_empty() {
            println!("::group::{} {}", id, strip_ansi(&status));
            for l in &lines {
                println!("{}", l.text);
            }
            println!("::endgroup::");
            return;
        }
        if show_lines {
            for l in &lines {
                self.print_line(id, l);
            }
        }
        if failed && self.github {
            println!("::error title={}::{}", id, strip_ansi(&status));
        }
        println!("{} {}", self.prefix(id), status);
    }

    fn status_line(&self, s: &TaskSummary) -> String {
        match &s.status {
            TaskStatus::Success => {
                let mut t = self.paint("32", &format!("✓ done in {}", fmt_ms(s.duration_ms)));
                if let Some(r) = miss_hint(&s.miss_reasons) {
                    t.push_str(&self.paint("2", &format!("  (cache miss: {})", r)));
                }
                t
            }
            TaskStatus::CacheHit { source } => self.paint(
                "36",
                &format!(
                    "⚡ cache hit ({}) {}",
                    match source {
                        neex_core::executor::CacheSource::Local => "local",
                        neex_core::executor::CacheSource::Remote => "remote",
                    },
                    s.key
                        .as_deref()
                        .map(|k| &k[..12.min(k.len())])
                        .unwrap_or("")
                ),
            ),
            TaskStatus::Failed { exit_code } => self.paint(
                "31",
                &format!(
                    "✗ failed (exit {}) after {}",
                    exit_code,
                    fmt_ms(s.duration_ms)
                ),
            ),
            TaskStatus::Skipped { reason } => self.paint("33", &format!("− skipped: {}", reason)),
            TaskStatus::NoCommand => self.paint("2", "· no command, skipped"),
        }
    }

    /// Final summary; failed tasks' output is repeated when it was hidden
    pub fn summary(&self, run: &RunSummary) {
        println!();
        let skipped = run
            .tasks
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::Skipped { .. }))
            .count();
        let ran: Vec<&TaskSummary> = run
            .tasks
            .iter()
            .filter(|t| !matches!(t.status, TaskStatus::NoCommand))
            .collect();
        let ok = ran.iter().filter(|t| t.status.ok()).count();

        if self.logs == OutputLogs::None {
            for t in run
                .tasks
                .iter()
                .filter(|t| matches!(t.status, TaskStatus::Failed { .. }))
            {
                for l in &t.failure_logs {
                    self.print_line(&t.id, l);
                }
            }
        }

        let mut line = format!(" Tasks:  {} successful, {} total", ok, ran.len());
        if run.cached > 0 {
            line.push_str(&format!(" · {} cached", run.cached));
        }
        if run.failed > 0 {
            line.push_str(&format!(" · {} failed", run.failed));
        }
        if skipped > 0 {
            line.push_str(&format!(" · {} skipped", skipped));
        }
        println!("{}", self.paint("1", &line));
        let mut time = format!("  Time:  {}", fmt_ms(run.duration_ms));
        if !ran.is_empty() && run.cached == ran.len() {
            time.push_str(&self.paint("36", "  ⚡ everything from cache"));
        }
        println!("{}", time);
        if run.failed > 0 {
            let failed: Vec<String> = run
                .tasks
                .iter()
                .filter_map(|t| match t.status {
                    TaskStatus::Failed { exit_code } => {
                        Some(format!("{} (exit {})", t.id, exit_code))
                    }
                    _ => None,
                })
                .collect();
            println!(
                "{}",
                self.paint("31", &format!(" Failed:  {}", failed.join(", ")))
            );
        }
    }
}

/// Remove `ESC [ ... m` color sequences
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn miss_hint(reasons: &[String]) -> Option<String> {
    match reasons.len() {
        0 => None,
        1 => Some(reasons[0].clone()),
        n => Some(format!("{}, +{} more", reasons[0], n - 1)),
    }
}

pub fn fmt_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}m{}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}
