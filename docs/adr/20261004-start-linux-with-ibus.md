# Linux は IBus から対応する

状態：採用

## 背景

Linux の入力の仕組みには、主に IBus と Fcitx5 がある。GNOME（Ubuntu・Fedora など）の標準は IBus で、Fcitx5 は KDE などで使われる。どちらでも、修飾キーの単独押しと、印を付けた未確定文字列が扱える（[Linux（IBus・Fcitx5）](../references/linux.md)）。

## 決定

Linux では、まず IBus のエンジンとして動かす。

## 検討した他の案

- Fcitx5 から始める：エンジンが C++ のアドオンになり、Rust の部分との間をつなぐ層が要る。既定の設定では、Fcitx5 が修飾キーの単独押しを自分の操作に使う。

## 結果

- GNOME の利用者は、入力ソースに Kanaemi を足せば使える。
- 候補の一覧やモードの示し方は、IBus が用意するものに従い、macOS や Windows とは揃わないことがある。
- Fcitx5 に対応するときは、別の OS ごとの入力方式として加える。
