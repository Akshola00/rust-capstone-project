#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

const RPC_URL: &str = "http://127.0.0.1:18443";
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
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

fn create_or_load_wallet(rpc: &Client, wallet_name: &str) -> bitcoincore_rpc::Result<()> {
    match rpc.call::<serde_json::Value>("loadwallet", &[json!(wallet_name)]) {
        Ok(_) => {
            println!("Wallet '{}' loaded", wallet_name);
            Ok(())
        }
        Err(_) => match rpc.call::<serde_json::Value>("createwallet", &[json!(wallet_name)]) {
            Ok(_) => {
                println!("Wallet '{}' created", wallet_name);
                Ok(())
            }
            Err(_) => {
                rpc.call::<serde_json::Value>("loadwallet", &[json!(wallet_name)])?;
                println!("Wallet '{}' loaded", wallet_name);
                Ok(())
            }
        },
    }
}

fn generate_blocks_to_address(
    rpc: &Client,
    address: &str,
    num_blocks: u32,
) -> bitcoincore_rpc::Result<Vec<String>> {
    #[derive(Deserialize)]
    struct GenerateResult {
        blocks: Vec<String>,
    }

    let result =
        rpc.call::<GenerateResult>("generatetoaddress", &[json!(num_blocks), json!(address)])?;
    Ok(result.blocks)
}

fn get_mempool_entry(rpc: &Client, txid: &str) -> bitcoincore_rpc::Result<serde_json::Value> {
    rpc.call::<serde_json::Value>("getmempoolentry", &[json!(txid)])
}

fn get_raw_transaction(rpc: &Client, txid: &str) -> bitcoincore_rpc::Result<serde_json::Value> {
    rpc.call::<serde_json::Value>("getrawtransaction", &[json!(txid), json!(true)])
}

fn get_block_info(rpc: &Client, block_hash: &str) -> bitcoincore_rpc::Result<serde_json::Value> {
    rpc.call::<serde_json::Value>("getblock", &[json!(block_hash)])
}

fn main() -> bitcoincore_rpc::Result<()> {
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);

    create_or_load_wallet(&rpc, "Miner")?;
    create_or_load_wallet(&rpc, "Trader")?;

    let miner_address = rpc.call::<String>("getnewaddress", &[json!("Mining Reward")])?;
    println!("Miner address: {}", miner_address);

    println!("Mining blocks for spendable balance...");
    generate_blocks_to_address(&rpc, &miner_address, 101)?;

    let miner_balance = rpc.call::<f64>("getbalance", &[])?;
    println!("Miner balance: {} BTC", miner_balance);
    println!("Block rewards need 100 confirmations to become spendable");

    let trader_address = rpc.call::<String>("getnewaddress", &[json!("Received")])?;
    println!("Trader address: {}", trader_address);

    println!("Sending 20 BTC to trader...");
    let txid = rpc.call::<String>("sendtoaddress", &[json!(trader_address), json!(20.0)])?;
    println!("Transaction sent: {}", txid);

    let mempool_entry = get_mempool_entry(&rpc, &txid)?;
    println!("Mempool entry: {:?}", mempool_entry);

    println!("Mining confirmation block...");
    let confirm_blocks = generate_blocks_to_address(&rpc, &miner_address, 1)?;
    let confirm_block_hash = &confirm_blocks[0];
    println!("Confirmed in block: {}", confirm_block_hash);

    let raw_tx = get_raw_transaction(&rpc, &txid)?;
    let block_info = get_block_info(&rpc, confirm_block_hash)?;
    let block_height = block_info["height"].as_u64().unwrap();

    let tx_details = raw_tx.as_object().unwrap();
    let vin = tx_details["vin"].as_array().unwrap();
    let vout = tx_details["vout"].as_array().unwrap();

    let input_txid = vin[0]["txid"].as_str().unwrap();
    let input_vout = vin[0]["vout"].as_u64().unwrap();

    let input_tx = get_raw_transaction(&rpc, input_txid)?;
    let input_tx_details = input_tx.as_object().unwrap();
    let input_vout_details = input_tx_details["vout"].as_array().unwrap();
    let input_vout_info = &input_vout_details[input_vout as usize];
    let input_amount = input_vout_info["value"].as_f64().unwrap();
    let input_address = input_vout_info["scriptPubKey"]["addresses"][0]
        .as_str()
        .unwrap();

    let mut trader_output_amount = 0.0;
    let mut miner_change_amount = 0.0;
    let mut trader_output_address = "";
    let mut miner_change_address = "";

    for output in vout {
        let amount = output["value"].as_f64().unwrap();
        let addresses = output["scriptPubKey"]["addresses"].as_array().unwrap();
        let address = addresses[0].as_str().unwrap();

        if address == trader_address {
            trader_output_amount = amount;
            trader_output_address = address;
        } else {
            miner_change_amount = amount;
            miner_change_address = address;
        }
    }

    let transaction_fees = input_amount - trader_output_amount - miner_change_amount;

    let mut output_file = File::create("../out.txt")?;
    writeln!(output_file, "{}", txid)?;
    writeln!(output_file, "{}", input_address)?;
    writeln!(output_file, "{}", input_amount)?;
    writeln!(output_file, "{}", trader_output_address)?;
    writeln!(output_file, "{}", trader_output_amount)?;
    writeln!(output_file, "{}", miner_change_address)?;
    writeln!(output_file, "{}", miner_change_amount)?;
    writeln!(output_file, "{}", transaction_fees)?;
    writeln!(output_file, "{}", block_height)?;
    writeln!(output_file, "{}", confirm_block_hash)?;

    println!("Output written to ../out.txt");

    Ok(())
}
