# macOS（InputMethodKit）

macOS の入力方式を作るときに知っておく、InputMethodKit とシステムのふるまい。

## 作り

- `IMKInputController` のサブクラスは、`IMKServer` を作る前に登録しておく。Info.plist の `InputMethodServerControllerClass` の名前で探される。
- Info.plist に要るもの：`InputMethodConnectionName`、`InputMethodServerControllerClass`、`TISIntendedLanguage = ja`、`LSBackgroundOnly`、`tsInputMethodIconFileKey`（PNG、`TISIconIsTemplate = true`）、`tsInputMethodCharacterRepertoireKey`。
- `.app` をアドホック署名して `~/Library/Input Methods` に置く。動いているプロセスを止めると、システムが必要なときに起動し直す。
- 入力ソースを有効にするには、利用者がシステム設定で足す必要がある。`TISEnableInputSource` は成功を返すが、有効にはならない。
- `~/Library/Input Methods` に置いた直後は、システム設定の入力ソースの一覧に出ない。消した入力ソースは空の行として残る。システム設定を終了し、`getconf DARWIN_USER_CACHE_DIR` の下の `com.apple.IntlDataCache.le*` を消すと、開き直したときに一覧に出る。管理者の権限もログアウトも要らない。
- 同じバンドル ID の `.app` が `/Library/Input Methods` と `~/Library/Input Methods` の両方にあると、macOS は `~/Library` 側を動かす。
- インストーラーのパッケージは、Distribution の `domains` で `enable_currentUserHome` だけを許すと、ペイロードを利用者のホームの下に置く。管理者の権限は求めず、`postinstall` もその利用者として動く。

## キー

- `recognizedEvents` に `flagsChanged` を入れないと、修飾キーだけのイベントが来ない。入れると、未確定文字列の外をクリックしたときの IMK の既定の確定処理が働かなくなるので、`LeftMouseDown` も受け取り、マウスでの確定は IME が自分で行う。
- 左右の Shift はキーコード 56（左）・60（右）。押したか離したかはデバイス依存の修飾ビット（左 0x2、右 0x4）で分かるが、合成したイベントには付かないことがある。付いていなければ「その修飾キーのフラグがない＝離した、それ以外は前回から反転」とみなす。Ctrl・Cmd・Option の左右も同じ。
- アプリによっては、同じ `flagsChanged` が短い間に 2 度届く。直前と同じものは捨てる。
- Shift の単独押しで動くときも、Shift のキー自体は食べない。
- JIS の英数キーは 102、かなキーは 104、前方削除は 117。
- Ctrl を押していると `characters` は制御文字に、Option を押していると別の文字になる。どちらのときも、押したキーの文字は `charactersIgnoringModifiers` で取る。
- パスワードの入力欄では、OS が入力方式を切る。
- 合成したキーを `CGEventPost` で送るには、アクセシビリティの許可が要る。

## 候補パネル（IMKCandidates）

- `IMKCandidatesSendServerKeyEventFirst` を付けると、Space や数字をパネルより先に IME が受け取る。
- `updateCandidates` で読み直すとハイライトが先頭に戻る。一覧が変わったときだけ読み直す。
- `selectCandidateWithIdentifier:` ではハイライトが動かない。`moveDown:`／`moveUp:` で動かす。

## モードの表示

- 自前の `NSPanel`（枠なし、`NonactivatingPanel`、`NSPopUpMenuWindowLevel`）でカーソルの下に出せる。位置は `attributesForCharacterIndex:lineHeightRectangle:` で取る（原点は左下）。
- 入力ソースのアイコンが古いまま残ることがある。入力ソースを削除して足し直すと直る。
