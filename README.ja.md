# vimeflow-terminal

**[herdr](https://github.com/herdrdev/herdr) の、こだわりを持った下流フォークです。**

<p align="center">
  <a href="https://github.com/herdrdev/herdr">上流の herdr</a> ·
  <a href="https://herdr.dev/docs/">herdr のドキュメント</a> ·
  <a href="FORK.md">このフォークの変更点</a> ·
  <a href="#試す">試す</a>
</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · 日本語
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
</p>

---

## まず herdr を試す

**まだ [herdr](https://herdr.dev) を使ったことがなければ、そちらから始めて
ください。** herdr こそが本物です。ターミナルの中に住むエージェント
マルチプレクサで、すべてのエージェントの状態が一目でわかり、どこからでも
デタッチ・再アタッチでき、エージェント自身が操作できる純粋なソケット API を
持ち、キーボードもマウスも一級市民で、Rust の単一バイナリです。

```bash
curl -fsSL https://herdr.dev/install.sh | sh
```

herdr はインストールでき、ドキュメントがあり、定期的にリリースされ、サポート
されています。このフォークはまだそのどれでもありません。あなたが欲しいものは
ほぼすべて herdr が既に備えています。しかも上流が出荷しているぶん、より良く
できています。

herdr を使ったうえで、以下に説明する「こだわりの層」を明確に求める場合にだけ、
ここに戻ってきてください。

## このフォークとは

[Vimeflow](https://github.com/winoooops/vimeflow) はコーディングエージェント
向けの Electron デスクトップアプリでした。2026 年 8 月にターミナルネイティブへ
転換し、**herdr v0.8.0 のトラッキングフォーク**として作り直されました。

役割分担は意図的なものです:

- **herdr がエンジンを提供します** — PTY の所有、ベンダリングされた
  libghostty-vt による VT エミュレーション、workspace → tab → pane モデル、
  git とワークツリーの状態、エージェント検出、通知、プラグインシステム、
  ソケット API。
- **vimeflow はその上にこだわりの層を提供します** — エージェントの可観測性を
  任意のプラグインではなく組み込みの UI として持ち、複数のコーディング
  エージェントを同時に走らせる前提のワークフロー機能を揃えます。

一言で言えば **こだわりの herdr** です。上流は汎用のままで、このフォークが
あなたの代わりに選択をします。ここでの選択が誰にとっても正しいと分かったなら、
それはフォークではなく上流に入るべきものです。

これは*トラッキング*フォークであり、ハードフォークではありません。上流の
リリースはリリース単位で取り込み、上流ファイルへの編集はすべて記録し、目標は
マージ可能であり続けること、つまり離れていかないことです。

## 今あるもの

herdr v0.8.0 のすべてに加えて:

- **組み込みエージェントウォッチャー** — コーディングエージェントの可観測性
  （トランスクリプト監視、ライフサイクル、メトリクス、通知）がバイナリに
  組み込まれ、サーバーと一緒に動きます。インストール、リンク、同期が必要な
  プラグインはありません。CLI は `vimeflow watcher` 配下です。
- **自動 pane タイトル** — pane のラベルが、エージェントの作業中のセッション
  タイトルに追従します。手動での改名が常に優先され、上書きされることは
  ありません。
- **Agents サイドバーのカード** — サイドバーの Agents セクションが、トークン行
  のリストではなくエージェントカード（ライフサイクルに加えてコンテキスト、
  キャッシュ、コスト、モデル、ツール、トレース）を描画します。カードは
  エージェントウォッチャー自身が描くので、カードの見た目の実装は 2 つではなく
  1 つです。ライブで設定できます:

  ```toml
  [ui.sidebar]
  agents_view = "cards"    # herdr の行リストに戻すなら "legacy"
  agents_hide_idle = false
  ```

- **キーボードによるエージェントナビゲーション** — `prefix+a` でフォーカスが
  Agents サイドバーに移ります。ゾーンは 2 つ。`j`/`k` でカードを移動し、`l` で
  選択中カードのツール呼び出しトレースに降り、そこで `o` を押すと状態、所要
  時間、タイムスタンプ、保持された引数全文を示す詳細パネルが開きます。カード
  カーソルの移動は意図的に pane のフォーカスを*変えません*。Enter だけが確定
  するので、長いリストを歩いてもメインビューが振り回されることはありません。
  マウスも全面的に使えます。トレース行をクリックで選択、もう一度クリックで
  開きます。

- **タブアイランド** — タブバーはカプセルです。アクティブなタブはタイトルを
  載せたピル、他のタブはドット（または番号、またはラベル）で、切り替え時に
  スプリングアニメーションします。バックグラウンドのエージェントが完了したり
  ブロックされたりすると通知レコードが残り、カプセルにベルと未読数が表示され
  ます。`prefix+i`（またはベルのクリック）でパネルが開き、Enter でそのレコード
  の workspace、tab、pane にジャンプ、`r` で全件既読、`c` でクリアします。
  トーストはカプセルに固定表示されます。すべてライブで再読み込みされ、
  `ui.tab_bar_style = "classic"` で herdr のタブバーに戻せます。

  ```toml
  [ui.island]
  position = "center"     # または "left"
  display = "dots"        # または "numbers"、"labels"
  arrivals = "toast"      # "silent" にするとレコードを静かに蓄積
  bell = "🔔"             # "!" で ASCII。1〜2 セル幅のグリフなら何でも可
  ```

- **コンパクトレールのエージェントマーク** — サイドバーを細いレールに
  折りたたむと、各行にその pane で動いているエージェントを示す 2 セル幅の
  マークが付き、レールを一瞥するだけでどこに誰がいるか分かります。
  `compact_rail_numbers` と `compact_rail_leading` が各行の先頭に何を出すかを
  決め、`[ui.sidebar.compact_rail_marks]` でエージェントごとにマークを上書き
  できます:

  ```toml
  [ui.sidebar]
  compact_rail_leading = "agent"
  compact_rail_numbers = true          # agent モードでは workspace 行の番号を制御
  [ui.sidebar.compact_rail_marks]
  claude = "Cl"                        # 特定エージェントのマークを上書き
  ```

- **バイナリのセルフアップデートは無効** — バイナリのセルフアップデートと
  プロダクトのお知らせは意図的に無効化されています。このフォークが自分の上に
  純正の herdr をインストールすることは決してありません。herdr.dev からの
  エージェント検出マニフェストの更新は、リリースビルドの通常のセッションでは
  デフォルトで有効です。無効にするには、`config.toml` で
  `update.manifest_check = false` を設定してください。

エージェントウォッチャー、自動タイトル、カード、キーボードナビゲーションは
**Unix 専用**（macOS と Linux）です。コンパクトレールのエージェントマークと
タブアイランドはすべてのプラットフォームでビルドされます。それ以外について、
Windows は上流の機能セットをビルドします。

## これから

- **pane カードとワークツリーフロー** — エージェントのグリフ、状態、ワーク
  ツリーバッジを持つカード型の pane ヘッダー。そして「新しいワークツリーに
  新しいエージェント pane を作る」操作を 1 つのアクションにまとめ、新しい
  workspace を生やす代わりに現在の tab を分割します。
- **ナビゲーターのエージェント行** — 各 workspace/tab エントリの下に
  エージェントごとの状態チップと最新のタスクメッセージを表示。クリックでその
  pane にフォーカスします。
- **ローカル hunk ビュー** — プロセス内 git エンジンの上に載る TUI の diff
  pane。ワークツリー単位のファイル一覧、hunk 単位の描画とナビゲーション。まずは
  読み取り専用から。

一部はまだ保留です。ブランド名の変更は実行ファイルと設定・状態ディレクトリには
及んでいますが、`HERDR_*` 環境変数と `herdr.sock` / `herdr-client.sock` の
ファイル名には**及んでいません**。これらは連携プロトコルであり
（`herdr-agent-watcher` だけでも 9 個の変数を読みます）、改名するとユーザーに
見える利点なしにプラグインが壊れるからです。[`FORK.md`](FORK.md) の保留中の
ブランド変更範囲を参照してください。

## 試す

ビルド済みバイナリも、インストーラーも、リリースチャンネルもありません。ソース
からビルドします。

**前提条件。** Rust 1.96.1（`rust-toolchain.toml` に固定されているので `rustup`
が自動的に拾います）と、ベンダリングされた libghostty-vt をコンパイルするための
**Zig 0.15.2**。`PATH` 上に*より新しい* Zig があると失敗します。バージョンは
厳密です。Zig のキャッシュが空なら先に `scripts/preseed_zig_cache.sh` を実行
してください。さもないと依存 tarball の取得でビルドが止まります。

```bash
git clone https://github.com/winoooops/vimeflow-terminal
cd vimeflow-terminal
scripts/preseed_zig_cache.sh      # Zig キャッシュが空のときだけ必要
cargo build --release
./target/release/vimeflow
```

実行ファイルは `herdr` ではなく `vimeflow` です。必要なら `PATH` に置いて
ください:

```bash
ln -s "$PWD/target/release/vimeflow" ~/.local/bin/vimeflow
```

### herdr との共存

vimeflow は独自のディレクトリを使うので、インストール済みの herdr には触れ
ません:

| | vimeflow | 上流の herdr |
| --- | --- | --- |
| 実行ファイル | `vimeflow` | `herdr` |
| 設定 | `~/.config/vimeflow/` | `~/.config/herdr/` |
| 状態 | `~/.local/state/vimeflow/` | `~/.local/state/herdr/` |

vimeflow は**クリーンな設定**から始まります。インストール済みの herdr からは
何も引き継ぎません。設定を使い回したければ `~/.config/herdr/config.toml` を
手でコピーしてください。

両者は**同時に**動かせます。サーバーは自分のソケットパスを自分が起動した pane
にエクスポートするので、それぞれの子プロセスは自分を起動した側に戻ります。
`HERDR_*` の変数名と `herdr.sock` のファイル名はプラグインを動かし続けるために
意図的に共有しており、その帰結が 1 つあります。*herdr の pane の中から*
vimeflow を起動すると、その pane の `HERDR_SOCKET_PATH` を継承して herdr に
アタッチしてしまいます。セッションの外のターミナルから起動するか、変数を消して
ください:

```bash
env -u HERDR_ENV -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH vimeflow
```

どちらに繋がっているかは `vimeflow status server` で確認できます。ソケットパスが
その持ち主を示します。

本当に共有できないものが 1 つあります。**Claude メトリクスブリッジ**は
グローバルの `~/.claude/settings.json` にある単一のフックなので、所有できるのは
1 つのインストールだけです。最後に `watcher claude-bridge enable` を実行した側が
Claude のコンテキスト、キャッシュ、コストの数値を受け取り、もう一方ではそれらが
`—` と表示されます。

### 設定

```bash
vimeflow --default-config > ~/.config/vimeflow/config.toml
```

すべての設定が**コメントアウトされた状態**でデフォルト値とともに列挙されます。
変えたいものだけコメントを外してください。フォーク専用の設定にはその旨が記され、
ファイル末尾にまとめられています。

### 知っておきたい落とし穴

カードがすべて `— no telemetry` と表示されるなら、スタンドアロンのエージェント
ウォッチャー*プラグイン*が有効で、組み込みのものと衝突しています。プラグインは
サーバー起動時に自前のデーモンを起動し、それが組み込みウォッチャーに取って
代わります。組み込み側は終了し、設計上再起動しません。プラグインを無効にして
ください:

```bash
vimeflow plugin disable herdr-agent-watcher
vimeflow server stop && vimeflow      # 反映には再起動が必要
```

プラグイン自身の `open-sidebar` キーバインドはどちらの場合も動き続けます。
vimeflow がそれを組み込みの Agents サイドバーに振り向けます。

### アタッチせずにサーバーを動かす

TUI をアタッチせずにサーバーを起動するには、次を実行します:

```bash
vimeflow server
```

既存のサーバーを停止する必要がある場合にのみ、次を実行してください。
そのセッションのすべての pane が終了します:

```bash
vimeflow server stop
```

### テスト

```bash
just test     # ユニットテスト + メンテナンスチェック
just check    # フルゲート: lint、テスト、Windows ターゲットの clippy
```

`just` を使わない場合、フォークの CI コマンドは次のとおりです:

```bash
cargo nextest run --locked -E 'not binary(live_handoff)'   # macOS
cargo nextest run --locked                                  # Linux
```

`live_handoff` は上流に合わせて macOS では除外されます。実際のサーバーを起動する
連携テストバイナリは 1 つずつ実行されます（`.config/nextest.toml` を参照）。
そうしないとファイルディスクリプターとファイルシステム監視を奪い合ってタイム
アウトします。

## このフォークが herdr を追いかける方法

- `main` がプロダクトブランチで、Vimeflow の作業をすべて載せます。
- `master` は `upstream/master` の fast-forward 専用ミラーです。そこには何も
  コミットしません。
- 上流のリリースは、コミット単位ではなくリリース単位で、レビューブランチを
  通して `main` にマージされます。
- 変更した上流ファイルにはすべてファイル内の変更告知と [`FORK.md`](FORK.md) の
  登録簿の行があり、コメントを書けないファイルは [`MODIFICATIONS`](MODIFICATIONS)
  に列挙されています。

[`FORK.md`](FORK.md) が完全な記録です。フォークの基点コミット、パスごとの上流
編集登録簿、マージ手順、既知の基準テスト挙動が載っています。
[`AGENTS.md`](AGENTS.md) はこのリポジトリで作業する AI エージェント向けの
ガイドです。

## ライセンスと帰属

このフォークは herdr と同じ [Apache License 2.0](LICENSE) の下でライセンスされ、
上流の LICENSE をそのまま保持しています。

herdr の著作権は herdr のコントリビューターにあり、
[@ogulcancelik](https://github.com/ogulcancelik) が作成・保守しています。この
プロジェクトはその成果がオープンソースだからこそ存在しており、称賛と支援は上流に
向けられるべきです。**このフォークが役に立ったなら、このフォークではなく
[herdr をスポンサー](https://github.com/sponsors/ogulcancelik)してください。**

ベンダリングされた依存関係にはそれぞれのライセンスがあります。`libghostty-vt`
（MIT、© Mitchell Hashimoto）と `portable-pty`（MIT、© Wez Furlong）です。
ベンダリングされた `libghostty-vt/pkg` ツリーには他の条件のサードパーティ素材も
含まれており、このフォークを将来バイナリ配布する際には、それらを網羅した
ライセンス一覧を同梱する必要があります。

「herdr」は上流プロジェクトの名前で、ここではこのフォークの出自を説明するため
だけに使っています。このプロジェクトはその名前に対していかなる権利も主張しま
せん。
