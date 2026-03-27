# Lumen Network — Developer Documentation

> **Build payments, games, and apps on Bitcoin.**  
> 1 LSAT = 1 Bitcoin Satoshi. Instant. Zero fees. No trust required.

---

## Table of Contents

- [What is Lumen?](#what-is-lumen)
- [Quick Start](#quick-start)
- [Core Concepts](#core-concepts)
- [API Reference](#api-reference)
- [App Tokens Guide](#app-tokens-guide)
- [JavaScript SDK](#javascript-sdk)
- [Use Case Examples](#use-case-examples)
- [Security Model](#security-model)
- [Running a Node](#running-a-node)

---

## What is Lumen?

Lumen is a Bitcoin Layer-2 network. It lets you send Bitcoin instantly with zero fees.

**The problem it solves:**

Paying 5,000 satoshis for a coffee on Bitcoin L1 costs $2–15 in fees and takes 10 minutes.  
On Lumen it costs **0 fees** and takes **milliseconds**.

**How it works:**

```
Your Bitcoin wallet
       │
       │  deposit (Peg-In)
       ▼
  LUMEN NETWORK  ──→  pay coffee ☕ (0 fee, instant)
       │          ──→  buy game tokens 🎮 (0 fee, instant)
       │          ──→  send to friend 💸 (0 fee, instant)
       │
       │  withdraw (Peg-Out, 24h security window)
       ▼
Your Bitcoin wallet / Exchange
```

**LSAT Token:**
- Symbol: `LSAT`
- 1 LSAT = 1 Bitcoin Satoshi = 1/100,000,000 BTC
- Supply is 100% backed — minted only when BTC is deposited, burned when withdrawn
- No volatility, no speculation — it's just Bitcoin, but fast

---

## Quick Start

### 1. Get testnet LSAT

```bash
# Request free testnet LSAT from faucet
curl -X POST http://194.15.112.56:3000/faucet \
  -H "Content-Type: application/json" \
  -d '{"address": "YOUR_BITCOIN_ADDRESS"}'

# Response:
# {"status":"success","amount":10000,"msg":"LSAT queued for delivery"}
```

### 2. Check your balance

```bash
curl http://194.15.112.56:3000/api/balance/YOUR_ADDRESS

# Response:
# {
#   "address": "tb1q...",
#   "lsat": 10000,
#   "btc_equivalent": "0.00010000 BTC",
#   "app_tokens": {}
# }
```

### 3. Send LSAT

Connect UniSat wallet at **http://194.15.112.56:3000** and use the dashboard,  
or use the API directly (requires a signed message).

### 4. Check network state

```bash
curl http://194.15.112.56:3000/api/state
```

---

## Core Concepts

### Transactions are free and instant

Every transfer on Lumen costs **0 LSAT**. Transactions confirm in milliseconds.  
The only cost is when moving funds between L1 (Bitcoin) and L2 (Lumen) — standard Bitcoin miner fees apply there.

### Memos

Every transfer supports an optional memo field — useful for tracking payments:

```
Transfer 5000 tb1qcafe... coffee_americano_large
Transfer 1000 tb1qgame... item:sword_legendary
Transfer 500  tb1quser... tip:thank_you
```

### Peg-In (BTC → LSAT)

Send BTC to the operator address. The node detects the deposit and mints LSAT for you.

```
1. Send BTC to operator address (shown on dashboard)
2. Wait for 1 Bitcoin confirmation (~10 min)
3. LSAT appears in your Lumen balance
```

### Peg-Out (LSAT → BTC)

Withdraw LSAT back to Bitcoin L1. Has a 24-hour challenge window for security.

```
1. Submit Withdraw command
2. LSAT is locked immediately (deducted from balance)
3. 24-hour challenge window starts (fraud protection)
4. After 24h with no challenge → BTC sent to your address
```

Why 24 hours? If the operator tries to publish a fake state, anyone can submit a fraud proof during this window and stop the withdrawal. This protects users without requiring ZK proofs.

### State Root

Every 3 minutes, the node publishes a Merkle root of all LSAT balances to Bitcoin via `OP_RETURN`. This means your balance is permanently recorded on Bitcoin L1 and can be verified by anyone.

```bash
# Verify your balance with a Merkle proof
curl http://194.15.112.56:3000/api/proof/YOUR_ADDRESS
```

---

## API Reference

**Base URL:** `http://194.15.112.56:3000`

### Read Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/state` | Full network state |
| `GET` | `/api/balance/:address` | LSAT balance + app tokens for address |
| `GET` | `/api/proof/:address` | Merkle proof of balance |
| `GET` | `/api/apps` | All registered apps |
| `GET` | `/api/apps/:app_id` | Single app info |
| `GET` | `/api/settlements` | L1 settlement history |
| `GET` | `/api/withdrawals` | Pending PegOut requests |
| `GET` | `/api/bond` | Operator bond status |

### Write Endpoint

All write operations go through one endpoint:

```
POST /api/cmd
Content-Type: application/json

{
  "cmd": "Transfer 1000 tb1q...",
  "sig": "<UniSat signature of cmd string>",
  "pubkey": "<user's public key hex>"
}
```

### Commands

| Command | Example | Description |
|---------|---------|-------------|
| `Faucet` | `Faucet` | Get 10,000 testnet LSAT |
| `Transfer` | `Transfer 1000 tb1q... coffee` | Send LSAT, optional memo |
| `Withdraw` | `Withdraw 5000 tb1qmywallet...` | Withdraw to Bitcoin L1 |
| `RegisterApp` | `RegisterApp my-game GOLD 10 Dragons-Quest` | Register app/game |
| `BuyToken` | `BuyToken my-game 100` | Buy app tokens with LSAT |
| `TransferToken` | `TransferToken my-game tb1q... 50` | Transfer app tokens |

### Response Format

All API responses return JSON:

```json
{ "status": "ok", "msg": "Transaction queued" }
{ "status": "error", "msg": "Insufficient LSAT: have 500, need 1000" }
```

---

## App Tokens Guide

App tokens let you build your own economy on top of Bitcoin.

**Use cases:**
- 🎮 Game studio: players buy GOLD tokens with LSAT, spend GOLD in-game
- ☕ Coffee shop: customers buy COFFEE_POINTS, redeem for drinks
- 🎵 Creator: fans buy CREATOR_COIN, unlock exclusive content
- 🏋️ Gym: members buy GYM_PASS tokens, check in at the door

### Step 1 — Register your app

```bash
# Format: RegisterApp APP_ID TOKEN_NAME RATE_PER_LSAT APP_NAME [DESCRIPTION]
# RATE = how many tokens per 1 LSAT
# Example: 1 LSAT = 10 GOLD

curl -X POST http://194.15.112.56:3000/api/cmd \
  -H "Content-Type: application/json" \
  -d '{
    "cmd": "RegisterApp dragons-quest GOLD 10 Dragons-Quest Epic RPG on Bitcoin",
    "sig": "YOUR_SIGNATURE",
    "pubkey": "YOUR_PUBKEY"
  }'
```

### Step 2 — Users buy your tokens

```bash
# User spends 100 LSAT → receives 1000 GOLD (rate=10)
curl -X POST http://194.15.112.56:3000/api/cmd \
  -H "Content-Type: application/json" \
  -d '{
    "cmd": "BuyToken dragons-quest 100",
    "sig": "USER_SIGNATURE",
    "pubkey": "USER_PUBKEY"
  }'
```

### Step 3 — Check token balance

```bash
curl http://194.15.112.56:3000/api/balance/USER_ADDRESS

# Response includes app tokens:
# {
#   "lsat": 9900,
#   "app_tokens": {
#     "dragons-quest:GOLD": 1000
#   }
# }
```

### Step 4 — Users transfer tokens

```bash
# Trade tokens with another player
# Format: TransferToken APP_ID RECIPIENT AMOUNT
curl -X POST http://194.15.112.56:3000/api/cmd \
  -H "Content-Type: application/json" \
  -d '{
    "cmd": "TransferToken dragons-quest tb1qfriend... 50",
    "sig": "USER_SIGNATURE",
    "pubkey": "USER_PUBKEY"
  }'
```

### App economics

When users buy tokens, LSAT flows to the **app owner's address**. The app owner can then withdraw LSAT back to Bitcoin at any time.

```
User pays 100 LSAT
    └─→ 100 LSAT credited to app owner
    └─→ 1000 GOLD tokens minted for user
```

---

## JavaScript SDK

Install:
```bash
npm install lumen-btc-sdk
```

### Browser (with UniSat wallet)

```html
<script type="module">
import { LumenClient, connectUniSat, formatLsat } from 'lumen-btc-sdk'

// Connect wallet
const signer = await connectUniSat()
const lumen = new LumenClient({ node: 'http://194.15.112.56:3000' }, signer)

// Get balance
const balance = await lumen.getBalance(await signer.getAddress())
console.log(formatLsat(balance)) // "10,000 LSAT"

// Pay for coffee
await lumen.transfer({
  to: 'tb1qcafe...',
  amount: 5000,
  memo: 'americano_large'
})

// Buy game tokens
await lumen.appToken.buy({
  appId: 'dragons-quest',
  lsatAmount: 100  // spend 100 LSAT → get 1000 GOLD
})
</script>
```

### Node.js / Server

```javascript
import { LumenClient } from 'lumen-btc-sdk'

const lumen = new LumenClient({ node: 'http://194.15.112.56:3000' })

// Read-only — no signer needed
const state = await lumen.getState()
const balance = await lumen.getBalance('tb1q...')
const apps = await lumen.appToken.listApps()
```

### Full example: Game shop

```javascript
import { LumenClient, connectUniSat } from 'lumen-btc-sdk'

const GAME_ID = 'my-game'
const NODE = 'http://194.15.112.56:3000'

async function initGame() {
  const signer = await connectUniSat()
  const lumen = new LumenClient({ node: NODE }, signer)
  const address = await signer.getAddress()

  // Check player balances
  const summary = await lumen.getBalanceSummary(address)
  console.log(`LSAT: ${summary.lsat}`)
  console.log(`GOLD: ${summary.appTokens[`${GAME_ID}:GOLD`] ?? 0}`)

  return lumen
}

async function buyItems(lumen, lsatAmount) {
  return await lumen.appToken.buy({
    appId: GAME_ID,
    lsatAmount,
  })
}

async function tradeWithPlayer(lumen, toAddress, goldAmount) {
  return await lumen.appToken.transfer(GAME_ID, 'GOLD', toAddress, goldAmount)
}
```

---

## Use Case Examples

### ☕ Coffee Shop Integration

```html
<!-- Add to your website -->
<div id="lumen-pay"></div>
<script>
async function payForCoffee(itemName, priceLsat) {
  const signer = await connectUniSat()
  const lumen = new LumenClient({ node: NODE }, signer)

  const result = await lumen.transfer({
    to: 'SHOP_BITCOIN_ADDRESS',
    amount: priceLsat,
    memo: itemName
  })

  if (result.status === 'ok') {
    showSuccess(`Paid ${priceLsat} LSAT for ${itemName}!`)
  }
}
</script>
```

### 🎮 Game Integration (React)

```jsx
import { LumenClient, connectUniSat } from 'lumen-btc-sdk'
import { useState, useEffect } from 'react'

function GameWallet({ gameId }) {
  const [lumen, setLumen] = useState(null)
  const [gold, setGold] = useState(0)

  async function connect() {
    const signer = await connectUniSat()
    const client = new LumenClient({ node: NODE }, signer)
    setLumen(client)

    const addr = await signer.getAddress()
    const summary = await client.getBalanceSummary(addr)
    setGold(summary.appTokens[`${gameId}:GOLD`] ?? 0)
  }

  async function buyGold(amount) {
    await lumen.appToken.buy({ appId: gameId, lsatAmount: amount })
    // refresh balance...
  }

  return (
    <div>
      <p>GOLD: {gold}</p>
      <button onClick={connect}>Connect Wallet</button>
      <button onClick={() => buyGold(100)}>Buy 1000 GOLD (100 LSAT)</button>
    </div>
  )
}
```

### 💸 Donation Button

```javascript
async function donate(creatorAddress, amount, message) {
  const signer = await connectUniSat()
  const lumen = new LumenClient({ node: NODE }, signer)

  await lumen.transfer({
    to: creatorAddress,
    amount,
    memo: `tip:${message}`
  })
}
```

---

## Security Model

### How user funds are protected

**L2 Transfers** — fully non-custodial. Every transaction is signed with the user's Bitcoin private key (via UniSat). The node verifies the signature before executing. No one can transfer your LSAT without your signature.

**Peg-Out (withdrawal)** — protected by a 24-hour challenge window. If the operator tries to process a fraudulent withdrawal, any observer can submit a fraud proof during this window. The operator's bond is at stake.

**State Root** — every 3 minutes, the Merkle root of all balances is published to Bitcoin via `OP_RETURN`. This creates a permanent, tamper-evident record on the most secure blockchain in the world.

**Replay Attack Protection** — every signed command can only be executed once. The node tracks all used signatures to prevent replay attacks.

### What the operator can and cannot do

| Can do | Cannot do |
|--------|-----------|
| Process transactions in any order | Execute transactions without valid user signature |
| Delay withdrawals (up to 24h) | Steal user funds directly |
| Publish state roots | Falsify balances (verifiable by anyone) |
| Charge bridge fees | Stop users from withdrawing (challenge mechanism) |

### Verify your balance yourself

Anyone can verify the state is honest:

```bash
# 1. Get your Merkle proof
curl http://194.15.112.56:3000/api/proof/YOUR_ADDRESS

# 2. Get the latest state root from Bitcoin L1
# Look up the operator address on mempool.space
# Find the latest OP_RETURN transaction
# The first 32 bytes after OP_RETURN = state root

# 3. Verify the proof matches the root
# The proof path lets you reconstruct the root from your balance
# If it matches what's on Bitcoin → your balance is correct
```

---

## Running a Node

### Prerequisites

- Rust 1.70+
- A Bitcoin testnet wallet (for signing L1 transactions)

### Setup

```bash
# Clone the repo
git clone https://github.com/null960/Lumen-btc-l2
cd Lumen-btc-l2

# Set up environment (IMPORTANT — don't skip this!)
cp .env.example .env
# Edit .env and set OPERATOR_WIF=your_private_key_wif

# Build and run
cargo run --bin node
```

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `OPERATOR_WIF` | ✅ Yes | — | Operator private key in WIF format |
| `LUMEN_NETWORK` | No | `testnet` | `testnet` or `mainnet` |
| `LUMEN_PORT` | No | `3000` | RPC server port |
| `LUMEN_SYNC_INTERVAL` | No | `30` | Bitcoin sync interval (seconds) |
| `LUMEN_SETTLEMENT_INTERVAL` | No | `180` | L1 anchor interval (seconds) |

### Security Checklist

- [ ] `operator.json` is in `.gitignore` ✅
- [ ] `.env` is in `.gitignore` ✅
- [ ] `OPERATOR_WIF` is set via environment, not hardcoded
- [ ] VPS has firewall — only port 3000 exposed
- [ ] Operator address has enough BTC for fees (>0.001 BTC recommended)

### Deploy to VPS (Linux)

```bash
# 1. Copy files to server
scp -r . user@YOUR_VPS:~/lumen

# 2. On the server — create .env
echo "OPERATOR_WIF=your_wif_here" > .env

# 3. Build release binary
cargo build --release

# 4. Create systemd service
sudo nano /etc/systemd/system/lumen.service
```

```ini
[Unit]
Description=Lumen Bitcoin L2 Node
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/lumen
EnvironmentFile=/home/ubuntu/lumen/.env
ExecStart=/home/ubuntu/lumen/target/release/node
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
# 5. Start and enable
sudo systemctl enable lumen
sudo systemctl start lumen
sudo systemctl status lumen

# 6. View logs
sudo journalctl -u lumen -f
```

### CLI Reference

```bash
# Show node info
cargo run --bin cli -- info

# Check balance
cargo run --bin cli -- balance tb1q...

# Request testnet LSAT
cargo run --bin cli -- faucet tb1q...

# List all apps
cargo run --bin cli -- apps

# Show pending withdrawals
cargo run --bin cli -- withdrawals

# Get Merkle proof
cargo run --bin cli -- proof tb1q...

# Connect to a different node
LUMEN_RPC=http://194.15.112.56:3000 cargo run --bin cli -- info
```

---

## Get Involved

- **GitHub:** https://github.com/null960/Lumen-btc-l2
- **Public Testnet:** http://194.15.112.56:3000
- **Issues & Feature Requests:** GitHub Issues

### Building something on Lumen?

Open a PR or issue on GitHub — we'll feature your app on the dashboard and help with integration.

---

*Lumen Network — Bitcoin L2 for the real world.*  
*Built with Rust. Secured by Bitcoin.*