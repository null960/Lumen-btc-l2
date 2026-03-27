# ⬡ Lumen Network — Bitcoin L2

**Instant. Zero fees. Built on Bitcoin.**

Lumen is a Layer-2 network where 1 LSAT = 1 Bitcoin Satoshi.  
Send Bitcoin in milliseconds for free. Build games, apps, and payments on top.

[![Testnet Live](https://img.shields.io/badge/Testnet-LIVE-brightgreen)](http://194.15.112.56:3000)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange)](https://www.rust-lang.org/)
[![Bitcoin L2](https://img.shields.io/badge/Bitcoin-L2-f7931a)](https://bitcoin.org)

## What can you build?

| Use Case | Example |
|----------|---------|
| ☕ **Payments** | Pay for coffee — 5000 LSAT, 0 fee, instant |
| 🎮 **Games** | Buy GOLD tokens with LSAT, trade between players |
| 💸 **Tipping** | Donate to creators with a memo |
| 🏪 **Loyalty** | Issue shop points backed by Bitcoin |

## Live Testnet

- 🖥 Dashboard: http://194.15.112.56:3000
- 🔌 RPC: http://194.15.112.56:3000/api/state

## Quick Start (30 seconds)

```bash
# Get free testnet LSAT
curl -X POST http://194.15.112.56:3000/faucet \
  -H "Content-Type: application/json" \
  -d '{"address": "YOUR_BTC_ADDRESS"}'

# Check balance
curl http://194.15.112.56:3000/api/balance/YOUR_BTC_ADDRESS
```

## For Developers

→ [Full Developer Documentation](docs/DEVELOPERS.md)

Register your app in one command:
```
RegisterApp my-game GOLD 10 MyGame-Name
```

## Run Locally

```bash
git clone https://github.com/null960/Lumen-btc-l2
cd Lumen-btc-l2
cp .env.example .env        # add your OPERATOR_WIF
cargo run --bin node
# Dashboard → http://localhost:3000
```

## Tech Stack

Rust · Tokio · Axum · Redb · Bitcoin-rs · WASM · Solana VM

## Roadmap

| Phase | Status |
|-------|--------|
| Core Node + Bridge | ✅ Done |
| LSAT Token (1:1 BTC) | ✅ Done |
| App Tokens (games/shops) | ✅ Done |
| L1 Settlement (OP_RETURN) | ✅ Done |
| Security (env keys, challenge window) | ✅ Done |
| JS SDK | 🔄 In Progress |
| Fraud Proofs | 📋 Planned |
| Mainnet | 📋 Planned |

*Built with Rust. Secured by Bitcoin.*