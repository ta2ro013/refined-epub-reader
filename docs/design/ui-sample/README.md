# UI サンプル — 採用済みデザイン資料

2026-10-08、ユーザーと Claude が作成したデザインを、本棚・読書画面・目次・表示設定の共通デザイン基準として採用した。対応する方針は [フロントエンドのデザイン方針](../../FRONTEND_DESIGN.md) を参照する。

資料内の「栞 / SHIORI」は今後変更予定の仮名で、正式なアプリ名ではない。サンプル HTML 内の表記は原本の記録として保持する。

## 資料と出典

| 保存した資料 | 対象 | 元の資料 |
| --- | --- | --- |
| [Main.dc.html](Main.dc.html) | 本棚・読書再開 | [claude-design/本一覧画面/Main.dc.html](../../../claude-design/本一覧画面/Main.dc.html) |
| [Reader.dc.html](Reader.dc.html) | 紙・生成・夜の読書画面、目次、表示設定 | [claude-design/読書画面-紙/Reader.dc.html](../../../claude-design/読書画面-紙/Reader.dc.html) |
| [ReaderNight.dc.html](ReaderNight.dc.html) | 夜テーマで目次・表示設定を開いた見本 | [claude-design/読書画面-夜/ReaderNight.dc.html](../../../claude-design/読書画面-夜/ReaderNight.dc.html) |

HTML は元の資料から変更せずコピーした。`support.js` と `vendor/react*.js` は資料を表示するためのランタイムで、アプリの本番コード・依存関係には取り込まない。夜テーマの資料は同じフォルダの `Reader.dc.html` を参照する。本棚と読書画面の相互参照も同じフォルダ内で解決できるよう配置した。

## 閲覧方法

リポジトリのルートで `python3 -m http.server 8080 --directory docs/design/ui-sample` を実行し、`http://localhost:8080/Main.dc.html`、`http://localhost:8080/Reader.dc.html`、`http://localhost:8080/ReaderNight.dc.html` を開く。書体は資料内の Google Fonts に接続できる場合に読み込まれ、接続できない場合は代替書体になる。

## 適用範囲

採用したのはデザインの方向性と画面構成。本棚管理、検索、メモ、読書時間、履歴保存、縦書き等の実装済みを意味しない。書籍名・冊数・章・進捗・引用などはサンプルデータ。実装では React コンポーネントと既存のスタイル構成に落とし込み、取得可能な実データへ接続する。

元の資料と ZIP はユーザーの原本として保持する。
