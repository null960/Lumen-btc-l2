# Lumen-btc-l2: SVM-based Bitcoin L2 🟠

**Lumen-btc-l2** is a modular Layer-2 solution built in Rust designed to scale Bitcoin.

![Explorer Preview](assets/explorer_preview.png)

## 🚀 Key Features
* **Trustless SPV Bridge:** Deposits are validated cryptographically using Merkle Proofs against Bitcoin block headers, removing the need to trust the RPC blindly.
* **Persistent State:** Integrated Redb (embedded KV store) ensures balances and transaction history survive node restarts and crashes.
* **Two-Way Peg (1:1):** seamless conversion between Bitcoin (L1) and Lumen Bitcoin (LBTC). Includes Peg-In (Deposit) and Peg-Out (Withdrawal) mechanisms.
* **Interactive Web Terminal:** A built-in browser dashboard/CLI for real-time interaction, faucet access, and network monitoring.

## 🏗 Architecture
This MVP demonstrates the complete lifecycle of a trustless rollup transaction:
1.  **Peg-In (Bridge)::** he node monitors the Bitcoin network. When a deposit is detected, the SPV Client verifies the transaction's inclusion in a block header before minting LBTC.
2.  **Sequencer (Node):** Processes L2 commands (e.g., Transfer) received via the Web API, validates signatures, and updates the in-memory Global State.
3.  **Persistence Layer:** Every state change is atomically committed to the local redb database to ensure data integrity.
4.  **Peg-Out (Settlement):** Withdraw commands burn LBTC on L2 and trigger the node to send real BTC back to the user via the Bitcoin RPC interface.
5.  **Visualization:** The Web Dashboard polls the node state to display real-time balances and transaction logs.

## 🗺 Roadmap

### Phase 1: MVP & Core Structure (Completed) ✅
- [x] **Core Node Initialization:** Setup Rust workspace and basic node structure.
- [x] **SVM Integration:** Integrate Solana Virtual Machine for local transaction execution.
- [x] **Data Availability (Mock):** Implement abstract DA adapter interface.
- [x] **Bitcoin Connection:** Establish connection to Bitcoin Testnet types.

### Phase 2: Network & Security (Completed) ✅
- [x] **JSON-RPC Server:** Axum-based API for receiving transactions.
- [x] **Mempool System:** Thread-safe storage for pending transactions.
- [x] **Sequencer Logic:** Automated batching of transactions (10s interval).
- [x] **Cryptography:** Signature verification using Solana SDK (ed25519).

### Phase 3: Live Integration (Completed) ✅
- [x] **Nubit DA:** Integration with live Data Availability layer (simulated/testnet).
- [x] **Lumen-CLI:** Client tool for wallet creation (`create-wallet`) and transfers.
- [x] **Logging:** Structured logs (`batches.log`) tracking all Bitcoin anchors.

### Phase 4: Smart Contracts & State (Completed) ✅
- [x] **Global State:** Implementation of in-memory state management (`AppState`).
- [x] **Smart Contracts:** Logic for stateful instructions (`Increment` command).
- [x] **Live Explorer:** Web-UI connected to real-time node state and DA history.

### Phase 5: Persistence & Future (Completed) ✅
- [x] **Data Persistence:** Integration of RocksDB/Sled to save state across restarts.
- [x] **Trustless Bridge:** Development of an SPV client for BTC bridging.

### Phase 6: Public Testnet (In progress) 🚧
- [ ] **Developer SDK:** Tools for deploying complex Anchor contracts.
- [ ] **Connect to Bitcoin Testnet4/Signet.**
- [ ] **Implement Raw Transaction Signing (Key Management).**
- [ ] **Public Beta Launch.**

## ⚡ Quick Start

Follow these steps to run the full **Lumen L2** environment locally on your machine.

### Prerequisites
1.  **Rust & Cargo:** [Install Rust](https://www.rust-lang.org/tools/install)
2.  **Bitcoin Core:** [Download Bitcoin Core](https://bitcoincore.org/en/download/) (Add `bitcoind` and `bitcoin-cli` to your system PATH).

### Step 1: Start Local Bitcoin Network (Regtest)
Open a terminal (PowerShell or Bash) and start a local Bitcoin node. We use hardcoded credentials (`user`/`password`) for this testnet demo.

```bash
bitcoind -regtest -server -printtoconsole -rpcuser=user -rpcpassword=password -fallbackfee=0.0001
```
**Keep this terminal open!** This is your Layer 1 blockchain.

### Step 2: "Mine" Initial Bitcoin
Open a new terminal. We need to create a wallet and mine 101 blocks so the coinbase coins become spendable (this gives the network liquidity for the Faucet).

1. Create a wallet named **miner:**

```bash
bitcoin-cli -regtest -rpcuser=user -rpcpassword=password createwallet "miner"
```

2. Generate an address:

```bash
bitcoin-cli -regtest -rpcuser=user -rpcpassword=password getnewaddress
```
(Copy the address output, let's assume it is <MINER_ADDR>)

3. Mine 101 blocks to that address:

```bash
bitcoin-cli -regtest -rpcuser=user -rpcpassword=password generatetoaddress 101 <MINER_ADDR>
```

### Step 3: Run Lumen L2 Node
In the project root directory, launch the node:

```bash
cargo run -p node
```

You should see:
🧠 VM Ready. 
🌍 Web Dashboard running at: http://localhost:3000/wallet

### Step 4: Interact via Web Terminal
Open your browser and go to http://localhost:3000/wallet.
You will see a command-line interface. Follow this test scenario:

1. Get Test BTC (L1)
First, you need real Bitcoin on your Regtest address to perform a deposit. Type in the web terminal:

```bash
Faucet <your_btc_address_from_step_2_or_new_one>
```
Look at the node terminal (console). You should see: > 🚰 FAUCET: Sent 0.1 BTC to ...

2. Deposit to L2 (Bridge)
Currently, deposits are automated in the background for simplicity in this Alpha. Send funds manually via CLI if you want to test the SPV bridge log:

```bash
bitcoin-cli -regtest -rpcuser=user -rpcpassword=password sendtoaddress <YOUR_BTC_ADDRESS> 1.0
bitcoin-cli -regtest -rpcuser=user -rpcpassword=password -generate 1
```
Watch the node logs for: ✅ LBTC DEPOSIT (SPV Verified)

3. Transfer LBTC (L2)
Send Lumen Bitcoin instantly to another user inside L2:

```bash
Transfer 5000 0xBob
```

4. Withdraw to Bitcoin (Peg-Out)
Burn your LBTC and receive real BTC back to your L1 wallet:

```bash 
Withdraw 100000 <your_btc_address>
```