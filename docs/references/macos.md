# macOS（InputMethodKit）

macOS の入力方式を作るときに知っておく、InputMethodKit とシステムのふるまい。

## 作り

- `IMKInputController` のサブクラスは、`IMKServer` を作る前に登録しておく。Info.plist の `InputMethodServerControllerClass` の名前で探される。
- Info.plist に要るもの：`InputMethodConnectionName`、`InputMethodServerControllerClass`、`TISIntendedLanguage = ja`、`LSBackgroundOnly`、`tsInputMethodIconFileKey`（PNG、`TISIconIsTemplate = true`）、`tsInputMethodCharacterRepertoireKey`。
- `.app` を署名して `~/Library/Input Methods` に置く。署名はアドホックでも、自己署名の証明書でもよい。動いているプロセスを止めると、システムが必要なときに起動し直す。
- 入力ソースを有効にするには、利用者がシステム設定で足す必要がある。`TISEnableInputSource` は成功を返すが、有効にはならない。
- `~/Library/Input Methods` に置いた直後は、システム設定の入力ソースの一覧に出ない。消した入力ソースは空の行として残る。システム設定を終了し、`getconf DARWIN_USER_CACHE_DIR` の下の `com.apple.IntlDataCache.le*` を消すと、開き直したときに一覧に出る。管理者の権限もログアウトも要らない。
- 同じバンドル ID の `.app` が `/Library/Input Methods` と `~/Library/Input Methods` の両方にあると、macOS は `~/Library` 側を動かす。
- インストーラーのパッケージは、Distribution の `domains` で `enable_currentUserHome` だけを許すと、ペイロードを利用者のホームの下に置く。管理者の権限は求めず、`postinstall` もその利用者として動く。

## キー

- `recognizedEvents` に `flagsChanged` を入れないと、修飾キーだけのイベントが来ない。入れると、未確定文字列の外をクリックしたときの IMK の既定の確定処理が働かなくなるので、`LeftMouseDown` も受け取り、マウスでの確定は IME が自分で行う。
- 左右の Shift はキーコード 56（左）・60（右）。押したか離したかはデバイス依存の修飾ビット（左 0x2、右 0x4）で分かるが、合成したイベントには付かないことがある。付いていなければ「その修飾キーのフラグがない＝離した、それ以外は前回から反転」とみなす。Ctrl・Cmd・Option の左右も同じ。
- アプリによっては、同じ `flagsChanged` が短い間に 2 度届く。直前と同じものは捨てる。
- アプリによっては（Ghostty）、IME が受け取ったと返したキーでも、押す前も押したあとも未確定文字列が空で、確定した文字もなければ、そのキーを自分で打つ。キーを受け取って働きを決めずにおくあいだは、未確定文字列を空にしない。
- Shift の単独押しで動くときも、Shift のキー自体は食べない。
- JIS の英数キーは 102、かなキーは 104、前方削除は 117。
- Ctrl を押していると `characters` は制御文字に、Option を押していると別の文字になる。どちらのときも、押したキーの文字は `charactersIgnoringModifiers` で取る。
- パスワードの入力欄では、OS が入力方式を切る。
- 合成したキーを `CGEventPost` で送るには、アクセシビリティの許可が要る。
- 送ったキーは IME にも戻ってくるが、`kCGEventSourceUserData` に付けた印は Input Method Kit を通ると消えている（`NSEvent` の `CGEvent` では 0）。イベントタップでは印が残っている。IME に戻ったキーは、送ったキーと時刻を覚えておいて見分ける。
- マウスのボタンは、`NSEvent` のグローバルモニタで、どのアプリで押されても知れる。キーと違い、許可は要らない。自分のプロセスのウィンドウ（候補パネル）で押されたものは届かない。
- `recognizedEvents` に `keyUp` を入れても、Input Method Kit は文字のキーを離したイベントを渡さない。離したことは、聞くだけのイベントタップで、起きた時刻とともに知る。イベントタップは Input Method Kit より先にキーを見ることがあり、逆のこともあるので、押したことが IME に届いてから順に渡す。イベントタップが使えないときや取りこぼしたときは、`CGEventSourceKeyState`（`kCGEventSourceStateHIDSystemState`）でキーボードの状態を見る。ただし、短い間隔で見ても、続けて離した 2 つのキーの順は分からない。
- イベントタップのイベントの時刻（`CGEventGetTimestamp`）は起動からの時間だが、単位が `mach_absolute_time` の目盛りかナノ秒かは確かめていない。Apple シリコンでは両者が大きく違うので、今の時刻に近い読み方を採る。`NSEvent` の時刻とは、`mach_absolute_time` を秒にしたものが一致する。
- 入力監視の許可がないプロセスには、`CGEventSourceKeyState` は文字を打つキー（Space を含む）をいつも離しているものとして返す。Backspace や修飾キーは許可がなくても読める。許可は `CGPreflightListenEventAccess` で確かめる。アドホック署名の入力方式からは、`CGRequestListenEventAccess`・`IOHIDRequestAccess`・聞くだけのイベントタップのどれでも、ふつうのアプリとして起動しても `LSUIElement` にしても、確かめる画面は出ず、入力監視の一覧にも載らない。利用者が一覧に `.app` を足してオンにする。アドホック署名の `.app` は、組み直すと許可が外れる。自己署名の証明書で署名すると、指定要件が証明書に結び付くので、組み直しても許可が残る（キーチェーンで信頼していない証明書でよい）。
- `CGEventSourceKeyState` に `kCGEventSourceStatePrivate` を渡すと戻ってこない。入力方式のメインスレッドが止まり、入力先のアプリも固まる。

## 候補パネル（IMKCandidates）

- `IMKCandidatesSendServerKeyEventFirst` を付けると、Space や数字をパネルより先に IME が受け取る。
- `updateCandidates` で読み直すとハイライトが先頭に戻る。一覧が変わったときだけ読み直す。
- `selectCandidateWithIdentifier:` ではハイライトが動かない。`moveDown:`／`moveUp:` で動かす。

## モードの表示

- 自前の `NSPanel`（枠なし、`NonactivatingPanel`、`NSPopUpMenuWindowLevel`）でカーソルの下に出せる。位置は `attributesForCharacterIndex:lineHeightRectangle:` で取る（原点は左下）。
- 入力ソースのアイコンが古いまま残ることがある。入力ソースを削除して足し直すと直る。
