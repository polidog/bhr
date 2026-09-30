use std::collections::HashMap;
use std::error::Error;
use std::path::Path;
use std::process::{Command, Stdio};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListState, Paragraph};

use crate::ask;
use crate::config::{self, Config};
use crate::history::{Days, Link};

#[derive(PartialEq)]
enum Input {
    None,
    /// `/` 文字で絞る
    Filter,
    /// `?` jev に意味で聞く
    Ask,
}

/// 左の行。月の行はその月ぜんぶ、日の行はその日だけを右に出す
enum Row {
    Month(String),
    Day(usize),
}

struct App {
    /// 除外だけ効いた全部。`days` はこれを検索語で絞ったもの
    all: Vec<(String, Vec<Link>)>,
    days: Vec<(String, Vec<Link>)>,
    /// 左に並ぶ行。日が出るのはカーソルのある月だけ
    rows: Vec<Row>,
    /// 右に並ぶ（日付, ページ）。選んだ行から作り、jev はこれを並べ替える
    view: Vec<(String, Link)>,
    query: String,
    ask: String,
    input: Input,
    /// URL → jev が付けた「検索に合う」確率
    scores: HashMap<String, f64>,
    row: ListState,
    link: ListState,
    on_links: bool,
    status: String,
}

/// 空白で区切った語がすべて、題・ホスト・URL のどこかに入っているか（大文字小文字は見ない）
fn hit(link: &Link, terms: &[String]) -> bool {
    let hay = format!("{}\n{}\n{}", link.title, link.host, link.url).to_lowercase();
    terms.iter().all(|t| hay.contains(t.as_str()))
}

impl App {
    fn refilter(&mut self) {
        let terms: Vec<String> = self.query.to_lowercase().split_whitespace().map(String::from).collect();
        self.days = self
            .all
            .iter()
            .map(|(d, links)| (d.clone(), links.iter().filter(|l| hit(l, &terms)).cloned().collect::<Vec<_>>()))
            .filter(|(_, links)| !links.is_empty())
            .collect();
        // 並びが元に戻るので、jev の点も捨てる
        self.scores.clear();
        let i = self.link.selected().unwrap_or(0);
        self.select(&self.key());
        if !self.view.is_empty() {
            self.link.select(Some(i.min(self.view.len() - 1)));
        }
    }

    fn label(&self, row: &Row) -> String {
        match row {
            Row::Month(m) => m.clone(),
            Row::Day(i) => self.days[*i].0.clone(),
        }
    }

    /// 選んでいる行の月（YYYY-MM）か日（YYYY-MM-DD）
    fn key(&self) -> String {
        self.row.selected().and_then(|i| self.rows.get(i)).map_or(String::new(), |r| self.label(r))
    }

    fn on_month(&self) -> bool {
        matches!(self.row.selected().and_then(|i| self.rows.get(i)), Some(Row::Month(_)))
    }

    /// `key`（月か日）の行を選び、その月だけ日を開いて並べ直す。無くなっていたらいちばん新しい日へ
    fn select(&mut self, key: &str) {
        let key = match self.days.first() {
            Some(_) if !key.is_empty() && self.days.iter().any(|(d, _)| d.starts_with(key)) => key.to_string(),
            Some((d, _)) => d.clone(),
            None => String::new(),
        };
        let open = &key[..key.len().min(7)];
        self.rows.clear();
        for (i, (d, _)) in self.days.iter().enumerate() {
            if i == 0 || d[..7] != self.days[i - 1].0[..7] {
                self.rows.push(Row::Month(d[..7].to_string()));
            }
            if &d[..7] == open {
                self.rows.push(Row::Day(i));
            }
        }
        let at = self.rows.iter().position(|r| self.label(r) == key).unwrap_or(0);
        self.row.select((!self.rows.is_empty()).then_some(at));
        self.view = self
            .days
            .iter()
            .filter(|(d, _)| d.starts_with(&key))
            .flat_map(|(d, links)| links.iter().map(move |l| (d.clone(), l.clone())))
            .collect();
        self.link.select((!self.view.is_empty()).then_some(0));
    }

    /// 右に出ているページを jev に採点させ、合う順に並べ替える。
    fn rank(&mut self, min_score: f64) {
        let links: Vec<Link> = self.view.iter().map(|(_, l)| l.clone()).collect();
        match ask::relevance(&links, &self.ask) {
            Ok(scores) => {
                for (link, s) in links.iter().zip(&scores) {
                    self.scores.insert(link.url.clone(), *s);
                }
                let score = |l: &Link| self.scores.get(&l.url).copied().unwrap_or(0.0);
                let before = self.view.len();
                self.view.retain(|(_, l)| score(l) >= min_score);
                self.view.sort_by(|(_, a), (_, b)| score(b).total_cmp(&score(a)));
                let hits = self.view.len();
                self.link.select((hits > 0).then_some(0));
                self.on_links = hits > 0;
                self.status = format!("jev: 「{}」に合う順  {hits}/{before} 件（{min_score} 以上、Esc で戻す）", self.ask);
            }
            Err(e) => self.status = format!("jev に聞けない: {e}"),
        }
    }

    fn current(&self) -> Option<&Link> {
        self.link.selected().and_then(|i| self.view.get(i)).map(|(_, l)| l)
    }

