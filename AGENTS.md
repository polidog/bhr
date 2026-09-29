# AGENTS.md

`bhr` (browser history reading): a Rust CLI/TUI that turns Chrome history into a
per-day list of pages read. Ported from `../kyoten/bin/reading.ts`, but standalone
(no kyoten allowlist-from-posts, no writing into kyoten).

## Commands

```sh
cargo build
cargo test            # filter tests live in src/config.rs
cargo test -- --ignored   # hits the real Jev API (needs TYPESAFE_API_KEY); uses made-up pages, never real history
cargo clippy
cargo install --path . --force   # reinstall ~/.cargo/bin/bhr
```

When testing against real history, pass `--config <scratch path>` so the user's
`~/.config/bhr/config.toml` is not touched (it is created automatically if missing).

## Layout

- `src/main.rs` — clap CLI: `tui` (default), `report` (alias `list`), `hosts`, `config`; global `--config/--since/--until/--json`
- `src/config.rs` — `Config` (TOML), `normalize()` (URL cleanup + all filtering), `allowed()`, `add_deny()` (edits TOML with `toml_edit` to keep comments)
- `src/history.rs` — find profiles, copy `History` to temp, query SQLite, dedupe per day, Markdown `render()`
- `src/tui.rs` — ratatui two-pane UI (days / pages); `/` text filter, `?` semantic ranking
- `src/ask.rs` — `relevance()`: one Jev `noul` request per page via the `jev` crate (git dep on polidog/jev, pinned rev), provider from `JEV_PROVIDER`
- `src/default_config.toml` — written to the config path on first run; also used by tests

## Rules

- All filtering goes through `Config::normalize()`; don't filter elsewhere.
- Always-on exclusions (localhost, IPs, auth subdomains, top pages, iframe visits) are hardcoded; everything site-specific belongs in the config, not in code.
- Dedupe key within a day is host + title (not URL), keeping the shortest URL.
- Chrome `visit_time` is microseconds since 1601-01-01; conversion is done in SQL (`date(... 'unixepoch', 'localtime')`).
- Keep dependencies minimal; reuse what is in `Cargo.toml`.

## Ideas not done yet

- Firefox (`places.sqlite`: `moz_places` + `moz_historyvisits`, µs since 1970, skip `visit_type = 8`)
- Safari (`~/Library/Safari/History.db`: `history_items` + `history_visits`, seconds since 2001, needs Full Disk Access)
