use std::marker;

use anyhow::{Error, Result};
use litesvm::LiteSVM;
use reqwest::Client;
use serde::Deserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    message::v0::Message,
    program_pack::Pack,
    pubkey,
    pubkey::Pubkey,
    signature::Keypair,
    signer::{EncodableKey, Signer},
    system_program,
    sysvar::{Sysvar, clock::Clock},
    transaction::VersionedTransaction,
};
use spl_associated_token_account::get_associated_token_address_with_program_id;
use spl_token::state::Account as TokenAccount;
pub const PROGRAM_ID: Pubkey = solana_sdk::pubkey!("6RavfKEf7qqJLXmmwUWVBkaN56pZ71JtqCFfS99bHrpu");
const PHOENIX: Pubkey = pubkey!("PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY");
const PHOENIX_LOG_AUTH: Pubkey = pubkey!("7aDTsspkQNGKmrexAN7FLx9oxU3iPczSSvHNggyuqYkR");
const WALLET_PATH: &str = "/home/mubariz/wallnuts/mainnet-keypair.json";
pub const WALLET: Pubkey = pubkey!("5BvrQfDzwjFFjpaAys2KA1a7GuuhLXKJoCWykhsoyHet"); //replace with your actual wallet
#[derive(Deserialize, Debug)]
struct PriceData {
    data: PriceInner,
}

