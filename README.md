# Nousnet Transaction Tracker

Real-time transaction monitoring service for Psyche training runs on Solana.

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

# Run on devnet
cargo run -- serve

# Open http://localhost:8765 in your browser
```

## CLI Options

```
psyche-tx-tracker serve [OPTIONS]

Options:
      --rpc <RPC>                    Solana RPC URL [default: https://api.devnet.solana.com]
      --ws-rpc <WS_RPC>              Solana WebSocket URL (derived from --rpc if not specified)
      --port <PORT>                  HTTP/WebSocket server port [default: 8765]
      --skip-recent                  Skip fetching recent historical transactions on start
      --recent-limit <RECENT_LIMIT>  Number of recent transactions to fetch per program [default: 50]
```

Static files are served from the `static/` directory relative to the working directory.

## API Endpoints

- `GET /api/transactions` - Query transactions with filters
  - Query params: `run_id`, `signer`, `instruction_type`, `program_name`, `min_time`, `max_time`, `limit`, `offset`
- `GET /api/stats` - Get transaction statistics
  - Query params: `run_id`
- `GET /api/health` - Health check
- `GET /ws` - WebSocket for real-time transaction updates

## Development

```bash
# Enter nix shell
nix develop

# Check
cargo check

# Run in development
cargo run -- serve
```
