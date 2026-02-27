# Lumen-btc-l2: Sovereign Bitcoin L2 with Settlement Proofs 🟠

**Lumen-btc-l2** is a modular Layer-2 solution built in Rust designed to scale Bitcoin.

## 🚀 Key Features
* **Trustless Settlement:** Every few transactions, the node creates a cryptographic hash of the state (balances) and writes it to Bitcoin via OP_RETURN.
* **Active Two-Way Peg:**
  * Peg-In: Automatic detection of deposits on Testnet and minting of LBTC on L2.
  * Peg-Out: Real withdrawal of funds from L2 back to the user’s Bitcoin wallet.
* **Persistence Layer:** The integrated Redb database preserves all balances and transaction history even after a node restart.
* **Command Center UI:** A professional web dashboard for network monitoring and wallet management.

## 🏗 Architecture
* **L1 Monitoring:** The node polls the Bitcoin blockchain (Testnet). When a transfer to the operator’s address is detected, funds are credited to the 0xUser account.
* **L2 Execution:** Transactions (Transfers) are processed instantly in the node’s memory via the mempool.
* **L1 Settlement:** The L2 state is periodically “anchored” to L1, ensuring data immutability.
* **Peg-Out:** The Withdraw command burns L2 tokens and initiates the transfer of real BTC from the node’s wallet to the user’s address.

## 🗺 Roadmap

### Phase 1-5: Core & Security (Completed) ✅
- [x] SVM Integration & JSON-RPC Server.
- [x] Global State & Redb Persistence.
- [x] Trustless Bridge (SPV client).
- [x] Live Explorer: Web-UI connected to real-time node state and DA history.

### Phase 6: Public Testnet (In progress) 🚧
- [x] Connect to Bitcoin Testnet3.
- [x] Implement Raw Transaction Signing (Key Management).
- [x] Cloud Deployment (Node is live on VPS).
- [ ] Developer SDK: Tools for deploying complex Anchor contracts.
- [ ] Public Beta Launch.

## ⚡ Quick Start

### Public Cloud Node
The Lumen L2 node is currently live and synchronized.
* **Block Explorer:** [http://194.15.112.56:3000](http://194.15.112.56:3000)
* **RPC Endpoint:** `http://194.15.112.56:3000`

### Run CLI Client Locally
Connect to the live network using the Lumen CLI:

```bash
# Set environment
$env:LUMEN_RPC="[http://194.15.112.56:3000](http://194.15.112.56:3000)"

# Request test tokens
cargo run --bin cli -- faucet <YOUR_ADDRESS>

# Check balance
cargo run --bin cli -- balance <YOUR_ADDRESS>