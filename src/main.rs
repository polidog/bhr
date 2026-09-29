mod ask;
mod config;
mod history;
mod tui;

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Chrome の履歴から、読んだページを日ごとに出す
#[derive(Parser)]
struct Cli {
    /// 設定ファイル（既定: ~/.config/bhr/config.toml）
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// この日から（YYYY-MM-DD）
    #[arg(long, global = true, value_parser = date)]
    since: Option<String>,
    /// この日まで（YYYY-MM-DD）
    #[arg(long, global = true, value_parser = date)]
    until: Option<String>,
    /// JSON で出す（report / hosts）
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// TUI で見る（既定）
    Tui,
    /// 日ごとに開いたページを出す（Markdown / --json）
    #[command(alias = "list")]
    Report {
        /// 1日だけ（YYYY-MM-DD）
        #[arg(long, value_parser = date)]
        day: Option<String>,
    },
    /// 拾ったホストを件数順に出す（deny に足すものを探す用）
    Hosts {
        #[arg(long, default_value_t = 50)]
        top: usize,
    },
    /// 設定ファイルの場所を出す
    Config,
}

fn date(s: &str) -> Result<String, String> {
    let ok = s.len() == 10
        && s.char_indices().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() });
    if ok { Ok(s.to_string()) } else { Err("YYYY-MM-DD で".into()) }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let path = cli.config.unwrap_or_else(config::default_path);
    let cfg = config::load(&path)?;
    let (mut since, mut until) = (cli.since, cli.until);

    match cli.cmd.unwrap_or(Cmd::Tui) {
        Cmd::Config => println!("{}", path.display()),
        Cmd::Tui => {
            let days = history::load(&cfg, since.as_deref(), until.as_deref())?;
            tui::run(cfg, &path, days)?;
        }
        Cmd::Report { day } => {
            if day.is_some() {
                (since, until) = (day.clone(), day);
            }
            let days = history::load(&cfg, since.as_deref(), until.as_deref())?;
            if cli.json {
                let out: Vec<_> = days.iter().map(|(date, links)| serde_json::json!({ "date": date, "links": links })).collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                let pages: Vec<String> = days.iter().map(|(d, links)| history::render(d, links)).collect();
                print!("{}", pages.join("\n"));
            }
        }
        Cmd::Hosts { top } => {
            let days = history::load(&cfg, since.as_deref(), until.as_deref())?;
            let mut count: HashMap<&str, usize> = HashMap::new();
            for link in days.values().flatten() {
                *count.entry(&link.host).or_default() += 1;
            }
            let mut count: Vec<_> = count.into_iter().collect();
            count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            count.truncate(top);
            if cli.json {
                let out: Vec<_> = count.iter().map(|(host, c)| serde_json::json!({ "host": host, "count": c })).collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                for (host, c) in count {
                    println!("{c:>6}  {host}");
                }
            }
        }
    }
    Ok(())
}
