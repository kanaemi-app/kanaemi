# リリース

GitHub のリリースを公開すると、そのタグから OS ごとのインストーラーを作り、リリースに付ける（[OS ごとのインストーラーで配る](../adr/20261005-distribute-with-an-installer-for-each-os.md)）。インストーラーと一緒に、それらの目録 `index.json` も付ける。最新のリリースの目録は `https://github.com/kanaemi-app/kanaemi/releases/latest/download/index.json` で取れる。

## 署名

版を上げても、利用者が IME に与えた OS の許可（キーの入力の監視など）は残る。

## 目録

```json
{
  "format": 2,
  "version": "v0.1.0",
  "packages": [
    {
      "file": "Kanaemi-0.1.0.pkg",
      "os": "macos",
      "arch": "arm64",
      "format": "pkg",
      "size": 12345678,
      "sha256": "…"
    },
    {
      "file": "fcitx5-kanaemi_0.1.0_amd64.deb",
      "os": "linux",
      "arch": "x64",
      "framework": "fcitx5",
      "format": "deb",
      "size": 12345678,
      "sha256": "…"
    }
  ]
}
```

- `format`：目録の形の版。この形は `2`。形を変えたら上げる。
- `version`：リリースのタグ。
- `packages`：リリースに付けたインストーラーのすべて。
- `file`：リリースに付けたファイルの名前。`https://github.com/kanaemi-app/kanaemi/releases/download/<タグ>/<名前>` で取れる。
- `os`：`macos`・`windows`・`linux` のどれか。
- `arch`：インストーラーが動く CPU。`x64` か `arm64`。
- `framework`：`linux` のインストーラーだけにある。どの入力の仕組みで動く入力方式か。`ibus` か `fcitx5`。
- `format`：インストーラーの形式。ファイル名の拡張子（`pkg`・`msi`・`deb`・`rpm`）。
- `size`・`sha256`：ファイルのバイト数と、小文字の 16 進の SHA-256。
