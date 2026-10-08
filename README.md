<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/kanaemi-app/kanaemi-brand/main/logo/kanaemi-logo-vertical-dark.svg">
    <img alt="kanaemi かなえみ" src="https://raw.githubusercontent.com/kanaemi-app/kanaemi-brand/main/logo/kanaemi-logo-vertical.svg" width="200">
  </picture>
</p>

<p align="center">
  どの OS でも同じ打ち方の日本語入力
</p>

<p align="center">
  <a href="https://kanaemi-app.github.io/">サイト</a> ·
  <a href="https://kanaemi-app.github.io/docs/install/">インストール</a> ·
  <a href="https://kanaemi-app.github.io/docs/">ドキュメント</a> ·
  <a href="https://github.com/kanaemi-app/kanaemi-dict">辞書</a>
</p>

Kanaemi（かなえみ）は、SKK の打ち方をもとにした日本語入力です。macOS・Windows・Linux で、キーの意味も変換の結果も変わりません。

- **どの OS でも同じ** ：キーの意味も変換の結果も OS によらず同じです。設定のフォルダを別のマシンに持っていけば、そのまま同じ打ち方になります。
- **頼んだときだけ変換する** ：ライブ変換も予測候補もありません。変換するのは `;` で読みを示したときだけです。
- **流れを止めずに辞書を育てる** ：候補になければ、その場で語を登録して確定まで進めます。
- **入力を外に出さない** ：変換は手元で完結し、入力を外へ送りません。
- **止まらない** ：内部で何が起きても、押したキーはアプリに届け、文字を打てる状態を保ちます。

## 打ち方

覚えるキーは Shift と `;` だけです。読みを打つ、候補を選ぶ、語を登録するといった状態には、モードを切り替えずに、打つキーに応じて入ったり抜けたりします。

### 左右の Shift でモードを選ぶ

Shift だけをポンと押して離すと、左は ABC モード、右はかなモードになります。今どちらのモードかを気にせず、打ちたいモードの側を押せば、そのモードに入れます。

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://kanaemi-app.github.io/guide/shift-dark.png">
  <img alt="左の Shift で ABC モード、右の Shift でかなモード。同じ nihongo でも、ABC モードでは nihongo、かなモードでは にほんご になる" src="https://kanaemi-app.github.io/guide/shift.png">
</picture>

### 漢字にしたい語だけ `;` から打つ

ふだんは打ったかながそのまま確定します。`;kanji` と打てば読み `›かんじ`、Space で `»漢字`、Enter で確定します。

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://kanaemi-app.github.io/guide/convert-dark.png">
  <img alt=";kanji で読み ›かんじ、Space で »漢字、Enter で確定して 漢字" src="https://kanaemi-app.github.io/guide/convert.png">
</picture>

### `;` のはたらき

`;` の意味は、いまの場面で変わります。読みの途中の `;` は送り仮名の始まりで、`;ka;ku` と打つと、送り仮名を打ったそばで `»書く` に変換します。

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://kanaemi-app.github.io/guide/semicolon-dark.png">
  <img alt="何も打っていないときは読みを始め、読みの途中では送り仮名の始まり、候補を選んでいるときは確定して次の読みへ、読みを始めたすぐあとなら ; の文字を打つ" src="https://kanaemi-app.github.io/guide/semicolon.png">
</picture>

### SandS の指づかいでも打てる

SKK で SandS を使ってきたなら、`;` の代わりに Space を押さえたまま打ちはじめられます。設定アプリのキーバインドで、「読みを始める」に「Space を押さえたまま」を足します。

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://kanaemi-app.github.io/guide/sands-dark.png">
  <img alt="Space を押さえたまま kanji で ›かんじ、ka のあと Space を押さえたまま ku で »書く。Space を単独で押せば、いつもどおり変換や空白になる" src="https://kanaemi-app.github.io/guide/sands.png">
</picture>

くわしくは [打ち方のドキュメント](https://kanaemi-app.github.io/docs/) を、手元に置ける一覧は [つかいかたの画像](https://kanaemi-app.github.io/guide/cheatsheet.png) を見てください。

## インストール

OS ごとのふつうの入れ方（インストーラーやパッケージ）で、[リリース](https://github.com/kanaemi-app/kanaemi/releases) から配っています。入れたあと、入力ソースに Kanaemi を足してください。手順は [インストールのドキュメント](https://kanaemi-app.github.io/docs/install/) にあります。

macOS では [Homebrew](https://brew.sh/) からも入れられます。

```sh
brew install --cask kanaemi-app/tap/kanaemi
```

インストーラーで入れた Kanaemi がすでにあると、Homebrew は上書きせずに止まります。そのときは `--force` を付けてください。置き換わるのはアプリだけで、辞書と設定はそのまま残ります。

辞書は Kanaemi の本体とは別に、[kanaemi-dict](https://github.com/kanaemi-app/kanaemi-dict) が配っています。設定アプリの「辞書」の「公式の辞書」から、基本辞書と分野ごとの追加辞書を入れられます。

## 開発

道具は [Nix](https://nixos.org/) の開発環境にそろっています。`nix develop`（または direnv）に入ってから、[just](https://github.com/casey/just) のレシピを使います。`just` だけを流すと、レシピの一覧が出ます。

```sh
nix develop
just ci              # CI と同じ確かめ（整形・lint・テスト）
just accuracy        # 変換の正解率を測る（KANAEMI_ACCURACY_DICTIONARIES に辞書のフォルダを渡すと、その辞書でも）
just bench           # ベンチマークを流す（CI はビルドだけ確かめる）
just install-macos   # macOS の入力方式を ~/Library/Input Methods に入れる
just install-ibus    # Linux（IBus）の入力方式を入れる
just install-windows # Windows の入力方式を入れる（管理者のシェルで）
```

| 場所 | 中身 |
| --- | --- |
| `crates/core` | 入力方式の状態機械。キーを受けて、表示するものと確定するものを返す |
| `crates/engine` | 変換。辞書を引き、候補を並べる |
| `crates/config` | 設定ファイルを読む |
| `crates/runtime` | どの OS の入力方式にも共通する、辞書・モデル・設定・ログ・入力欄ごとの状態 |
| `apps/macos` | macOS の入力方式（Input Method Kit） |
| `apps/windows` | Windows の入力方式（Text Services Framework） |
| `apps/ibus` | Linux の入力方式（IBus） |
| `apps/settings` | 設定アプリ |

- [docs/concept.md](docs/concept.md)：考え方
- [docs/adr/](docs/adr/)：設計判断の記録
- [docs/spec/](docs/spec/)：振る舞いの約束

## ライセンス

Kanaemi のソフトウェアは [MIT ライセンス](LICENSE) です。Kanaemi の名前とロゴは、[商標ポリシー](https://github.com/kanaemi-app/kanaemi-brand/blob/main/TRADEMARKS.ja.md) に従って使えます。ロゴは [kanaemi-brand](https://github.com/kanaemi-app/kanaemi-brand) にあります。
