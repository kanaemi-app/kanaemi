# 設定のフォルダ

IME と設定アプリが読み書きするファイルを置くフォルダ。

| OS | 場所 |
| --- | --- |
| macOS | `~/Library/Application Support/kanaemi/` |
| Windows | `%APPDATA%\kanaemi\` |

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

IME のログは、設定のフォルダの外に書く。

| OS | 場所 |
| --- | --- |
| macOS | `~/Library/Logs/kanaemi.log` |
| Windows | `%LOCALAPPDATA%\kanaemi\kanaemi.log` |

## 読み直し

IME は、入力欄にフォーカスが入るとき、変わったファイルだけを読み直す。

- `config.toml` が変わっていれば、設定を読み直す。
- 使っている辞書のファイルや `custom.tsv` を足した・消した・書き換えたとき、辞書の一覧が変わったとき、`ranking.model` が変わったときは、それを反映する。打っている途中の確定履歴と、まだ書いていない記録は引き継ぐ。
- `selections.tsv` が変わったか消されたときは、読み直す。
