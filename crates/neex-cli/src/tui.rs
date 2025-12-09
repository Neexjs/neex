//! Neex TUI - Beautiful Terminal UI
//!
//! Command Center layout with:
//! - Header: Logo, status, progress bar
//! - Sidebar: Task list with live status
//! - Main: Log output with syntax highlighting
//! - Footer: Keyboard shortcuts

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::System;

/// Task status for display
#[derive(Clone, Debug, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed(u64), // ms
    Failed(String),
    Cached(u64),    // ms
}

/// Task for TUI display
#[derive(Clone, Debug)]
pub struct TuiTask {
    pub name: String,
    pub status: TaskStatus,
    pub logs: Vec<String>,
}

impl TuiTask {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: TaskStatus::Pending,
            logs: Vec::new(),
        }
    }
}

/// TUI State
pub struct TuiState {
    pub tasks: Vec<TuiTask>,
    pub selected: usize,
    pub progress: f64,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub cache_hits: usize,
    pub start_time: Instant,
    pub should_quit: bool,
    pub p2p_peers: usize,
    pub cloud_enabled: bool,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            selected: 0,
            progress: 0.0,
            total_tasks: 0,
            completed_tasks: 0,
            cache_hits: 0,
            start_time: Instant::now(),
            should_quit: false,
            p2p_peers: 0,
            cloud_enabled: false,
        }
    }
}

impl TuiState {
    pub fn add_task(&mut self, name: &str) {
        self.tasks.push(TuiTask::new(name));
        self.total_tasks = self.tasks.len();
    }

    pub fn update_task(&mut self, name: &str, status: TaskStatus) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.name == name) {
            task.status = status.clone();
            
            match status {
                TaskStatus::Completed(_) | TaskStatus::Failed(_) => {
                    self.completed_tasks += 1;
                }
                TaskStatus::Cached(_) => {
                    self.completed_tasks += 1;
                    self.cache_hits += 1;
                }
                _ => {}
            }
            
            self.progress = self.completed_tasks as f64 / self.total_tasks as f64;
        }
    }

    pub fn add_log(&mut self, name: &str, log: &str) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.name == name) {
            task.logs.push(log.to_string());
        }
    }

    pub fn next(&mut self) {
        if !self.tasks.is_empty() {
            self.selected = (self.selected + 1) % self.tasks.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.tasks.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.tasks.len() - 1);
        }
    }
}

/// Run TUI application
pub fn run_tui(state: Arc<Mutex<TuiState>>) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // System info
    let mut sys = System::new();

    // Main loop
    loop {
        // Update system info
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let cpu = sys.global_cpu_usage();
        let mem = sys.used_memory() / 1024 / 1024; // MB

        // Draw
        {
            let state_guard = state.lock().unwrap();
            terminal.draw(|f| ui(f, &state_guard, cpu, mem))?;
            
            if state_guard.should_quit {
                break;
            }
        }

        // Handle events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut state_guard = state.lock().unwrap();
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            state_guard.should_quit = true;
                        }
                        KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
                            state_guard.next();
                        }
                        KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => {
                            state_guard.prev();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Cleanup
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Draw UI
fn ui(f: &mut Frame, state: &TuiState, cpu: f32, mem: u64) {
    let size = f.area();

    // Main layout: Header, Content, Footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Content
            Constraint::Length(1), // Footer
        ])
        .split(size);

    // Header
    draw_header(f, chunks[0], state, cpu, mem);

    // Content: Sidebar + Main
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28), // Sidebar
            Constraint::Min(40),    // Main
        ])
        .split(chunks[1]);

    // Sidebar
    draw_sidebar(f, content_chunks[0], state);

    // Main log panel
    draw_main(f, content_chunks[1], state);

    // Footer
    draw_footer(f, chunks[2]);
}

/// Draw header with logo, status, progress
fn draw_header(f: &mut Frame, area: Rect, state: &TuiState, cpu: f32, mem: u64) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(10),  // Logo
            Constraint::Length(30),  // Status
            Constraint::Min(20),     // Progress
        ])
        .split(area);

    // Logo
    let logo = Paragraph::new("🚀 NEEX")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(logo, header_chunks[0]);

    // Status
    let p2p = if state.p2p_peers > 0 {
        format!("P2P:{}", state.p2p_peers)
    } else {
        "P2P:Off".to_string()
    };
    let cloud = if state.cloud_enabled { "☁️ On" } else { "☁️ Off" };
    let status_text = format!(" {} │ {} │ CPU:{}% │ {}MB", p2p, cloud, cpu as u32, mem);
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(status, header_chunks[1]);

    // Progress
    let progress_label = format!(
        "Progress: {}/{} ({:.0}%)",
        state.completed_tasks,
        state.total_tasks,
        state.progress * 100.0
    );
    let progress = Gauge::default()
        .block(Block::default().borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
        .percent((state.progress * 100.0) as u16)
        .label(progress_label);
    f.render_widget(progress, header_chunks[2]);
}

