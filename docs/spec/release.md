# リリース

GitHub のリリースを公開すると、そのタグから OS ごとのインストーラーを作り、リリースに付ける（[OS ごとのインストーラーで配る](../adr/20261005-distribute-with-an-installer-for-each-os.md)）。インストーラーと一緒に、それらの目録 `index.json` も付ける。最新のリリースの目録は `https://github.com/kanaemi-app/kanaemi/releases/latest/download/index.json` で取れる。

## 署名

macOS のインストーラーに入れるアプリは、どのリリースも同じ自己署名の証明書で署名する。版を上げても、利用者が与えた入力監視の許可は残る。

## 目録

```json
{
  "format": 1,
  "version": "v0.1.0",
  "packages": [
    {
      "file": "Kanaemi-0.1.0.pkg",
      "os": "macos",
      "arch": "arm64",
      "format": "pkg",
      "size": 12345678,
      "sha256": "…"
    }
  ]
}
```

- `format`：目録の形の版。この形は `1`。形を変えたら上げる。
- `version`：リリースのタグ。
- `packages`：リリースに付けたインストーラーのすべて。OS、CPU、形式の順に並べる。
- `file`：リリースに付けたファイルの名前。`https://github.com/kanaemi-app/kanaemi/releases/download/<タグ>/<名前>` で取れる。
- `os`：`macos`・`windows`・`linux` のどれか。
- `arch`：インストーラーが動く CPU。`x64` か `arm64`。
- `format`：インストーラーの形式。ファイル名の拡張子（`pkg`・`msi`・`deb`・`rpm`）。
- `size`・`sha256`：ファイルのバイト数と、小文字の 16 進の SHA-256。
