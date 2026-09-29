use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use url::{Host, Url};

const DEFAULT: &str = include_str!("default_config.toml");

#[derive(Deserialize)]
pub struct Config {
    #[serde(default)]
    pub chrome_root: String,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub tracking_params: Vec<String>,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

pub fn default_path() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".config"))
        .join("bhr/config.toml")
}

/// 読む。無ければ既定の中身を書いてから読む（あとで手で直せるように）。
pub fn load(path: &Path) -> Result<Config, Box<dyn Error>> {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, DEFAULT)?;
    }
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()).into())
}

/// `deny` に1つ足す。コメントや並びは残す。
pub fn add_deny(path: &Path, host: &str) -> Result<(), Box<dyn Error>> {
    let mut doc: toml_edit::DocumentMut = fs::read_to_string(path)?.parse()?;
    doc["deny"]
        .or_insert(toml_edit::array())
        .as_array_mut()
        .ok_or("deny が配列ではない")?
        .push(host);
    fs::write(path, doc.to_string())?;
    Ok(())
}

fn listed(host: &str, list: &[String]) -> bool {
    list.iter()
        .any(|e| host == e || host.strip_suffix(e.as_str()).is_some_and(|p| p.ends_with('.')))
}

impl Config {
    pub fn chrome_root(&self) -> PathBuf {
        if let Some(rest) = self.chrome_root.strip_prefix("~/") {
            home().join(rest)
        } else if !self.chrome_root.is_empty() {
            PathBuf::from(&self.chrome_root)
        } else if cfg!(target_os = "macos") {
            home().join("Library/Application Support/Google/Chrome")
        } else {
            home().join(".config/google-chrome")
        }
    }

    pub fn allowed(&self, host: &str) -> bool {
        // 認証・ログインの画面は、どのサイトのものでも読み物ではない
        let auth = ["auth.", "login.", "signin.", "accounts.", "account.", "id.", "sso."];
        if auth.iter().any(|p| host.starts_with(p)) || listed(host, &self.deny) {
            return false;
        }
        self.allow.is_empty() || listed(host, &self.allow)
    }

    fn tracking(&self, key: &str) -> bool {
        self.tracking_params.iter().any(|p| match p.strip_suffix('*') {
            Some(prefix) => key.starts_with(prefix),
            None => key == p,
        })
    }

    /// 行き先を1つに揃えて、ホストと一緒に返す。拾わないものは None。
    pub fn normalize(&self, raw: &str) -> Option<(String, String)> {
        let mut url = Url::parse(raw).ok()?;
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let host = match url.host()? {
            Host::Domain(d) => d.to_lowercase(),
            Host::Ipv4(_) | Host::Ipv6(_) => return None,
        };
        let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
        let local = [".local", ".localhost", ".test", ".internal"];
        if host == "localhost" || !host.contains('.') || local.iter().any(|s| host.ends_with(s)) {
            return None;
        }
        if !self.allowed(&host) {
            return None;
        }
        url.set_fragment(None);
        let kept: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| !self.tracking(k))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if kept.is_empty() {
            url.set_query(None);
        } else {
            url.query_pairs_mut().clear().extend_pairs(kept);
        }
        // トップページは「読んだ記事」ではない
        if url.path() == "/" && url.query().is_none() {
            return None;
        }
        Some((url.to_string(), host))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        toml::from_str(DEFAULT).unwrap()
    }

    #[test]
    fn filters() {
        let c = cfg();
        assert_eq!(
            c.normalize("https://www.zenn.dev/a/b?utm_source=x&p=1#top"),
            Some(("https://www.zenn.dev/a/b?p=1".into(), "zenn.dev".into()))
        );
        assert_eq!(c.normalize("https://zenn.dev/"), None);
        assert_eq!(c.normalize("https://item.rakuten.co.jp/x"), None);
        assert!(c.normalize("https://notrakuten.co.jp/x").is_some());
        assert_eq!(c.normalize("http://localhost:3000/x"), None);
        assert_eq!(c.normalize("http://192.168.0.1/x"), None);
        assert_eq!(c.normalize("https://auth.example.com/x"), None);

        let only = Config { allow: vec!["zenn.dev".into()], ..cfg() };
        assert!(only.normalize("https://blog.zenn.dev/x").is_some());
        assert_eq!(only.normalize("https://qiita.com/x"), None);
    }
}
