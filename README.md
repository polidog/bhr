# bhr — browser history reading

[日本語](README.ja.md)

Turn your Chrome history into a per-day list of pages you actually read.
Work tools, search pages, SNS timelines, localhost and the like are filtered out
by a config file, and repeated visits to the same page collapse into one line.

Works with Chrome and Chromium-based browsers (Brave, Edge, Vivaldi, Arc) by
pointing `chrome_root` at their profile directory.

## Install

```sh
cargo install --git https://github.com/polidog/bhr
# or, from a clone
cargo install --path .
```

## Usage

```sh
bhr                                # TUI (default)
bhr list --day 2026-09-28          # one day as Markdown (`list` is an alias of `report`)
bhr list --since 2026-09-01        # a range (--until also works)
bhr list --json --since 2026-09-01 # [{"date", "links": [{"host", "title", "url"}]}]
bhr list --since 2026-09-01 --ask "Rust memory management"  # rank each day's pages with Jev
bhr hosts --top 50                 # hosts by count — find what to deny
bhr hosts --json                   # [{"host", "count"}]
bhr config                         # print the config path
```

`--config`, `--since`, `--until` and `--json` work with every subcommand.

### Searching from the CLI

Text search needs nothing extra — each page is one line in Markdown, and JSON works with `jq`:

```sh
bhr list --since 2026-09-01 | grep -i rust
bhr list --json --since 2026-09-01 | jq '[.[] | .links |= map(select(.title | test("rust"; "i")))]'
```

For meaning-based search use `--ask` (see [Semantic search](#semantic-search)). Each day is
sorted by relevance, and every JSON link gets a `score` (0–1). Pick your own cutoff downstream:

```sh
bhr list --json --since 2026-09-01 --ask "Rust memory management" \
  | jq '[.[].links[] | select(.score > 0.4)]'
```

Piping `bhr list` into the `jev` CLI does not work for this: `jev` treats all of stdin as one
state and returns one answer, not a score per page.

### TUI keys

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | move |
| `h` / `l`, `Tab` | switch between days and pages |
| `/` | filter by text (all words must appear in title, host or URL; `Esc` clears) |
| `?` | semantic search with [jev](https://github.com/polidog/jev): ranks the current day's pages by relevance |
| `Enter` | open the page in the browser (`xdg-open` / `open`) |
| `x` | add the selected page's host to `deny` (written to the config) |
| `q` / `Esc` | quit |

### Semantic search

`?` in the TUI (selected day) and `list --ask` (every day in range) work the same way.
Each page (title, host, URL) is sent to Jev with a yes/no
question — "is this what someone searching for *query* wants?" — and the list is
sorted by that probability. Combine with `/` to narrow the pages first.

One request per page, 8 in parallel; the TUI waits until all answers are back.
With `--ask`, narrow the range with `--since` / `--day` — a month of history can be thousands of requests.
The provider follows the `jev` CLI: `JEV_PROVIDER` = `typesafe` (default,
`TYPESAFE_API_KEY`), `cloudflare` (`CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN`)
or `vercel` (`AI_GATEWAY_API_KEY`).

Note: this sends those page titles and URLs to the provider.

## Config

`~/.config/bhr/config.toml` (respects `$XDG_CONFIG_HOME`). Created with defaults on first run.

```toml
chrome_root = ""     # empty = OS default (~/.config/google-chrome, ~/Library/Application Support/Google/Chrome)
profiles = []        # empty = Default and every "Profile N"
allow = []           # empty = everything not denied; otherwise only these hosts
deny = ["github.com", "x.com", "google.com"]   # hosts to drop, subdomains included
tracking_params = ["utm_*", "fbclid", "ref"]   # query params to strip; trailing * = prefix
```

Always dropped regardless of config: localhost, IP addresses, `.local` / `.test` /
`.internal` hosts, auth pages (`auth.`, `login.`, `accounts.` …), top pages (`/`),
and iframe visits.

Tip: deny-only mode lets work domains through. If you only want reading material,
list the sites you read in `allow`.

## How it works

- The `History` SQLite file is copied to a temp dir before opening (Chrome locks it while running).
- Dates are local time.
- Within a day, pages are deduplicated by host + title, keeping the shortest URL.
- Chrome drops history older than ~90 days, so export regularly if you want to keep it.