/// Draw sidebar with task list
fn draw_sidebar(f: &mut Frame, area: Rect, state: &TuiState) {
    let items: Vec<ListItem> = state
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let (icon, style) = match &task.status {
                TaskStatus::Pending => ("⏸", Style::default().fg(Color::DarkGray)),
                TaskStatus::Running => ("⏳", Style::default().fg(Color::Yellow)),
                TaskStatus::Completed(ms) => {
                    let text = format!("✓ {} {}ms", task.name, ms);
                    return ListItem::new(text).style(Style::default().fg(Color::Green));
                }
                TaskStatus::Cached(ms) => {
                    let text = format!("⚡ {} {}ms", task.name, ms);
                    return ListItem::new(text).style(Style::default().fg(Color::Cyan));
                }
                TaskStatus::Failed(_) => ("✗", Style::default().fg(Color::Red)),
            };
            
            let text = format!("{} {}", icon, task.name);
            let mut item = ListItem::new(text).style(style);
            
            if i == state.selected {
                item = item.style(style.add_modifier(Modifier::REVERSED));
            }
            
            item
        })
        .collect();

    let cache_rate = if state.total_tasks > 0 {
        state.cache_hits * 100 / state.total_tasks
    } else {
        0
    };

    let title = format!("📦 Tasks (Cache: {}%)", cache_rate);
    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    
    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    
    f.render_stateful_widget(list, area, &mut list_state);
}

/// Draw main log panel
fn draw_main(f: &mut Frame, area: Rect, state: &TuiState) {
    let selected_task = state.tasks.get(state.selected);
    
    let (title, logs) = match selected_task {
        Some(task) => {
            let title = format!("📋 {}", task.name);
            let logs: Vec<Line> = task.logs.iter().map(|log| {
                // Syntax highlighting
                let style = if log.contains("error") || log.contains("Error") || log.contains("ERROR") {
                    Style::default().fg(Color::Red)
                } else if log.contains("warn") || log.contains("Warn") || log.contains("WARN") {
                    Style::default().fg(Color::Yellow)
                } else if log.contains("✓") || log.contains("success") || log.contains("Success") {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                
                Line::from(Span::styled(log.clone(), style))
            }).collect();
            (title, logs)
        }
        None => ("📋 No task selected".to_string(), vec![]),
    };

    let paragraph = Paragraph::new(logs)
        .block(Block::default().title(title).borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Draw footer with shortcuts
fn draw_footer(f: &mut Frame, area: Rect) {
    let shortcuts = Line::from(vec![
        Span::styled(" [Tab]", Style::default().fg(Color::Cyan)),
        Span::raw(" Switch "),
        Span::styled("[↑↓]", Style::default().fg(Color::Cyan)),
        Span::raw(" Navigate "),
        Span::styled("[q]", Style::default().fg(Color::Cyan)),
        Span::raw(" Quit "),
    ]);
    
    let footer = Paragraph::new(shortcuts)
        .style(Style::default().bg(Color::DarkGray));
    
    f.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_add_task() {
        let mut state = TuiState::default();
        state.add_task("web:build");
        state.add_task("ui:build");
        
        assert_eq!(state.tasks.len(), 2);
        assert_eq!(state.total_tasks, 2);
    }

    #[test]
    fn test_state_update_task() {
        let mut state = TuiState::default();
        state.add_task("web:build");
        state.update_task("web:build", TaskStatus::Completed(100));
        
        assert_eq!(state.completed_tasks, 1);
        assert_eq!(state.progress, 1.0);
    }

    #[test]
    fn test_state_cache_hit() {
        let mut state = TuiState::default();
        state.add_task("web:build");
        state.update_task("web:build", TaskStatus::Cached(50));
        
        assert_eq!(state.cache_hits, 1);
    }

    #[test]
    fn test_navigation() {
        let mut state = TuiState::default();
        state.add_task("a");
        state.add_task("b");
        state.add_task("c");
        
        assert_eq!(state.selected, 0);
        state.next();
        assert_eq!(state.selected, 1);
        state.next();
        assert_eq!(state.selected, 2);
        state.next();
        assert_eq!(state.selected, 0); // Wrap
    }
}
