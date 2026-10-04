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

X11 の GTK3 の入力欄（`fcitx5-frontend-gtk3`）で確かめた。

- エンジンは `fcitx::InputMethodEngineV2` を継承した共有ライブラリ（アドオン）で、アドオンと入力メソッドの `.conf` で登録する。
- `keyEvent` に押す・離すの両方が届く（`isRelease()`）。左右の Shift は keysym で区別できる。
- 未確定文字列は、クライアントが対応していれば `InputPanel::setClientPreedit` のあと `InputContext::updatePreedit()` でアプリの中に出せる。印を付けたまま表示される。カーソルの位置は `InputContext::cursorRect()` で取れる。
- 既定の設定では、左 Shift の単独押しを Fcitx5 が取る。「入力メソッドを一時的に切り替える」キー（`Hotkey/AltTriggerKeys`）の既定値が左 Shift で、押すとキーボード配列に切り替わり、エンジンは非アクティブになる。`~/.config/fcitx5/config` の `[Hotkey/AltTriggerKeys]` を空にすると届く。

## 確かめていないこと

- KDE（KWin の Wayland）での Fcitx5。違いが出るとすれば、Wayland の入力メソッドのプロトコル（未確定文字列の扱い、カーソルの位置と候補の窓の置き方）。
- フォーカスが外れたとき、アプリが未確定文字列をそのまま確定してしまわないか（印や打ちかけが入らないか）。
