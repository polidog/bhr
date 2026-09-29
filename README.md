# bhr — browser history reading

Turn your Chrome history into a per-day list of pages you actually read.
Work tools, search pages, SNS timelines, localhost and the like are filtered out
by a config file, and repeated visits to the same page collapse into one line.

Works with Chrome and Chromium-based browsers (Brave, Edge, Vivaldi, Arc) by
pointing `chrome_root` at their profile directory.

## Install

```sh
cargo install --path .
```

## Usage

```sh
bhr                                # TUI (default)
bhr list --day 2026-09-28          # one day as Markdown (`list` is an alias of `report`)
bhr list --since 2026-09-01        # a range (--until also works)
bhr list --json --since 2026-09-01 # [{"date", "links": [{"host", "title", "url"}]}]
bhr hosts --top 50                 # hosts by count — find what to deny
bhr hosts --json                   # [{"host", "count"}]
bhr config                         # print the config path
```

`--config`, `--since`, `--until` and `--json` work with every subcommand.

### TUI keys

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | move |
| `h` / `l`, `Tab` | switch between days and pages |
| `Enter` | open the page in the browser (`xdg-open` / `open`) |
| `x` | add the selected page's host to `deny` (written to the config) |
| `q` / `Esc` | quit |

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
