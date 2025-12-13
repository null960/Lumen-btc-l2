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

## ⚡ Quick Start (MVP)

### Prerequisites
* Rust (latest stable)
* Solana Tool Suite

### Running the Node
```bash
# Clone the repository
git clone [https://github.com/null960/Lumen-btc-l2.git](https://github.com/null960/Lumen-btc-l2.git)

# Run the node
cargo run -p node