    fn step(&mut self, down: bool) {
        let (cur, len) = if self.on_links {
            (self.link.selected(), self.view.len())
        } else {
            (self.row.selected(), self.rows.len())
        };
        if len == 0 {
            return;
        }
        let i = cur.unwrap_or(0);
        let i = if down { (i + 1).min(len - 1) } else { i.saturating_sub(1) };
        if self.on_links {
            self.link.select(Some(i));
        } else {
            self.select(&self.label(&self.rows[i]));
        }
    }
}

fn open(url: &str) -> std::io::Result<()> {
    let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    Command::new(cmd).arg(url).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ())
}

fn draw(f: &mut Frame, app: &mut App) {
    let [main, foot] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());
    let [left, right] = Layout::horizontal([Constraint::Length(20), Constraint::Min(1)]).areas(main);
    let focus = |on: bool| Style::default().fg(if on { Color::Cyan } else { Color::DarkGray });
    let hl = Style::default().add_modifier(Modifier::REVERSED);

    let rows = List::new(app.rows.iter().map(|r| match r {
        Row::Month(m) => {
            let n: usize = app.days.iter().filter(|(d, _)| d.starts_with(m.as_str())).map(|(_, l)| l.len()).sum();
            Line::styled(format!("{m} {n:>7}"), Style::default().add_modifier(Modifier::BOLD))
        }
        Row::Day(i) => {
            let (d, l) = &app.days[*i];
            Line::raw(format!("  {} {:>7}", &d[5..], l.len()))
        }
    }))
    .block(Block::bordered().title("月・日").border_style(focus(!app.on_links)))
    .highlight_style(hl);
    f.render_stateful_widget(rows, left, &mut app.row);

    let month = app.on_month();
    let links = List::new(app.view.iter().map(|(d, l)| {
        let mut spans = Vec::new();
        if month {
            spans.push(Span::styled(format!("{} ", &d[5..]), Style::default().fg(Color::Cyan)));
        }
        if let Some(s) = app.scores.get(&l.url) {
            spans.push(Span::styled(format!("{s:.2} "), Style::default().fg(Color::Green)));
        }
        spans.push(Span::styled(format!("{:<24} ", l.host), Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(l.title.clone()));
        Line::from(spans)
    }))
    .block(Block::bordered().title("開いたページ").border_style(focus(app.on_links)))
    .highlight_style(hl);
    f.render_stateful_widget(links, right, &mut app.link);

    let text = match app.current() {
        _ if app.input == Input::Filter => format!("/{}▏  (Enter 決定  Esc 消す)", app.query),
        _ if app.input == Input::Ask => format!("?{}▏  (Enter で jev に聞く  Esc やめる)", app.ask),
        Some(l) if app.on_links => format!("{}  |  {}", l.url, app.status),
        _ => app.status.clone(),
    };
    let text = if app.input == Input::None && !app.query.is_empty() {
        format!("[/{}] {text}", app.query)
    } else {
        text
    };
    f.render_widget(Paragraph::new(text).style(Style::default().fg(Color::DarkGray)), foot);
}

pub fn run(mut cfg: Config, path: &Path, days: Days) -> Result<(), Box<dyn Error>> {
    let mut app = App {
        all: days.into_iter().rev().collect(),
        days: Vec::new(),
        rows: Vec::new(),
        view: Vec::new(),
        query: String::new(),
        ask: String::new(),
        input: Input::None,
        scores: HashMap::new(),
        row: ListState::default(),
        link: ListState::default(),
        on_links: false,
        status: "j/k 移動  h/l 切替  / 検索  ? jev で検索  Enter 開く  x このホストを除外  q 終了".into(),
    };
    app.refilter();

    let mut term = ratatui::init();
    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            term.draw(|f| draw(f, &mut app))?;

            let Event::Key(key) = event::read()? else { continue };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.input != Input::None {
                let filter = app.input == Input::Filter;
                let buf = if filter { &mut app.query } else { &mut app.ask };
                match key.code {
                    KeyCode::Enter => app.input = Input::None,
                    KeyCode::Esc => {
                        app.input = Input::None;
                        buf.clear();
                    }
                    KeyCode::Backspace => {
                        buf.pop();
                    }
                    KeyCode::Char(c) => buf.push(c),
                    // 打ちながらでも動けるように（j/k は文字として入るので矢印で）
                    KeyCode::Down | KeyCode::Up => {
                        app.step(key.code == KeyCode::Down);
                        continue;
                    }
                    KeyCode::Tab => {
                        app.on_links = !app.on_links;
                        continue;
                    }
                    _ => continue,
                }
                if filter {
                    app.refilter();
                } else if key.code == KeyCode::Enter && !app.ask.trim().is_empty() {
                    // ponytail: 待っている間は画面が止まる。遅くて困るならスレッドに出す
                    app.status = format!("jev に聞いている…（{} ページ）", app.view.len());
                    term.draw(|f| draw(f, &mut app))?;
                    app.rank(cfg.min_score);
                }
                continue;
            }
            match key.code {
                KeyCode::Char('/') => app.input = Input::Filter,
                KeyCode::Char('?') => {
                    app.input = Input::Ask;
                    app.ask.clear();
                }
                // 絞り込み中の Esc は検索を解くだけ
                KeyCode::Esc if !app.query.is_empty() || !app.scores.is_empty() => {
                    app.query.clear();
                    app.refilter();
                }
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
                    for (_, links) in &mut app.all {
                        links.retain(|l| cfg.allowed(&l.host));
                    }
                    app.all.retain(|(_, l)| !l.is_empty());
                    app.refilter();
                    app.status = format!("{host} を deny に足した");
                }
                _ => {}
            }
        }
    })();
    ratatui::restore();
    result
}
