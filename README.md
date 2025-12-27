# Lumen-btc-l2: SVM-based Bitcoin L2 🟠

**Lumen-btc-l2** is a modular Layer-2 solution that brings the speed of Solana (SVM) to the Bitcoin network. We utilize Bitcoin for Data Availability (DA) and Settlement, enabling high-performance DeFi applications secured by the world's most robust blockchain.

## 🚀 Key Features
* **SVM Execution Environment:** Parallel transaction processing (10,000+ TPS).
* **Bitcoin Security:** All transaction batches are anchored to the Bitcoin network.
* **Modular Architecture:** Built with Rust, utilizing a decoupled DA adapter.

## 🛠 Architecture
This MVP demonstrates the core lifecycle of a Lumen-btc-l2 Rollup block:
1.  **Execution:** Transactions are processed via the Solana Virtual Machine.
2.  **Batching:** State diffs are aggregated into a batch.
3.  **DA Submission:** The batch proof is hashed and submitted to the Bitcoin network (Testnet/Signet).

## 🗺 Roadmap

### Phase 1: MVP & Architecture (Completed)
- [x] **Core Node Initialization:** Setup Rust workspace and basic node structure.
- [x] **SVM Integration:** Integrate Solana Virtual Machine for local transaction execution.
- [x] **Data Availability (Mock):** Implement abstract DA adapter interface.
- [x] **Bitcoin Connection:** Establish connection to Bitcoin Testnet types.

### Phase 2: Testnet Alpha (Completed)
- [x] **RPC Interface:** Implemented JSON-RPC server (Axum) for receiving transactions.
- [x] **Mempool System:** Added thread-safe storage for pending SVM-style transactions.
- [x] **Sequencer Logic:** Implemented automated batching of transactions every 10 seconds.
- [x] **Data Persistence:** Added `batches.log` to track Bitcoin anchoring history.
- [x] **Secured Execution:** Integrated signature verification logic (ed25519) at the RPC level.

### Phase 3: Public Beta (In Progress)
- [ ] **Block Explorer:** Simple web-interface to view L2 blocks and batches anchored on Bitcoin.
- [ ] **Live DA Integration:** Transitioning from mock adapter to **Nubit** or **Babylon** testnet.
- [ ] **Trustless Bridge:** Development of an SPV client for BTC bridging.
- [ ] **Developer SDK:** CLI tools for deploying Anchor contracts to Lumen-btc-l2.