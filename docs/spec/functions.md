# 関数

[表記の置き場所](placeholder.md) を埋める関数。関数は変換元（文字列）と引数（文字列か、なし）を受け取り、置き場所を埋める文字列を返す。

IME は [数の書き方](numeric-conversion.md#数値の項目) の関数を、組み込みの関数として Luau で持つ。利用者も Luau で関数を足せる。

## 組み込みの関数

組み込みの関数は、`functions/` に置く利用者の関数と同じ形の Luau のファイル（関数どうしが共有するモジュールを含む）で書かれていて、IME に入っている。組み込みのファイルの `require` は、組み込みのファイルの中を見る。組み込みのファイルをまとめて `functions/` に置いても、同じに動く。

- `functions/` に同じ名前のファイルがあれば、そのファイルの関数を使う。

## 利用者の関数

利用者は、[設定のフォルダ](settings-folder.md) の `functions/` に [Luau](https://luau.org/) のファイルを置いて関数を足せる。`functions/` の直下にある `.luau` のファイルのうち、関数を返すもの 1 つが関数 1 つで、拡張子を除いたファイル名が関数の名前になる。

```luau
-- functions/dai.luau：数を位取りした漢数字にし、「第」を付ける（12 → 第十二）
return function(source, arg)
  local digits = kanaemi.number.digits(source)
  local kanji = digits and kanaemi.number.counted(digits, false)
  return kanji and ("第" .. kanji)
end
```

```luau
-- functions/date.luau：今日の日付を、引数の書式で書く
return function(source, arg)
  return os.date(arg or "%Y-%m-%d")
end
```

- 関数は変換元（文字列）と引数（文字列か `nil`）を受け取り、置き場所を埋める文字列を返す。`nil` を返すと、その項目から候補を作らない。
- IME が持つ関数と同じ名前のファイルがあれば、そのファイルの関数を使う。
- 関数を返さないファイルと、`functions/` の中のフォルダにあるファイルは、関数ではなくモジュール。
- Luau として読めないファイル、読むとエラーになるファイル、名前に使えない文字を含む名前のファイルは使わず、ログに残す。
- エラーになった関数、長く動き続けた関数、メモリを使いすぎた関数は、失敗として扱う。関数ごとに、最初の失敗をログに残す。長く動き続けた関数は、関数を読み直すまで呼ばない。
- `print` で出した文字は、関数の名前を添えて、IME のログに書く。一度にたくさん出した分は捨てる。

### 使えるもの

- Luau の標準ライブラリ。Luau には、ファイル・環境変数・ほかのプログラムに触れる関数はない。標準ライブラリは書き換えられない。
- `kanaemi` の関数（下）。`kanaemi` も書き換えられない。
- `require` は、`functions/` の中のモジュールを、`require` するファイルからの相対パスで読む（`require("./lib/util")`）。`functions/` の外は読めない。`functions/` の中に置いたシンボリックリンクは辿る。
- 関数どうしで値をやりとりするときは、同じモジュールを `require` し、そのテーブルを使う。グローバル変数に書いた値は、ほかの関数から見えるとは限らない。

```luau
-- functions/lib/counter.luau：関数が共有するモジュール
return { count = 0 }
```

```luau
-- functions/count.luau：呼ぶたびに 1 つ大きい数を書く
local counter = require("./lib/counter")
return function(source, arg)
  counter.count += 1
  return tostring(counter.count)
end
```

### kanaemi

関数を書くのに使える、IME が持つ関数。

数は、数字の値（0〜9）を前から並べたリスト（`{ 2, 0, 2, 6 }`）で扱う。Luau の数に収まらない大きな数も、そのまま扱える。

| 関数 | 返す値 |
| --- | --- |
| `kanaemi.number.digits(source)` | `source` の数字（半角と全角。混ざっていてもよい）の値のリスト。数字のほかの文字があるとき、空のときは `nil` |
| `kanaemi.number.counted(digits, daiji)` | 先頭の 0 を落とし、[位取り](numeric-conversion.md#位取り) した漢数字。`daiji` が真なら [大字](numeric-conversion.md#大字)。京の 1 万倍以上なら `nil` |
