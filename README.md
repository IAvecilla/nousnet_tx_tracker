# Psyche Transaction Tracker

Real-time transaction monitoring service for Psyche training runs on Solana.

## Features

- Real-time monitoring of all Psyche program transactions via WebSocket
- In-memory transaction store (keeps last 5000 transactions)
- Web UI with transaction list, filters, and statistics
- Decodes Anchor instruction types (coordinator, authorizer, treasurer, mining pool)
- Extracts run IDs and client pubkeys from transactions
- No database required - queries Solana RPC directly

## Tracked Programs

| Program | ID |
|---------|-----|
| Coordinator | `4SHugWqSXwKE5fqDchkJcPEqnoZE22VYKtSTVm7axbT7` |
| Authorizer | `PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw` |
| Treasurer | `EnU7DRx5az5YWxaxgqEGbXSYtudcfnjXewyBRRZCjJPw` |
| Mining Pool | `CQy5JKR2Lrm16pqSY5nkMaMYSazRk2aYx99pJDNGupR7` |

## Quick Start

```bash
# Enter development shell
nix develop

# Build
cargo build --release

# Run on devnet (real-time only)
./target/release/psyche-tx-tracker serve

# Run with recent transaction fetch
./target/release/psyche-tx-tracker serve --fetch-recent

# Open http://localhost:8765 in your browser
```

## CLI Options

```
psyche-tx-tracker serve [OPTIONS]

Options:
      --rpc <RPC>                    Solana RPC URL [default: https://api.devnet.solana.com]
      --ws-rpc <WS_RPC>              Solana WebSocket URL (derived from --rpc if not specified)
      --port <PORT>                  HTTP/WebSocket server port [default: 8765]
      --fetch-recent                 Fetch recent historical transactions on start
      --recent-limit <RECENT_LIMIT>  Number of recent transactions to fetch per program [default: 50]
      --static-dir <STATIC_DIR>      Path to static files directory (uses embedded files if not specified)
```

## API Endpoints

- `GET /api/transactions` - Query transactions with filters
  - Query params: `run_id`, `signer`, `instruction_type`, `program_name`, `min_time`, `max_time`, `limit`, `offset`
- `GET /api/stats` - Get transaction statistics
  - Query params: `run_id`
- `GET /api/health` - Health check
- `GET /ws` - WebSocket for real-time transaction updates

## Architecture

```
┌─────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│  Solana RPC     │────▶│  Rust Backend        │────▶│  In-Memory      │
│  (WebSocket)    │     │  - Log subscription  │     │  Store          │
└─────────────────┘     │  - Tx decode         │     │  (last 5000 tx) │
                        │  - HTTP server       │     └─────────────────┘
                        │  - WebSocket server  │◀───┐
                        └──────────────────────┘    │
                                 │                  │
                                 ▼                  │
                        ┌──────────────────────┐    │
                        │  Web UI (HTML/JS)    │────┘
                        │  - Real-time updates │
                        │  - Transaction list  │
                        │  - Filters & search  │
                        └──────────────────────┘
```

## Development

```bash
# Enter nix shell
nix develop

# Check
cargo check

# Run in development
cargo run -- serve --fetch-recent

# Build release
cargo build --release
```
