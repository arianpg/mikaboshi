# Mikaboshi

Mikaboshiは、ネットワークトラフィックをブラウザ上の3D空間でリアルタイムに可視化するアプリケーションです。
Rust製の高性能なエージェントとサーバー、そしてReact + Three.jsによるモダンなフロントエンドで構成されています。

## 構成

- **Mikaboshi-Agent**:
    - libpcapを使用してネットワークトラフィックをキャプチャします。
    - gRPCを使用してキャプチャしたデータをサーバーに送信します。
    - Rustで実装されており、LinuxおよびWindowsで動作します。
- **Mikaboshi-Server**:
    - エージェントからのデータを受け取り、接続されたWebクライアントにブロードキャストします。
    - Webアプリケーションの静的ファイルも配信します。
    - Rustで実装されています。
- **Mikaboshi-Web**:
    - トラフィックを3D可視化するWebフロントエンドです。
    - ReactとThree.jsを使用しています。

## 前提条件

ビルドには以下のツールが必要です：

- Docker
- Make

Windows環境でエージェントを実行する場合、[Npcap](https://npcap.com/)のインストールが必要です。

## ビルド

以下のコマンドで各コンポーネントをビルドできます。成果物は `build` ディレクトリに出力されます。

```bash
# 全てをビルド
make build-all

# エージェントのみビルド (Linux用)
make build-agent

# エージェントのみビルド (Windows用)
make build-agent-windows

# サーバーとWebフロントエンドをビルド
make build-server
```

## 実行方法

### 1. Mikaboshi-Server

サーバーを起動すると、WebサーバーとgRPCサーバーが立ち上がります。

```bash
./mikaboshi-server
```

**オプション:**

- `--http-port <u16>`: Webサーバーのポート (デフォルト: 8080)
- `--grpc-port <u16>`: gRPCサーバーのポート (デフォルト: 50051)
- `--peer-timeout <u64>`: 通信がないPeerを切断とみなすまでの秒数 (デフォルト: 30)

### 2. Mikaboshi-Agent

エージェントは管理者権限(root)で実行する必要があります。

```bash
sudo ./mikaboshi-agent --server localhost:50051 --device eth0
```

**オプション:**

- `--server <string>`: 接続先サーバーのアドレス (デフォルト: "localhost:50051")
- `--device <string>`: キャプチャ対象のデバイス名 (デフォルト: "any")
- `--list_devices`: 利用可能なデバイス一覧を表示して終了します
- `--promiscuous`: プロミスキャスモードを有効にします
- `--ipv6`: IPv6トラフィックもキャプチャ対象にします (デフォルトはIPv4のみ)
- `--mock`: 実際のトラフィックの代わりにモックデータを生成して送信します

### 3. ブラウザでアクセス

ブラウザで `http://localhost:8080` (または設定したポート) にアクセスしてください。

## 機能

- **リアルタイム可視化**: エージェント(球体)と通信相手(ディスク)のトラフィックを光るラインで表示します。
- **データ量表現**: トラフィックのサイズに応じてラインの色が変化します。
- **詳細情報**: PeerのIPアドレスや国情報(ipapi利用)を表示します。

## ライセンス

MIT License