#[derive(Deserialize, Debug)]
struct PriceInner {
    amount: String,
    base: String,
    currency: String,
}
pub struct PhoenixStrategyState {
    pub trader: Pubkey,
    pub market: Pubkey,
    // Order parameters
    pub bid_order_sequence_number: u64,
    pub bid_price_in_ticks: u64,
    pub initial_bid_size_in_base_lots: u64,
    pub ask_order_sequence_number: u64,
    pub ask_price_in_ticks: u64,
    pub initial_ask_size_in_base_lots: u64,
    pub last_update_slot: u64,
    pub last_update_unix_timestamp: i64,
    // Strategy parameters
    /// Number of basis points betweeen quoted price and fair price
    pub quote_edge_in_bps: u64,
    /// Order notional size in quote atoms
    pub quote_size_in_quote_atoms: u64,
    /// If set to true, the orders will never cross the spread
    pub post_only: u8,
    /// Determines whether/how to improve BBO
    pub price_improvement_behavior: u8,
    padding: [u8; 6],
}
pub fn simulate_transaction(litesvm: &mut LiteSVM, accounts: Vec<AccountMeta>, data: Vec<u8>) {
    let payer = Keypair::read_from_file(WALLET_PATH).unwrap();
    let ix = Instruction {
        program_id: PROGRAM_ID,
        accounts,
        data,
    };
    let message = Message::try_compile(&WALLET, &[ix], &[], litesvm.latest_blockhash()).unwrap();
    let tx =
        VersionedTransaction::try_new(solana_sdk::message::VersionedMessage::V0(message), &[payer])
            .unwrap();
    //let mut clock = litesvm.get_sysvar::<Clock>();
    //    clock.unix_timestamp = 1772448000;
    //  litesvm.set_sysvar::<Clock>(&clock);
    let result = litesvm.simulate_transaction(tx).unwrap();
    println!("{:#?}", result.meta.logs)
}
#[tokio::main]
async fn main() {
    let rpc = RpcClient::new("https://api.mainnet-beta.solana.com");
    let pool = Pubkey::from_str_const("4DoNfFBfF7UokCC2FQzriy7yHK6DY6NVdYpuekQ5pRgg"); //phoenix sol-usdc pool
    let base_account = Pubkey::from_str_const("689gZnbWXCGDcTwqknp9CtRZGgrHxFmhQKBCFBcJWeJY"); //sol
    let quote_account = Pubkey::from_str_const("GSBto5i58DWh8jimTLqhq5eC1KUZKX5grNYFeYyGT8K"); //usdc
    //vault addresses can be derived from pool data too
    let base_vault = Pubkey::from_str_const("8g4Z9d6PqGkgH31tMW6FwxGhwYJrXpxZHQrkikpLJKrG");
    let quote_vault = Pubkey::from_str_const("3HSYXeGc3LjEPCuzoNDjQN37F1ebsSiR4CqXVqQCdekZ");
    let strategy = Pubkey::find_program_address(
        &[b"phoniex_strategy".as_ref(), WALLET.as_ref()],
        &PROGRAM_ID,
    )
    .0;
    let seat = Pubkey::find_program_address(
        &[b"seat".as_ref(), pool.as_ref(), WALLET.as_ref()],
        &PROGRAM_ID,
    )
    .0;
    //fetching some accounts from mainnet to hydrate litesvm
    let mut mainnet_accounts = rpc.get_multiple_accounts(&[WALLET, pool]).unwrap();
    let mut litesvm = LiteSVM::new();
    litesvm
        .set_account(WALLET, mainnet_accounts[0].clone().unwrap())
        .unwrap();
    litesvm
        .set_account(pool, mainnet_accounts[1].clone().unwrap())
        .unwrap();
    litesvm
        .add_program_from_file(PROGRAM_ID, "../target/deploy/phoniex_mm.so")
        .unwrap();
    litesvm
        .add_program_from_file(PHOENIX, "../phoenix.so")
        .unwrap();
    // ---InitalizeInstruction---
    let mut accounts = vec![];
    accounts.push(AccountMeta::new(strategy, false));
    accounts.push(AccountMeta::new(WALLET, true));
    accounts.push(AccountMeta::new_readonly(pool, false));
    accounts.push(AccountMeta::new_readonly(system_program::id(), false));

    let mut data: Vec<u8> = vec![0u8];
    let quote_edge_in_bps: u64 = 2;
    let quote_size_in_quote_atoms: u64 = 500u64 * 1_000_000u64;
    let price_improvement_behavior: u8 = 2; //price improvment behaviour (0 ->join,1->Dime,2->Ignore)
    let post_only: u8 = false as u8;
    //8+8+1+1+6
    data.extend_from_slice(&quote_edge_in_bps.to_le_bytes());
    data.extend_from_slice(&(quote_size_in_quote_atoms).to_le_bytes());
    data.extend_from_slice(&price_improvement_behavior.to_le_bytes());
    data.extend_from_slice(&post_only.to_le_bytes());
    data.extend_from_slice(&[0u8; 6]);
    //simulate initalize instruction
    simulate_transaction(&mut litesvm, accounts, data);

    //step below is specific to litesvm
    let space = core::mem::size_of::<PhoenixStrategyState>();
    let mut strategy_account = Account::new(
        litesvm.minimum_balance_for_rent_exemption(space),
        space,
        &PROGRAM_ID,
    );
    let strategy_data = PhoenixStrategyState {
        trader: WALLET, //user
        market: pool,   //market
        bid_order_sequence_number: 0,
        bid_price_in_ticks: 0,
        initial_bid_size_in_base_lots: 0,
        ask_order_sequence_number: 0,
        ask_price_in_ticks: 0,
        initial_ask_size_in_base_lots: 0,
        last_update_slot: litesvm.get_sysvar::<Clock>().slot,
        last_update_unix_timestamp: litesvm.get_sysvar::<Clock>().unix_timestamp,
        quote_edge_in_bps: quote_edge_in_bps,
        quote_size_in_quote_atoms: quote_size_in_quote_atoms,
        post_only: post_only,
        price_improvement_behavior: price_improvement_behavior,
        padding: [0; 6],
    };
    let data_bytes = unsafe {
        core::slice::from_raw_parts(
            (&strategy_data as *const PhoenixStrategyState) as *const u8,
            size_of::<PhoenixStrategyState>(),
        )
    };
    strategy_account.data = data_bytes.to_vec();
    //hydrate with strategy account as its created now
    litesvm.set_account(strategy, strategy_account).unwrap();
    let client = Client::new();
    for i in 0..10 {
        let resp = client
            .get("https://api.coinbase.com/v2/prices/SOL-USD/spot")
            .send()
            .await
            .unwrap()
            .json::<PriceData>()
            .await
            .unwrap();
        let price_f64: f64 = resp.data.amount.parse().unwrap();
        let price_u64 = (price_f64 * 1_000_000.0).round() as u64;

        println!("SOL/USD Price: ${}", price_f64);
        println!("Price in quote atoms (u64): {}", price_u64);
        mainnet_accounts = rpc
            .get_multiple_accounts(&[
                WALLET,
                pool,
                base_account,
                quote_account,
                base_vault,
                quote_vault,
            ])
            .unwrap();

        litesvm.set_account(WALLET, mainnet_accounts[0].clone().unwrap());
        litesvm.set_account(pool, mainnet_accounts[1].clone().unwrap());
        litesvm.set_account(base_account, mainnet_accounts[2].clone().unwrap());
        litesvm.set_account(quote_account, mainnet_accounts[3].clone().unwrap());
        litesvm.set_account(base_vault, mainnet_accounts[4].clone().unwrap());
        litesvm.set_account(quote_vault, mainnet_accounts[5].clone().unwrap());
        // ---UpdateInstruction
        accounts = vec![];
        accounts.push(AccountMeta::new(strategy, false));
        accounts.push(AccountMeta::new(pool, false));
        accounts.push(AccountMeta::new(WALLET, false));
        accounts.push(AccountMeta::new_readonly(PHOENIX, false));
        accounts.push(AccountMeta::new_readonly(PHOENIX_LOG_AUTH, false));
        accounts.push(AccountMeta::new(seat, false));
        accounts.push(AccountMeta::new(base_account, false));
        accounts.push(AccountMeta::new(quote_account, false));
        accounts.push(AccountMeta::new(base_vault, false));
        accounts.push(AccountMeta::new(quote_vault, false));
        accounts.push(AccountMeta::new_readonly(spl_token::id(), false));

        let mut data = vec![1u8];
        //8+8+8+1+1+6
        data.extend_from_slice(&(price_u64 * 1_000_000u64).to_le_bytes()); //  fairPriceInQuoteAtomsPerRawBaseUnit: new BN(Math.floor(price * 1e6)),
        data.extend_from_slice(&quote_edge_in_bps.to_le_bytes());
        data.extend_from_slice(&(quote_size_in_quote_atoms).to_le_bytes());
        data.extend_from_slice(&price_improvement_behavior.to_le_bytes());
        data.extend_from_slice(&post_only.to_le_bytes());
        data.extend_from_slice(&[0u8; 6]);
        simulate_transaction(&mut litesvm, accounts, data);
    }
}
