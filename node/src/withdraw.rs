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

const FEE_SATS: u64 = 500;

pub fn create_withdrawal_tx(
    wallet: &LocalWallet,
    utxos: Vec<Utxo>,
    recipient_addr: String,
    amount_sats: u64,
) -> Result<Transaction, Box<dyn Error>> {
    
    let secp = Secp256k1::new();
    
    let recipient = Address::from_str(&recipient_addr)?
        .require_network(bitcoin::Network::Testnet)
        .map_err(|_| "Invalid Testnet address")?;
        
    let my_address = wallet.get_address_obj();

    let mut inputs: Vec<TxIn> = Vec::new();
    let mut total_input: u64 = 0;
    
    for utxo in utxos {
        total_input += utxo.value;
        
        let txid = Txid::from_str(&utxo.txid)?;
        let outpoint = OutPoint::new(txid, utxo.vout);
        
        inputs.push(TxIn {
            previous_output: outpoint,
            script_sig: ScriptBuf::new(), 
            sequence: Sequence::MAX,
            witness: bitcoin::Witness::new(),
        });

        if total_input >= amount_sats + FEE_SATS {
            break;
        }
    }

    if total_input < amount_sats + FEE_SATS {
        return Err("Insufficient funds in L1 Node Wallet".into());
    }

    let change = total_input - amount_sats - FEE_SATS;

    let mut outputs = Vec::new();
    
    outputs.push(TxOut {
        value: amount_sats,
        script_pubkey: recipient.script_pubkey(),
    });

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

    let mut sighash_cache = SighashCache::new(&tx);
    let mut signatures = Vec::new();

    for (i, _input) in tx.input.iter().enumerate() {
        let prev_script = my_address.script_pubkey();

        let sighash = sighash_cache.legacy_signature_hash(
            i,
            &prev_script,
            EcdsaSighashType::All.to_u32(),
        )?;
        
        let msg = Message::from_slice(sighash.as_byte_array())?;
        let signature = secp.sign_ecdsa(&msg, &secret_key);

        let mut sig_with_hashtype = signature.serialize_der().to_vec();
        sig_with_hashtype.push(EcdsaSighashType::All as u8);

        signatures.push(sig_with_hashtype);
    }

    for (i, sig_bytes) in signatures.into_iter().enumerate() {
        let sig_push = PushBytesBuf::try_from(sig_bytes)
            .map_err(|_| "Signature too long")?;
        
        let pub_key_bytes = pub_key.to_bytes();
        let pub_key_push = PushBytesBuf::try_from(pub_key_bytes.to_vec())
            .map_err(|_| "Pubkey too long")?;

        let script_sig = Builder::new()
            .push_slice(&sig_push)
            .push_slice(&pub_key_push)
            .into_script();

        tx.input[i].script_sig = script_sig;
    }

    Ok(tx)
}