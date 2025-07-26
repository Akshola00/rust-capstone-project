#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

const NODE_URL: &str = "http://127.0.0.1:18443";
const NODE_USER: &str = "alice";
const NODE_PASS: &str = "password";

fn send_transaction(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]),
        json!(null),
        json!(null),
        json!(null),
        json!(null),
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

static EMPTY_ADDRESS: [bitcoincore_rpc::bitcoin::Address<
    bitcoincore_rpc::bitcoin::address::NetworkUnchecked,
>; 0] = [];

fn main() -> bitcoincore_rpc::Result<()> {
    let rpc = Client::new(
        NODE_URL,
        Auth::UserPass(NODE_USER.to_owned(), NODE_PASS.to_owned()),
    )?;

    let chain_info = rpc.get_blockchain_info()?;
    println!("Chain Data: {chain_info:?}");

    for wallet_name in ["Miner", "Trader"] {
        let res = rpc.create_wallet(wallet_name, None, None, None, None);
        match res {
            Ok(_) => println!("Wallet '{wallet_name}' created."),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("already exists") {
                    println!("Wallet '{wallet_name}' has been created already");
                } else {
                    return Err(e);
                }
            }
        }
    }
    
    let miner_client = Client::new(
        &format!("{}/wallet/{}", NODE_URL, "Miner"),
        Auth::UserPass(NODE_USER.to_owned(), NODE_PASS.to_owned()),
    )?;
    let trader_client = Client::new(
        &format!("{}/wallet/{}", NODE_URL, "Trader"),
        Auth::UserPass(NODE_USER.to_owned(), NODE_PASS.to_owned()),
    )?;

    let reward_address = miner_client
        .get_new_address(Some("Mining Reward"), None)?
        .assume_checked();
    println!("Miner's reward address: {reward_address}");

    let mut current_balance = miner_client.get_balance(None, None)?.to_btc();
    let mut total_blocks = 0;
    while current_balance <= 0.0 {
        miner_client.generate_to_address(1, &reward_address)?;
        total_blocks += 1;
        current_balance = miner_client.get_balance(None, None)?.to_btc();
    }
    println!("Blocks generated for balance: {total_blocks}");

    println!("Miner balance: {current_balance} BTC");

    let receiver_address = trader_client
        .get_new_address(Some("Received"), None)?
        .assume_checked();
    println!("Trader's receiver address: {receiver_address}");

    let transaction_id = miner_client.send_to_address(
        &receiver_address,
        Amount::from_btc(20.0)?,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    println!("Transferred 20 BTC from Miner to Trader. Transaction ID: {transaction_id}");

    let mempool_data = miner_client.get_mempool_entry(&transaction_id)?;
    println!("Mempool data for txid {transaction_id}: {mempool_data:#?}");

    miner_client.generate_to_address(1, &reward_address)?;
    println!("Generated 1 block to confirm transaction.");

    use bitcoincore_rpc::bitcoin::Txid;
    use std::path::Path;

    let tx_data = miner_client.get_transaction(&transaction_id, None)?;
    let block_id = tx_data
        .info
        .blockhash
        .expect("Transaction should be confirmed in a block");
    let block_data = miner_client.get_block_info(&block_id)?;
    let height = block_data.height;

    let raw_transaction = miner_client.get_raw_transaction(&transaction_id, Some(&block_id))?;
    let decoded_transaction = miner_client.decode_raw_transaction(&raw_transaction, None)?;

    let input_data = &decoded_transaction.vin[0];
    let previous_txid = input_data.txid.expect("Input should have txid");
    let previous_vout = input_data.vout.expect("Input should have vout") as usize;
    let previous_tx = miner_client.get_raw_transaction(&previous_txid, None)?;
    let previous_decoded = miner_client.decode_raw_transaction(&previous_tx, None)?;
    let previous_output = &previous_decoded.vout[previous_vout];
    let input_addresses = &previous_output.script_pub_key.addresses;
    let source_address: String = input_addresses
        .first()
        .map(|a| format!("{}", a.clone().assume_checked()))
        .unwrap_or_default();
    let source_amount: f64 = previous_output.value.to_btc();

    let mut destination_address: String = String::new();
    let mut destination_amount: f64 = 0.0;
    let mut change_address: String = String::new();
    let mut change_amount: f64 = 0.0;

    for vout in &decoded_transaction.vout {
        if let Some(addr) = &vout.script_pub_key.address {
            let addr_str = addr.clone().assume_checked().to_string();
            println!("  Address: {addr_str}, Value: {:.8}", vout.value.to_btc());
            if addr_str == receiver_address.to_string() {
                destination_address = addr_str.clone();
                destination_amount = vout.value.to_btc();
            } else {
                let info = miner_client.get_address_info(&addr.clone().assume_checked());
                if let Ok(address_info) = info {
                    if address_info.is_mine.unwrap_or(false) {
                        change_address = addr_str.clone();
                        change_amount = vout.value.to_btc();
                    }
                }
            }
        }
    }
    println!("change_address: {change_address}");
    println!("destination_amount: {destination_amount:.8}");
    println!("change_amount: {change_amount:.8}");
    println!("destination_address: {destination_address}");

    let fee = source_amount - (destination_amount + change_amount);

    let output_path = Path::new("../out.txt");
    let mut output_file = File::create(output_path)?;
    writeln!(output_file, "{transaction_id}")?;
    writeln!(output_file, "{source_address}")?;
    writeln!(output_file, "{source_amount:.8}")?;
    writeln!(output_file, "{destination_address}")?;
    writeln!(output_file, "{destination_amount:.8}")?;
    writeln!(output_file, "{change_address}")?;
    writeln!(output_file, "{change_amount:.8}")?;
    writeln!(output_file, "{:.8}", fee.abs())?;
    writeln!(output_file, "{height}")?;
    writeln!(output_file, "{block_id}")?;
    println!("Transaction details saved to ../out.txt");

    Ok(())
}