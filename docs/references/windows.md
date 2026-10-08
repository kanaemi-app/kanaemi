# Windows（TSF）

Windows の入力方式を作るときに知っておく、Text Services Framework とシステムのふるまい。Windows 11（arm64）で確かめた。

## 作り

- TIP は COM の DLL。`ITfTextInputProcessorEx`、`ITfKeyEventSink`、`ITfCompositionSink` を実装し、`DllGetClassObject`・`DllRegisterServer`・`DllUnregisterServer`・`DllCanUnloadNow` を出す。
- 登録：HKLM の CLSID（`InprocServer32`、`ThreadingModel = Apartment`）、`ITfInputProcessorProfileMgr::RegisterProfile`（言語 0x0411）、カテゴリ `GUID_TFCAT_TIP_KEYBOARD`・`GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT`・`GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT`。
- DLL は Program Files に置く。AppContainer のプロセスから読めるのはここ。
- 未確定文字列：編集セッションの中で、初回は `InsertTextAtSelection(TF_IAS_QUERYONLY)` で得た範囲で `StartComposition` する。そのたびに composition の範囲に `SetText` し、キャレットを末尾に置く。確定は `EndComposition`。
- x64・arm64 に加え、32 ビットのアプリ向けに x86 の DLL が要る。32 ビットの DLL は、32 ビットの `regsvr32` で登録すると、レジストリの 32 ビット側（`WOW6432Node`）に入る。
- ARM64 の Windows で x64 のアプリが読み込むには、ARM64X か ARM64EC の DLL が要る。x64 の DLL を 64 ビット側に登録すると、ARM64 のアプリが読み込めなくなる。
- コードを持たず転送するだけの ARM64X の DLL を 64 ビット側に登録すると、ARM64 のアプリ（Notepad など）は ARM64 の DLL を、x64 のアプリは x64 の DLL を読み込み、どちらでも入力できる。`regsvr32` で登録すると、ARM64 の DLL の `DllRegisterServer` が呼ばれる。
- 転送用の DLL をリンクするには、転送先の DLL の名前で作った import library が要る。ないと転送する名前が解決できない（LNK2001）。`lib /def` は出力先に残った古い import library を読もうとして失敗する（LNK1136）。
- 読み込み中の DLL は削除できないが改名はできる。改名して新しい DLL を置けば、アプリを起動し直したときに読み込まれる。

## フォーカス

