//! jev に「このページは検索に合うか」を聞く。
use std::error::Error;

use clap::ValueEnum;
use indexmap::IndexMap;
use jev::cli::ProviderKind;
use jev::model::{Answer, Question, Request};
use serde_json::json;

use crate::history::Link;

/// 同時に投げる数
const PARALLEL: usize = 8;

/// jev CLI と同じく `JEV_PROVIDER` で選ぶ（既定 typesafe）。
fn provider() -> Result<ProviderKind, Box<dyn Error>> {
    match std::env::var("JEV_PROVIDER") {
        Ok(name) => Ok(ProviderKind::from_str(&name, true)?),
        Err(_) => Ok(ProviderKind::Typesafe),
    }
}

/// ページごとに、検索に合う確率（0〜1）を返す。並びは `links` と同じ。
pub fn relevance(links: &[Link], query: &str) -> Result<Vec<f64>, Box<dyn Error>> {
    let kind = provider()?;
    let ask = move |link: &Link| -> Result<f64, String> {
        let req = Request {
            state: json!({ "title": link.title, "host": link.host, "url": link.url }),
            questions: IndexMap::from([(
                "match".to_string(),
                Question::Noul {
                    instructions: json!(format!("Is this web page what someone searching for \"{query}\" is looking for?")),
                    criteria: None,
                },
            )]),
        };
        match jev::provider::of(kind).evaluate(&req).map_err(|e| e.to_string())?.answers.get("match") {
            Some(Answer::Noul { noul }) => Ok(*noul),
            _ => Err("jev の答えに noul が無い".into()),
        }
    };
    // ponytail: 1 ページ 1 リクエスト。ページが数百を超えて遅いなら、1 リクエストに問いをまとめる
    let chunk = links.len().div_ceil(PARALLEL).max(1);
    let scores: Vec<Result<Vec<f64>, String>> = std::thread::scope(|s| {
        let handles: Vec<_> = links
            .chunks(chunk)
            .map(|part| s.spawn(move || part.iter().map(ask).collect::<Result<Vec<_>, _>>()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("jev のスレッドが落ちた".into()))).collect()
    });
    let mut out = Vec::with_capacity(links.len());
    for part in scores {
        out.extend(part?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本物の API を叩く。`cargo test -- --ignored`（TYPESAFE_API_KEY が要る）
    #[test]
    #[ignore]
    fn ranks_by_meaning() {
        let link = |title: &str, host: &str| Link { url: format!("https://{host}/a"), host: host.into(), title: title.into() };
        let links = [
            link("所有権と借用を図で理解する", "zenn.dev"),
            link("今日の晩ごはんレシピ 10 選", "cookpad.com"),
            link("Understanding lifetimes in Rust", "blog.example.com"),
        ];
        let s = relevance(&links, "Rust のメモリ管理").unwrap();
        println!("{s:?}");
        assert!(s[0] > s[1] && s[2] > s[1]);
    }
}
