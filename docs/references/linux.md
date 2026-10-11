# Linux（IBus・Fcitx5）

Linux の入力方式を作るときに知っておく、IBus と Fcitx5 のふるまい。

## IBus

X11 の GTK3・GTK4 と、GNOME Shell（Wayland）の GTK4 の入力欄で確かめた。Kanaemi のエンジンは Debian 13（IBus 1.5.32）で確かめた。

### 作り

- エンジンは IBus のコンポーネントの XML で登録する。GNOME では GNOME Shell が IBus を起動し、エンジンは IBus の設定ではなく GNOME の入力ソース（`org.gnome.desktop.input-sources` に `('ibus', '<エンジン名>')`）で選ぶ。
- エンジンは D-Bus のサービスとして書ける。IBus のバスで `org.freedesktop.IBus.Factory` の `CreateEngine` に答え、作ったエンジンのオブジェクトで `org.freedesktop.IBus.Engine` に答える。C のライブラリは要らない。
- テキストや候補の一覧は、型の名前と添付の辞書で始まる構造体で送る。テキストは `(sa{sv}sv)`、候補の一覧は `(sa{sv}uubbiavav)`、プロパティは `(sa{sv}suvsvbbuvv)`。この形で、IBus のパネルが番号付きの候補と、選んでいる候補を出す。
- D-Bus のメソッドは、zbus の既定では呼ばれるたびに別のタスクで動き、順番が保たれない。インターフェースに `spawn = false` を付けると、届いた順に 1 つずつ処理する。
- X11 の GTK3 のアプリでは、入力モジュール `ibus-gtk3` が要る。ないとキーはエンジンを通らない。

### キー

- `process_key_event` で、左右の Shift は keyval（`Shift_L`／`Shift_R`）でも keycode（42／54）でも区別できる。離したイベントは `state` に `RELEASE_MASK` が付く。
- 単独押しは離した時点で判定でき、キーを食べずに未確定文字列を出せる。
- 置き換えたキーは `ForwardKeyEvent` でアプリに届く。

### フォーカス

