use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::config::Config;

#[derive(Clone, serde::Serialize)]
pub struct Link {
    pub url: String,
    pub host: String,
    pub title: String,
}

/// 日付（YYYY-MM-DD, ローカル時刻）→ その日に開いたページ
pub type Days = BTreeMap<String, Vec<Link>>;

fn profiles(cfg: &Config) -> Vec<PathBuf> {
    let root = cfg.chrome_root();
    let names: Vec<String> = if cfg.profiles.is_empty() {
        let mut names: Vec<String> = fs::read_dir(&root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n == "Default" || n.strip_prefix("Profile ").is_some_and(|d| d.parse::<u32>().is_ok()))
            .collect();
        names.sort();
        names
    } else {
        cfg.profiles.clone()
    };
    names.iter().map(|n| root.join(n).join("History")).filter(|p| p.exists()).collect()
}

/// Chrome が起きているとロックされているので、一時ディレクトリへ写してから開く。
fn visits(path: &Path, since: &str, until: &str) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
    let copy = std::env::temp_dir().join(format!("bhr-history-{}", std::process::id()));
    fs::copy(path, &copy)?;
    let rows = (|| -> rusqlite::Result<_> {
        let db = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        // visit_time は 1601-01-01 からのマイクロ秒。
        // transition の下位 8bit が 3/4 のものは iframe（本人が開いたページではない）。
        let mut stmt = db.prepare(
            "SELECT u.url, u.title,
                    date(v.visit_time / 1000000 - 11644473600, 'unixepoch', 'localtime') AS d
               FROM visits v JOIN urls u ON u.id = v.url
              WHERE (v.transition & 255) NOT IN (3, 4)
                AND u.title IS NOT NULL AND u.title <> ''
                AND d >= ?1 AND d <= ?2",
        )?;
        stmt.query_map([since, until], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()
    })();
    let _ = fs::remove_file(&copy);
    Ok(rows?)
}

pub fn load(cfg: &Config, since: Option<&str>, until: Option<&str>) -> Result<Days, Box<dyn Error>> {
    let paths = profiles(cfg);
    if paths.is_empty() {
        return Err(format!("History が見つからない: {}", cfg.chrome_root().display()).into());
    }
    let mut days: BTreeMap<String, HashMap<(String, String), Link>> = BTreeMap::new();
    for path in paths {
        for (raw, title, date) in visits(&path, since.unwrap_or(""), until.unwrap_or("9999"))? {
            let Some((url, host)) = cfg.normalize(&raw) else { continue };
            let title: String = title.trim().chars().take(200).collect();
            // 同じ日に同じページを何度開いても1行にする。鍵は URL ではなく
            // ホストと題（検索欄の1文字ずつが別 URL で残るため）。いちばん短い URL を残す。
            let day = days.entry(date).or_default();
            let key = (host.clone(), title.clone());
            if day.get(&key).is_none_or(|seen| url.len() < seen.url.len()) {
                day.insert(key, Link { url, host, title });
            }
        }
    }
    Ok(days
        .into_iter()
        .map(|(date, day)| {
            let mut links: Vec<Link> = day.into_values().collect();
            links.sort_by(|a, b| a.host.cmp(&b.host).then_with(|| a.url.cmp(&b.url)));
            (date, links)
        })
        .collect())
}

/// `scores` があれば合う順の1本の並び（ホストで分けない）、無ければホストごとに分ける。
pub fn render(date: &str, links: &[Link], scores: Option<&[f64]>) -> String {
    let mut out = format!("# {date} に開いたページ\n");
    if scores.is_some() {
        out.push('\n');
    }
    let mut host = "";
    for (i, link) in links.iter().enumerate() {
        let label: String = link.title.replace(['[', ']'], "").split_whitespace().collect::<Vec<_>>().join(" ");
        match scores {
            Some(s) => out.push_str(&format!("- {:.2} [{label}]({}) — {}\n", s[i], link.url, link.host)),
            None => {
                if link.host != host {
                    host = &link.host;
                    out.push_str(&format!("\n## {host}\n\n"));
                }
                out.push_str(&format!("- [{label}]({})\n", link.url));
            }
        }
    }
    out
}