- 入力欄ごとの `ITfDocumentMgr` は context のスタックを持ち、入力を受けるのは一番上の context。スタックに積めるのは 2 つまで（[ITfDocumentMgr::Push](https://learn.microsoft.com/en-us/windows/win32/api/msctf/nf-msctf-itfdocumentmgr-push)）。
- 文書マネージャーの間のフォーカスの移りは `ITfThreadMgrEventSink::OnSetFocus` で、スタックへの積み下ろしは `OnPushContext`／`OnPopContext` で知らされる。アプリがフォーカスのある文書マネージャーに context を積むと、`OnSetFocus` が来ないまま一番上の context が変わる。
- `OnPopContext` が来たとき、その context がまだスタックに残っているかは確かめていない。

## アクティブ化と片付け

- IME を別の入力方式に切り替えると（`ITfInputProcessorProfileMgr::ActivateProfile`）、TSF はそのスレッドの TIP の `Deactivate` を呼ぶ。戻すと、新しい TIP を作って `ActivateEx` を呼ぶ。
- 窓を持つスレッドでは、アプリが `ITfThreadMgr::Deactivate` を呼んでも TIP は非アクティブにならない。窓を壊したとき（`DestroyWindow`）に、IMM の側から `Deactivate` が呼ばれる。
- アプリがプロセスで最後の `CoUninitialize` を呼ぶと、`DllCanUnloadNow` が `S_FALSE` を返していても、COM は DLL を外す（`CClassCache::CleanUpDllsForProcess` → `FreeLibrary`）。TIP がまだアクティブなら、外さない。
- DLL が外されるとき、Rust のスレッドローカル変数の片付けは `DLL_PROCESS_DETACH` の中、ローダーロックを握ったまま動く（`ucrtbase!execute_onexit_table` → `FlsFree` → `std` の片付け）。プロセスが終わるとき（`ExitProcess`）の `DLL_PROCESS_DETACH` では、片付けは動かなかった。
- DLL が起こしたスレッドが、DLL が外されたあとに目を覚ますと、外された DLL の中でアクセス違反になり、アプリが落ちる。`GetModuleHandleExW` に `GET_MODULE_HANDLE_EX_FLAG_PIN` を付けると、`CoUninitialize` のあとも DLL は外れない。

## キー

- Notepad は `OnTestKeyDown`／`OnTestKeyUp` を呼ばない。状態は `OnKeyDown`／`OnKeyUp` で更新し、`OnTestKey*` は食べるかどうかの予測を返すだけにする。
- 左右の Shift はどちらも `VK_SHIFT` で来る。lParam のスキャンコード（左 0x2a、右 0x36）で区別する。
- 単独押しは離した時点で判定し、キーを食べずに、同期・非同期のどちらでもよい編集セッション（`TF_ES_ASYNCDONTCARE | TF_ES_READWRITE`）を頼む。文字キーを食べるときは同期でよい。

## 表示

- 印を付けた未確定文字列は、Notepad とスタートメニューの検索欄でそのまま表示される。
- 候補ウィンドウの位置は、編集セッションの中で `GetActiveView` → `GetTextExt(composition の範囲)` で画面座標の矩形が取れる。レイアウトの前は `TS_E_NOLAYOUT` で失敗し、見えていないときは空の矩形が返る。
- 候補を `BeginUIElement` で渡し、その答えに従って自前の窓を出す。Notepad でも検索欄でも、自前の窓を出すよう答えが返る。`ITfIntegratableCandidateListUIElement` と `ITfFnSearchCandidateProvider` を実装すると、Windows が候補を検索欄の下に並べる（[IME の要件](https://learn.microsoft.com/en-us/windows/apps/develop/input/input-method-editor-requirements)）。
- スタートメニューの検索欄では、TIP は検索欄の `Windows.UI.Core.CoreWindow` と同じスレッドで動く。`ITfContextView::GetWnd` も `GetFocus` もこの窓を返さない。
- 持ち主のない窓は、検索欄（z-order の band 6）とは別の band に作られ、検索欄の下に隠れる。窓の band は作るときに決まり、あとで持ち主を変えても移らない。同じスレッドで見えている最上位ウィンドウ（CoreWindow）を持ち主にして作ると、band 6 に入り、検索欄の上に見える。

## AppContainer

- TIP は AppContainer のプロセス（スタートメニューの検索欄、msedgewebview2）にも読み込まれて動く。Program Files は読めるが、ユーザーのディレクトリや `C:\Users\Public` には書けない。
- Notepad（ストアアプリ）は AppContainer ではない。
- `icacls` で ALL APPLICATION PACKAGES（`S-1-15-2-1`）と ALL RESTRICTED APPLICATION PACKAGES（`S-1-15-2-2`）に読む権限を与えたファイルは、検索欄の TIP から読める。

### 名前付きパイプ

AppContainer の中の TIP から、ユーザーのセッションで動く別プロセスへのパイプに書き込めるかは、パイプのセキュリティ記述子で決まる。

| パイプの DACL（すべてに `S:(ML;;NW;;;LW)`）                  | Notepad | 検索欄 |
| ------------------------------------------------------------ | ------- | ------ |
| SYSTEM・Administrators・OWNER RIGHTS＋AC＋`S-1-15-2-2`       | 通る    | 拒否   |
| ユーザー本人の SID だけ                                      | 通る    | 拒否   |
| ユーザー本人の SID＋AC＋`S-1-15-2-2`                         | 通る    | 通る   |

- ユーザーとしての許可（本人の SID）と AppContainer としての許可（AC と `S-1-15-2-2`）の両方が要る。OWNER RIGHTS の行では通らない。それぞれが単独で必要かは確かめていない。
- `GW` はパイプのインスタンスを作る権限（`FILE_CREATE_PIPE_INSTANCE`）も与える（[Named Pipe Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)）。AppContainer には `FILE_WRITE_DATA`・`FILE_READ_ATTRIBUTES`・`SYNCHRONIZE`（`0x00100082`）だけを与え、クライアントもこの権限だけで開くと、検索欄から書き込める。ふつうの書き込み権限で開くと、インスタンスを作る権限も求めることになり、拒否される。
- パイプの名前は全セッションで共通なので、ユーザーの SID かセッション ID を名前に入れる。

## E2E テスト

- `ITfInputProcessorProfileMgr::ActivateProfile`（`TF_IPPMF_FORSESSION | TF_IPPMF_ENABLEPROFILE | TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE`）で、セッション全体を TIP に切り替えられる。
- キーは `SendInput`。右 Shift は `wVk = VK_RSHIFT, wScan = 0x36`。結果は Ctrl+A・Ctrl+C でクリップボードから読む。
- 対話的なデスクトップで動かす必要がある。SSH のセッションには画面がない。

## ビルド

- ARM64 の MSVC 14.44.35207（Visual Studio 2022 17.14）は、`/O2` で Luau（luau0-src 0.22.0+luau740）の `lua_newstate` を誤ってコンパイルする。一度も書いていないスタックの値をポインタとして読み、その先に 16 バイトの 0 を書く。新しい状態のページの一覧の一部が初期化されないまま残り、Luau の状態を閉じるときや、全体のメモリの回収（`luaM_getnextpage`）で落ちる。`/Od` と x64 では起きない。`/Os` か clang-cl でコンパイルすると起きない。
- そのため、このターゲットの C++ は `.cargo/config.toml` で clang-cl にしている。clang-cl は `/EHsc` がないと、Luau の `throw` を受け付けない。
- ARM64 では、ring が C を clang でコンパイルするので、LLVM の `bin`（`C:\Program Files\LLVM\bin`）が `PATH` に要る。clang-cl もそこにある。GitHub Actions の `windows-11-arm` のランナーには、はじめから入っている。
- Cargo は `.cargo/config.toml` を作業ディレクトリから探し、`--manifest-path` の先からは探さない。リポジトリの外で `--manifest-path` を付けてビルドすると、Luau は MSVC でコンパイルされる。
- Luau をコンパイルするビルドスクリプトは、コンパイラを変えても走り直さない。前のコンパイラで作った Luau が残っていれば、`cargo clean -p mlua-sys` で消す。
