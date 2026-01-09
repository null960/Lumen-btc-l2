use bitcoin::blockdata::script::Builder;
use bitcoin::hash_types::Txid;
use bitcoin::secp256k1::{Message, Secp256k1};
use bitcoin::hashes::{Hash, sha256};
use bitcoin::script::PushBytesBuf;
use bitcoin::sighash::{SighashCache, EcdsaSighashType};
use bitcoin::absolute::LockTime;
use bitcoin::opcodes;
use bitcoin::{
    OutPoint, ScriptBuf, Transaction, TxIn, TxOut, Sequence, PrivateKey
};
use std::str::FromStr;
use std::error::Error;
use std::collections::HashMap;
use crate::wallet::LocalWallet;
use crate::btc_api::Utxo;

const FEE_SATS: u64 = 500;

pub fn hash_state(balances: &HashMap<String, u64>) -> String {
    let mut users: Vec<_> = balances.keys().collect();
    users.sort();

    let mut raw_data = String::new();
    for user in users {
        let bal = balances.get(user).unwrap();
        raw_data.push_str(&format!("{}:{}|", user, bal));
    }

    let hash = sha256::Hash::hash(raw_data.as_bytes());
    hash.to_string()
}

pub fn create_settlement_tx(
    wallet: &LocalWallet,
    utxos: Vec<Utxo>,
    state_hash: String,
) -> Result<Transaction, Box<dyn Error>> {
    
    let secp = Secp256k1::new();
    let my_address = wallet.get_address_obj();

    let data_bytes = state_hash.as_bytes();
    let safe_len = std::cmp::min(data_bytes.len(), 80);
    
    let push_data = PushBytesBuf::try_from(data_bytes[..safe_len].to_vec())?;

    let op_return_script = Builder::new()
        .push_opcode(opcodes::all::OP_RETURN)
        .push_slice(&push_data)
        .into_script();

    let mut inputs: Vec<TxIn> = Vec::new();
    let mut total_input: u64 = 0;
    
    for utxo in utxos {
        total_input += utxo.value;
        let txid = Txid::from_str(&utxo.txid)?;
        inputs.push(TxIn {
            previous_output: OutPoint::new(txid, utxo.vout),
            script_sig: ScriptBuf::new(), 
            sequence: Sequence::MAX,
            witness: bitcoin::Witness::new(),
        });
        if total_input >= FEE_SATS { break; }
    }

    if total_input < FEE_SATS {
        return Err("Insufficient funds for Settlement Fee".into());
    }

    let mut outputs = Vec::new();

    outputs.push(TxOut {
        value: 0,
        script_pubkey: op_return_script,
    });

    let change = total_input - FEE_SATS;
    if change > 546 {
        outputs.push(TxOut {
            value: change,
            script_pubkey: my_address.script_pubkey(),
        });
    }

    let mut tx = Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: outputs,
    };

    let private_key = PrivateKey::from_str(&wallet.secret_wif)?;
    let secret_key = private_key.inner;
    let pub_key = private_key.public_key(&secp);

    let sighash_cache = SighashCache::new(&tx);
    let mut signatures = Vec::new();

    for (i, _input) in tx.input.iter().enumerate() {
        let prev_script = my_address.script_pubkey();
        let sighash = sighash_cache.legacy_signature_hash(
            i, &prev_script, EcdsaSighashType::All.to_u32()
        )?;
        
        let msg = Message::from_slice(sighash.as_byte_array())?;
        let signature = secp.sign_ecdsa(&msg, &secret_key);

        let mut sig_with_hashtype = signature.serialize_der().to_vec();
        sig_with_hashtype.push(EcdsaSighashType::All as u8);
        signatures.push(sig_with_hashtype);
    }

    for (i, sig_bytes) in signatures.into_iter().enumerate() {
        let sig_push = PushBytesBuf::try_from(sig_bytes)?;
        let pub_key_push = PushBytesBuf::try_from(pub_key.to_bytes().to_vec())?;
        
        tx.input[i].script_sig = Builder::new()
            .push_slice(&sig_push)
            .push_slice(&pub_key_push)
            .into_script();
    }

    Ok(tx)
}