- IBus は、エンジンを 1 つだけ作り、フォーカスの移った入力欄に付け替えて使う（グローバルエンジン）。付け替えたあとで前の入力欄の `FocusOut` を送るので、それに答えて送った `CommitText` は次の入力欄に届く。
- エンジンの `FocusId` プロパティを `true` にすると、`FocusIn`／`FocusOut` の代わりに `FocusInId`／`FocusOutId` が来る。`FocusInId` の `client` は、GTK の入力モジュールなら `gtk4-im:プログラム名`（`gtk3-im:`・`gtk-im:` も）、XIM なら `xim`、GNOME Shell が受け持つ入力欄なら `gnome-shell` で、プログラム名は GTK の入力モジュールのときだけ分かる（[ibusengine.c](https://github.com/ibus/ibus/blob/main/src/ibusengine.c) の `focus-in-id` シグナルの説明）。

### 表示

- 未確定文字列は `update_preedit_text`、確定は `commit_text`。印を付けた未確定文字列はそのまま表示される。
- 未確定文字列に下線なし（`IBusAttribute` の下線を `NONE`）の属性を付けると、X11 の GTK4 は下線なしで出す。Wayland の GTK4 は、属性によらず下線を付けて出す。
- 候補の位置のためのカーソルの矩形は、`set_cursor_location` で届く。最後に届いた矩形を使う。Wayland では幅 0 の、キャレットの縦線としての矩形になる。
- GNOME（Wayland）では、候補の一覧は GNOME Shell が入力欄の下に出し、補助テキストはカーソルの下に小さな札として出て、`HideAuxiliaryText` で消える。
- IBus の GTK のパネル（ibus-ui-gtk3）は、補助テキストを出すと候補の一覧の枠（ページ送りの矢印）も出し、補助テキストを消しても、候補の一覧を一度出して消すまで枠を残す。

### 試す環境

- Xvfb（X11）の上の GTK4 の入力欄に、xdotool で打てる。
- Debian のクラウドイメージ（genericcloud）のカーネルには画面のドライバがなく、デスクトップを入れても画面に出ない。普通のカーネル（`linux-image-arm64`）なら出る。

## Fcitx5

X11 の GTK3 の入力欄（`fcitx5-frontend-gtk3`）で確かめた。Kanaemi のアドオンは Debian 13（Fcitx5 5.1.12）の、Xvfb（X11）の上の GTK4 の入力欄（`fcitx5-frontend-gtk4`）で確かめた。

### 作り

- エンジンは `fcitx::InputMethodEngineV2` を継承した共有ライブラリ（アドオン）で、アドオンと入力メソッドの `.conf` で登録する。`.conf` は XDG のデータのフォルダの `fcitx5/addon/`・`fcitx5/inputmethod/` から読まれ、`/usr/local/share` でもよい。
- ライブラリは、`.conf` の `Library` に `.so` を付けた名前で、Fcitx5 のアドオンのフォルダ（`pkg-config --variable=libdir Fcitx5Core` の下の `fcitx5`。環境変数 `FCITX_ADDON_DIRS` で変えられる）だけから探される。ほかの場所のライブラリへのシンボリックリンクでも読み込める。
- Fcitx5 はライブラリの `fcitx_addon_factory_instance` を探して呼ぶ。Rust の `cdylib` は Rust で定義した記号しか外に出さないので、C++ の `FCITX_ADDON_FACTORY` では見つからない。Rust の側で定義して、C++ のファクトリーを返す。
- アドオンは Fcitx5 のプロセスで、1 つのイベントループの上で呼ばれる。ほかのスレッドから処理を頼むときは `EventDispatcher` を使う。
- アドオンのフォルダは、Debian ではマルチアーキテクチャのライブラリのフォルダの下（`/usr/lib/aarch64-linux-gnu/fcitx5` など）、Fedora などの RPM のディストリビューションでは `/usr/lib64/fcitx5`。
- アドオンが Fcitx5 のライブラリをリンクしないと、記号は読み込んだ Fcitx5 のものに解決されて動くが、どの版の Fcitx5 に対してビルドしたかがライブラリに残らない。パッケージの依存（`dpkg-shlibdeps`、RPM の自動の依存）は、リンクしたライブラリから決まる。
- 入力コンテキストの `frontendName()` は 5.0.22 から。`frontend()` はそれより前からあり、同じ名前を返す。Ubuntu 22.04 の Fcitx5 は 5.0.14。

### キー

- `keyEvent` に押す・離すの両方が届く（`isRelease()`）。左右の Shift は keysym で区別できる。修飾キーの状態のビットは IBus と同じ（Shift `1 << 0`、Ctrl `1 << 2`、Alt `1 << 3`、Super `1 << 6` と `1 << 26`）。
- 既定の設定では、左 Shift の単独押しを Fcitx5 が取る。「入力メソッドを一時的に切り替える」キー（`Hotkey/AltTriggerKeys`）の既定値が左 Shift で、押すとキーボード配列に切り替わり、エンジンは非アクティブになる。`~/.config/fcitx5/config` の `[Hotkey/AltTriggerKeys]` を空にすると届く。

### フォーカス

- 入力欄ごとに入力コンテキストがあり、エンジンは 1 つのまま、どのコンテキストのイベントかを受け取る。コンテキストごとの状態は `InputContextProperty` に持ち、コンテキストが消えるとそのデストラクターが呼ばれる。
- フォーカスが外れると、そのコンテキストで `deactivate` が呼ばれる。そこで `commitString` した文字列は、フォーカスの外れた入力欄に届く。
- フォーカスが外れるとき、`deactivate` より前に、Fcitx5 かクライアント（GTK の入力モジュールは `ClientUnfocusCommit` を持つ）が、アプリの中の未確定文字列を確定する。`TextFormatFlag::DontCommit` を付けた部分は確定されない。
- 入力欄の種類は `capabilityFlags()` で分かる。GTK のパスワードと PIN の入力欄は `Password`、Wayland の機密のデータの入力欄は `Sensitive` になる。

### 表示

- 未確定文字列は、クライアントが対応していれば `InputPanel::setClientPreedit` のあと `InputContext::updatePreedit()` でアプリの中に出せる。印を付けたまま表示される。書式のない部分は、GTK4 では下線なしで出る。カーソルの位置は `InputContext::cursorRect()` で取れる。
- 候補の一覧は、Fcitx5 のパネル（classicui）が出す。候補に付けた `comment` は、候補の後ろに離して出る。候補の `setComment` は 5.1.9 から。
- パネルは、候補がなくても補助テキスト（`setAuxUp`）だけを小さな札として出し、補助テキストを消すと札も消える。

### Wayland

ヘッドレスの Sway（Debian 13、`WLR_BACKENDS=headless`）の上の GTK4 の入力欄で、`wtype` で打って確かめた。

- Sway のような入力メソッドのプロトコルの第 2 版を使うコンポジターでは、入力コンテキストの `frontendName()` は `wayland_v2`、KWin のような第 1 版では `wayland` になる。
- `wayland_v2` では、`forwardKey` は仮想キーボードを通して送られ、渡した修飾キーの状態は使われない（[waylandimserverv2.cpp](https://github.com/fcitx/fcitx5/blob/master/src/frontend/waylandim/waylandimserverv2.cpp) の `forwardKeyDelegate`）。上の環境では、送った Backspace は入力欄に効かなかった。`deleteSurroundingText` は効く。
- `wayland`・`wayland_v2` のどちらでも、入力欄を手放してから `deactivate` が呼ばれ、そのあとの `commitString` は届かない（[waylandimserver.cpp](https://github.com/fcitx/fcitx5/blob/master/src/frontend/waylandim/waylandimserver.cpp)、`waylandimserverv2.cpp` の `done`）。上の環境では、読みを打っている途中で同じ窓の別の入力欄にフォーカスを移すと、読みはどちらの入力欄にも入らなかった。
- どちらも `ClientUnfocusCommit` を持つので、フォーカスが外れたとき、Fcitx5 は未確定文字列を確定せずクライアントに任せる。上の環境の GTK4 は確定しなかった。

## 確かめていないこと

- KDE（KWin の Wayland）での Fcitx5。違いが出るとすれば、Wayland の入力メソッドのプロトコル（未確定文字列の扱い、カーソルの位置と候補の窓の置き方）。
- IBus で、フォーカスが外れたとき、アプリが未確定文字列をそのまま確定してしまわないか（印や打ちかけが入らないか）。
- Fcitx5 で、GTK 以外のクライアント（Qt、XIM）。
- Fcitx5 のアドオンの RPM のパッケージを、RPM のディストリビューションで入れること。
