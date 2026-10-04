# 設定のフォルダ

IME と設定アプリが読み書きするファイルを置くフォルダ。

| OS | 場所 |
| --- | --- |
| macOS | `~/Library/Application Support/kanaemi/` |
| Windows | `%APPDATA%\kanaemi\` |
| Linux | `$XDG_CONFIG_HOME/kanaemi/`（なければ `~/.config/kanaemi/`） |

## ファイル

| ファイル | 中身 |
| --- | --- |
| `config.toml` | [設定ファイル](settings.md) |
| `custom.tsv` | [ユーザーカスタム辞書](text-dictionary.md#ユーザーカスタム辞書) |
| `dictionaries/` | 辞書のファイル。中にフォルダを作ってもよい |
| `romaji/` | [ローマ字の表](romaji-table.md) のファイル |
| `ranking.model` | [並べ替えのモデル](ranking-model.md)（なくてもよい） |
| `selections.tsv` | [よく選ぶ候補](conversion.md#よく選ぶ候補) の記録（IME が作る） |

- IME は、設定のフォルダと `config.toml` がなければ作る。
- 利用者の打った語を含むファイル（`custom.tsv`・`selections.tsv`）は、IME と設定アプリが作るとき、持ち主だけが読み書きできるようにする。
- Windows では、書き込み用のプロセス（`kanaemi-server`）が、AppContainer のアプリに `config.toml`・`dictionaries/`・`romaji/`・`ranking.model` だけを読ませる（[Windows ではユーザーデータの書き込みを別プロセスに任せる](../adr/20261003-write-user-data-from-a-separate-process-on-windows.md)）。AppContainer のアプリの中の IME は、`custom.tsv` と `selections.tsv` を読みも書きもせず、辞書登録と削除の行を書き込み用のプロセスに送る。

IME のログは、設定のフォルダの外に書く。

| OS | 場所 |
| --- | --- |
| macOS | `~/Library/Logs/kanaemi.log` |
| Windows | `%LOCALAPPDATA%\kanaemi\kanaemi.log` |
| Linux | `$XDG_STATE_HOME/kanaemi/kanaemi.log`（なければ `~/.local/state/kanaemi/kanaemi.log`） |

## 読み直し

IME は、入力欄にフォーカスが入るとき、変わったファイルだけを読み直す。

- `config.toml` が変わっていれば、設定を読み直す。
- 使っている辞書のファイルや `custom.tsv` を足した・消した・書き換えたとき、辞書の一覧が変わったとき、`ranking.model` が変わったときは、それを反映する。打っている途中の確定履歴と、まだ書いていない記録は引き継ぐ。
- `selections.tsv` が変わったか消されたときは、読み直す。
