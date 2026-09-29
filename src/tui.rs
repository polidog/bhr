use std::error::Error;
use std::path::Path;
use std::process::{Command, Stdio};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListState, Paragraph};

use crate::config::{self, Config};
use crate::history::{Days, Link};

struct App {
    days: Vec<(String, Vec<Link>)>,
    day: ListState,
    link: ListState,
    on_links: bool,
    status: String,
}

impl App {
    fn links(&self) -> &[Link] {
        self.day.selected().and_then(|i| self.days.get(i)).map_or(&[], |d| &d.1)
    }

    fn current(&self) -> Option<&Link> {
        self.link.selected().and_then(|i| self.links().get(i))
    }

    fn select_day(&mut self, i: usize) {
        self.day.select(Some(i.min(self.days.len().saturating_sub(1))));
        self.link.select(if self.links().is_empty() { None } else { Some(0) });
    }

    fn step(&mut self, down: bool) {
        let (cur, len) = if self.on_links {
            (self.link.selected(), self.links().len())
        } else {
            (self.day.selected(), self.days.len())
        };
        if len == 0 {
            return;
        }
        let i = cur.unwrap_or(0);
        let i = if down { (i + 1).min(len - 1) } else { i.saturating_sub(1) };
        if self.on_links {
            self.link.select(Some(i));
        } else {
            self.select_day(i);
        }
    }
}

fn open(url: &str) -> std::io::Result<()> {
    let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    Command::new(cmd).arg(url).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ())
}

pub fn run(mut cfg: Config, path: &Path, days: Days) -> Result<(), Box<dyn Error>> {
    let mut app = App {
        days: days.into_iter().rev().collect(),
        day: ListState::default(),
        link: ListState::default(),
        on_links: false,
        status: "j/k 移動  h/l 切替  Enter 開く  x このホストを除外  q 終了".into(),
    };
    app.select_day(0);

    let mut term = ratatui::init();
    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            term.draw(|f| {
                let [main, foot] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());
                let [left, right] = Layout::horizontal([Constraint::Length(20), Constraint::Min(1)]).areas(main);
                let focus = |on: bool| Style::default().fg(if on { Color::Cyan } else { Color::DarkGray });
                let hl = Style::default().add_modifier(Modifier::REVERSED);

                let days = List::new(app.days.iter().map(|(d, l)| format!("{d} {:>4}", l.len())))
                    .block(Block::bordered().title("日").border_style(focus(!app.on_links)))
                    .highlight_style(hl);
                f.render_stateful_widget(days, left, &mut app.day);

                let links = List::new(app.links().iter().map(|l| {
                    Line::from(vec![Span::styled(format!("{:<24} ", l.host), Style::default().fg(Color::Yellow)), Span::raw(l.title.clone())])
                }))
                .block(Block::bordered().title("開いたページ").border_style(focus(app.on_links)))
                .highlight_style(hl);
                f.render_stateful_widget(links, right, &mut app.link);

                let foot_text = match app.current() {
                    Some(l) if app.on_links => format!("{}  |  {}", l.url, app.status),
                    _ => app.status.clone(),
                };
                f.render_widget(Paragraph::new(foot_text).style(Style::default().fg(Color::DarkGray)), foot);
            })?;

            let Event::Key(key) = event::read()? else { continue };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('j') | KeyCode::Down => app.step(true),
                KeyCode::Char('k') | KeyCode::Up => app.step(false),
                KeyCode::Char('h') | KeyCode::Left => app.on_links = false,
                KeyCode::Char('l') | KeyCode::Right => app.on_links = true,
                KeyCode::Tab => app.on_links = !app.on_links,
                KeyCode::Enter if !app.on_links => app.on_links = true,
                KeyCode::Enter => {
                    if let Some(url) = app.current().map(|l| l.url.clone()) {
                        app.status = match open(&url) {
                            Ok(()) => "開いた".into(),
                            Err(e) => format!("開けない: {e}"),
                        };
                    }
                }
                KeyCode::Char('x') if app.on_links => {
                    let Some(host) = app.current().map(|l| l.host.clone()) else { continue };
                    config::add_deny(path, &host)?;
                    cfg.deny.push(host.clone());
                    for (_, links) in &mut app.days {
                        links.retain(|l| cfg.allowed(&l.host));
                    }
                    app.days.retain(|(_, l)| !l.is_empty());
                    let (d, i) = (app.day.selected().unwrap_or(0), app.link.selected().unwrap_or(0));
                    app.select_day(d);
                    if !app.links().is_empty() {
                        app.link.select(Some(i.min(app.links().len() - 1)));
                    }
                    app.status = format!("{host} を deny に足した");
                }
                _ => {}
            }
        }
    })();
    ratatui::restore();
    result
}
