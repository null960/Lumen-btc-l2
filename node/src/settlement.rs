use bitcoin::hashes::{sha256, Hash, HashEngine};
use std::collections::HashMap;
use crate::wallet::LocalWallet;
use crate::btc_api::Utxo;
use bitcoin::{Transaction, TxIn, TxOut, OutPoint, ScriptBuf, Sequence, Witness, PrivateKey};
use bitcoin::blockdata::script::Builder;
use bitcoin::opcodes;
use bitcoin::sighash::{SighashCache, EcdsaSighashType};
use bitcoin::secp256k1::{Secp256k1, Message};
use bitcoin::script::PushBytesBuf;
use std::str::FromStr;

fn hash_leaf(address: &str, balance: u64) -> [u8; 32] {
    let mut engine = sha256::Hash::engine();
    engine.input(address.as_bytes());
    engine.input(&balance.to_le_bytes());
    sha256::Hash::from_engine(engine).to_byte_array()
}

fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut engine = sha256::Hash::engine();
    engine.input(left);
    engine.input(right);
    sha256::Hash::from_engine(engine).to_byte_array()
}

pub fn build_merkle_root(balances: &HashMap<String, u64>) -> String {
    if balances.is_empty() {
        return sha256::Hash::hash(b"empty").to_string();
    }

    let mut keys: Vec<_> = balances.keys().collect();
    keys.sort();

    let mut leaves: Vec<[u8; 32]> = keys.into_iter().map(|k| {
        hash_leaf(k, *balances.get(k).unwrap())
    }).collect();

    while leaves.len() > 1 {
        let mut next_level = Vec::new();
        for chunk in leaves.chunks(2) {
            if chunk.len() == 2 {
                next_level.push(hash_node(&chunk[0], &chunk[1]));
            } else {
                next_level.push(hash_node(&chunk[0], &chunk[0]));
            }
        }
        leaves = next_level;
    }
    
    sha256::Hash::from_byte_array(leaves[0]).to_string()
}

pub fn generate_merkle_proof(balances: &HashMap<String, u64>, target_addr: &str) -> Option<Vec<(String, bool)>> {
    if !balances.contains_key(target_addr) { return None; }
    
    let mut keys: Vec<_> = balances.keys().collect();
    keys.sort();

    let mut leaves: Vec<[u8; 32]> = keys.iter().map(|k| {
        hash_leaf(k, *balances.get(*k).unwrap())
    }).collect();

    let mut target_idx = keys.iter().position(|&k| k == target_addr)?;
    let mut proof = Vec::new();

    while leaves.len() > 1 {
        let mut next_level = Vec::new();
        for i in (0..leaves.len()).step_by(2) {
            let left = leaves[i];
            let right = if i + 1 < leaves.len() { leaves[i+1] } else { leaves[i] };

            if i == target_idx {
                proof.push((sha256::Hash::from_byte_array(right).to_string(), false));
            } else if i + 1 == target_idx {
                proof.push((sha256::Hash::from_byte_array(left).to_string(), true));
            }

            next_level.push(hash_node(&left, &right));
        }
        target_idx /= 2;
        leaves = next_level;
    }
    
    Some(proof)
}

pub fn create_settlement_tx(
    wallet: &LocalWallet,
    utxos: Vec<Utxo>,
    state_root_hash: String,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync>> {
    let secp = Secp256k1::new();
    let my_addr = wallet.get_address_obj();
    let fee = 1000;

    let op_return_data = PushBytesBuf::try_from(state_root_hash.as_bytes()[..32].to_vec())?;
    let op_return_script = Builder::new()
        .push_opcode(opcodes::all::OP_RETURN)
        .push_slice(&op_return_data)
        .into_script();

    let mut inputs = Vec::new();
    let mut total_in = 0;

    for utxo in utxos {
        total_in += utxo.value;
        inputs.push(TxIn {
            previous_output: OutPoint::new(bitcoin::Txid::from_str(&utxo.txid)?, utxo.vout),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        });
        if total_in >= fee { break; }
    }

    let mut outputs = vec![TxOut { value: 0, script_pubkey: op_return_script }];
    if total_in > fee + 546 {
        outputs.push(TxOut { value: total_in - fee, script_pubkey: my_addr.script_pubkey() });
    }

    let mut tx = Transaction { 
        version: 2, 
        lock_time: bitcoin::absolute::LockTime::ZERO, 
        input: inputs, 
        output: outputs 
    };

    let priv_key = PrivateKey::from_wif(&wallet.secret_wif)?;
    let prev_script = my_addr.script_pubkey();

    let mut signatures = Vec::new();
    {
        let sighash_cache = SighashCache::new(&tx);
        for i in 0..tx.input.len() {
            let msg_hash = sighash_cache.legacy_signature_hash(i, &prev_script, EcdsaSighashType::All as u32)?;
            let msg = Message::from_slice(msg_hash.as_byte_array())?;
            let sig = secp.sign_ecdsa(&msg, &priv_key.inner);
            let mut sig_der = sig.serialize_der().to_vec();
            sig_der.push(EcdsaSighashType::All as u8);
            signatures.push(sig_der);
        }
    }

    let pubkey_bytes = priv_key.public_key(&secp).to_bytes();
    for (i, sig_der) in signatures.into_iter().enumerate() {
        tx.input[i].script_sig = Builder::new()
            .push_slice(&PushBytesBuf::try_from(sig_der)?)
            .push_slice(&PushBytesBuf::try_from(pubkey_bytes.to_vec())?)
            .into_script();
    }

    Ok(tx)
}