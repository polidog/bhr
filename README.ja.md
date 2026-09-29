# bhr — browser history reading

[English](README.md)

Chrome の履歴から、実際に読んだページを日ごとの一覧にします。
仕事の道具・検索ページ・SNS のタイムライン・localhost などは設定ファイルで除外し、
同じページを何度開いても1行にまとめます。

Chromium 系のブラウザ（Brave, Edge, Vivaldi, Arc）も、`chrome_root` をそのブラウザの
プロファイルのフォルダにすれば読めます。

## インストール

```sh
cargo install --git https://github.com/polidog/bhr
# clone したものからなら
cargo install --path .
```

## 使い方

```sh
bhr                                # TUI（既定）
bhr list --day 2026-09-28          # 1日ぶんを Markdown で（`list` は `report` の別名）
bhr list --since 2026-09-01        # 期間で（--until も使えます）
bhr list --json --since 2026-09-01 # [{"date", "links": [{"host", "title", "url"}]}]
bhr list --since 2026-09-01 --ask "Rust のメモリ管理"  # jev で日ごとに合う順に並べる
bhr hosts --top 50                 # ホストを件数順に（除外するものを探す用）
bhr hosts --json                   # [{"host", "count"}]
bhr config                         # 設定ファイルの場所を出す
```

`--config`・`--since`・`--until`・`--json` はどのサブコマンドでも使えます。

### CLI で検索する

文字の検索は、ほかに何も要りません。Markdown では1ページが1行なので grep で絞れますし、
JSON なら `jq` が使えます。

```sh
bhr list --since 2026-09-01 | grep -i rust
bhr list --json --since 2026-09-01 | jq '[.[] | .links |= map(select(.title | test("rust"; "i")))]'
```

意味で探すときは `--ask` を使います（[意味で検索](#意味で検索)を参照）。日ごとに合う順に並び、
JSON ではページごとに `score`（0〜1）が付きます。どこから上を拾うかは、受け取る側で決めてください。

```sh
bhr list --json --since 2026-09-01 --ask "Rust のメモリ管理" \
  | jq '[.[].links[] | select(.score > 0.4)]'
```

`bhr list` の出力を `jev` の CLI にパイプで渡しても、この用途には使えません。`jev` は標準入力を
丸ごと1つの状態として受け取り、答えを1つ返すだけなので、ページごとの点数にはなりません。

### TUI のキー

| キー | 動き |
|---|---|
| `j` / `k`, `↓` / `↑` | 移動 |
| `h` / `l`, `Tab` | 日付とページの一覧を切り替え |
| `/` | 文字で絞り込む（空白で区切った語がすべて、題・ホスト・URL のどこかに入っているもの。`Esc` で解除） |
| `?` | [jev](https://github.com/polidog/jev) で意味で検索。選んでいる日のページを合う順に並べる |
| `Enter` | ブラウザで開く（`xdg-open` / `open`） |
| `x` | 選んだページのホストを `deny` に足す（設定ファイルに書き込みます） |
| `q` / `Esc` | 終了 |

### 意味で検索

TUI の `?`（選んでいる日）と `list --ask`（期間内のすべての日）は同じしくみです。
ページごとに題・ホスト・URL を Jev に送って「*検索語* で探している人が求めているページか」を
はい/いいえで聞き、その確率の順に並べます。先に `/` で絞っておくと、送るページを減らせます。

1ページにつき1リクエストで、8本ずつ並べて投げます。TUI は答えが全部返るまで待ちます。
`--ask` では `--since` や `--day` で期間を絞ってください。1か月ぶんだと数千リクエストになることがあります。
プロバイダは `jev` の CLI と同じく環境変数で選びます。`JEV_PROVIDER` に `typesafe`（既定、
`TYPESAFE_API_KEY`）、`cloudflare`（`CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN`）、
`vercel`（`AI_GATEWAY_API_KEY`）のどれかを指定します。

注意: 意味で検索すると、そのページの題と URL がプロバイダに送られます。

## 設定

`~/.config/bhr/config.toml`（`$XDG_CONFIG_HOME` があればそちら）。初めて起動したときに既定の中身で作られます。

```toml
chrome_root = ""     # 空なら OS の既定（~/.config/google-chrome, ~/Library/Application Support/Google/Chrome）
profiles = []        # 空なら Default と "Profile N" をすべて
allow = []           # 空なら deny 以外はすべて拾う。書けば、載っているホストだけ拾う
deny = ["github.com", "x.com", "google.com"]   # 除外するホスト。サブドメインごと落とす
tracking_params = ["utm_*", "fbclid", "ref"]   # URL から落とすパラメータ。末尾 * は前方一致
```

設定に関係なく、次のものは常に除外します: localhost、IP アドレス、`.local` / `.test` /
`.internal` のホスト、認証の画面（`auth.`、`login.`、`accounts.` など）、トップページ（`/`）、
iframe の中で開かれたページ。

ヒント: `deny` だけで運用すると、仕事のドメインも入ってきます。読み物だけにしたいなら、
読むサイトを `allow` に並べてください。

## しくみ

- `History`（SQLite）は、一時フォルダに写してから開きます（Chrome が動いている間はロックされているため）。
- 日付はローカル時刻です。
- 同じ日の中では、ホストと題が同じページを1行にまとめ、いちばん短い URL を残します。
- Chrome はおよそ 90 日より古い履歴を消すので、残したいなら定期的に書き出してください。
