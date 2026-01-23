# Lumen-btc-l2: Sovereign Bitcoin L2 with Settlement Proofs 🟠

**Lumen-btc-l2** is a modular Layer-2 solution built in Rust designed to scale Bitcoin.

![Explorer Preview](assets/explorer_preview.png)

## 🚀 Key Features
* **Trustless Settlement:** Every few transactions, the node creates a cryptographic hash of the state (balances) and writes it to Bitcoin via OP_RETURN.
* **Active Two-Way Peg:**
Peg-In: Automatic detection of deposits on Testnet and minting of LBTC on L2.
Peg-Out: Real withdrawal of funds from L2 back to the user’s Bitcoin wallet.
* **Persistence Layer:** The integrated Redb database preserves all balances and transaction history even after a node restart.
* **Command Center UI:** A professional web dashboard for network monitoring and wallet management.

## 🏗 Architecture
* **L1 Monitoring:** The node polls the Bitcoin blockchain (Testnet). When a transfer to the operator’s address is detected, funds are credited to the 0xUser account.
* **L2 Execution:** Transactions (Transfers) are processed instantly in the node’s memory via the mempool.
* **L1 Settlement:** The L2 state is periodically “anchored” to L1, ensuring data immutability.
* **Peg-Out:** The Withdraw command burns L2 tokens and initiates the transfer of real BTC from the node’s wallet to the user’s address.

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
- [x] **Connect to Bitcoin Testnet3.**
- [x] **Implement Raw Transaction Signing (Key Management).**
- [ ] **Developer SDK:** Tools for deploying complex Anchor contracts.
- [ ] **Public Beta Launch.**

## ⚡ Quick Start

Follow these steps to run the full **Lumen L2** environment locally on your machine.

### Prerequisites
* **Rust & Cargo:** [Install Rust](https://www.rust-lang.org/tools/install)

### Step 1: Run the Node
Clone the repository and start the node:

```bash
cargo run -p node
```
On the first launch, the node will create a keypair.json file and output your OPERATOR WALLET ADDRESS.

### Step 2: Get Testnet BTC

1. Copy your address from the terminal.

2. Use any faucet (for example, [coinfaucet.eu](https://coinfaucet.eu/en/btc-testnet/)) to receive test coins.

3. As soon as the transaction appears on the network, the node will automatically detect it and credit your balance.

### Step 3: Access Command Center
Open your browser: http://localhost:3000/wallet

### Step 4: Interact
Enter commands in the web terminal:
* **Me** — check the current balance and status.
* **Transfer 500 0xBob** — instant transfer within L2.
* **Withdraw 1000 <your_address>** — withdraw real BTC back to your wallet.