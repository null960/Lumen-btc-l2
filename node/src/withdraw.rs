use bitcoin::blockdata::script::Builder;
use bitcoin::hash_types::Txid;
use bitcoin::secp256k1::{Message, Secp256k1};
use bitcoin::hashes::Hash; 
use bitcoin::script::PushBytesBuf;
use bitcoin::sighash::{SighashCache, EcdsaSighashType};
use bitcoin::absolute::LockTime;
use bitcoin::{
    Address, OutPoint, ScriptBuf, Transaction, TxIn, TxOut, Sequence, PrivateKey
};
use std::str::FromStr;
use std::error::Error;
use crate::wallet::LocalWallet;
use crate::btc_api::Utxo;

const FEE_SATS: u64 = 1000;

#[derive(Debug, Clone)]
pub struct BatchTarget {
    pub address: String,
    pub amount: u64,
}

pub fn create_batch_withdrawal_tx(
    wallet: &LocalWallet,
    utxos: Vec<Utxo>,
    targets: Vec<BatchTarget>,
) -> Result<Transaction, Box<dyn Error + Send + Sync>> {
    
    if targets.is_empty() {
        return Err("No withdrawal targets provided".into());
    }

    let secp = Secp256k1::new();
    let my_address = wallet.get_address_obj();

    let mut outputs = Vec::new();
    let mut total_send_amount = 0;

    for target in &targets {
        if target.amount < 546 { continue; }
        
        let recipient = Address::from_str(&target.address)?
            .require_network(bitcoin::Network::Testnet)
            .map_err(|_| "Invalid Testnet address")?;
            
        outputs.push(TxOut {
            value: target.amount,
            script_pubkey: recipient.script_pubkey(),
        });
        total_send_amount += target.amount;
    }

    if outputs.is_empty() {
        return Err("All targets were below dust limit".into());
    }

    let mut inputs: Vec<TxIn> = Vec::new();
    let mut total_input: u64 = 0;
    
    let estimated_fee = FEE_SATS + (outputs.len() as u64 * 50);

    for utxo in utxos {
        total_input += utxo.value;
        inputs.push(TxIn {
            previous_output: OutPoint::new(Txid::from_str(&utxo.txid)?, utxo.vout),
            script_sig: ScriptBuf::new(), 
            sequence: Sequence::MAX,
            witness: bitcoin::Witness::new(),
        });
        if total_input >= total_send_amount + estimated_fee { break; }
    }

    if total_input < total_send_amount + estimated_fee {
        return Err(format!("Insufficient funds: Need {}, have {}", total_send_amount + estimated_fee, total_input).into());
    }

    let change = total_input - total_send_amount - estimated_fee;
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

    let mut signatures = Vec::new();
    {
         let sighash_cache = SighashCache::new(&tx);
         for i in 0..tx.input.len() {
            let prev_script = my_address.script_pubkey();
            let sighash = sighash_cache.legacy_signature_hash(
                i, &prev_script, EcdsaSighashType::All.to_u32()
            )?;
            let msg = Message::from_slice(sighash.as_byte_array())?;
            let signature = secp.sign_ecdsa(&msg, &secret_key);
            let mut sig_der = signature.serialize_der().to_vec();
            sig_der.push(EcdsaSighashType::All as u8);
            signatures.push(sig_der);
         }
    }

    let pub_key_bytes = pub_key.to_bytes();
    for (i, sig_der) in signatures.into_iter().enumerate() {
        tx.input[i].script_sig = Builder::new()
            .push_slice(&PushBytesBuf::try_from(sig_der)?)
            .push_slice(&PushBytesBuf::try_from(pub_key_bytes.to_vec())?)
            .into_script();
    }

    Ok(tx)
}