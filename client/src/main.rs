pub mod utils;
use crate::utils::*;
use anyhow::{Error, Result};
use bytemuck::{Pod, Zeroable};
use core::num;
use litesvm::LiteSVM;
use phoenix_mm::types::*;
use phoenix_mm::utils::*;
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
use spl_associated_token_account::{
    get_associated_token_address, get_associated_token_address_with_program_id,
};
use spl_token::state::Account as TokenAccount;
use std::collections::BTreeMap;
use std::marker;
use std::time::Duration;
pub const PROGRAM_ID: Pubkey = solana_sdk::pubkey!("6RavfKEf7qqJLXmmwUWVBkaN56pZ71JtqCFfS99bHrpu");
const PHOENIX: Pubkey = pubkey!("PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY");
const PHOENIX_SEAT_MANAGER: Pubkey = pubkey!("PSMxQbAoDWDbvd9ezQJgARyq6R9L5kJAasaLDVcZwf1");
const PHOENIX_LOG_AUTH: Pubkey = pubkey!("7aDTsspkQNGKmrexAN7FLx9oxU3iPczSSvHNggyuqYkR");
const WALLET_PATH: &str = "/home/mubariz/wallnuts/mainnet-keypair.json";
pub const WALLET: Pubkey = pubkey!("5BvrQfDzwjFFjpaAys2KA1a7GuuhLXKJoCWykhsoyHet"); //replace with your actual wallet
const SOL_BALANCE: u64 = 1000 * 1_000_000_000; //hehehe
const USDC_BALANCE: u64 = 10_000 * 1_000_000;

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

const MAX_DMMS: u64 = 128;

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct SeatManager {
    pub market: Pubkey,
    pub authority: Pubkey,
    pub successor: Pubkey,
    pub num_makers: u64,
    pub _header_padding: [u64; 11],
    pub designated_market_makers: [Pubkey; MAX_DMMS as usize],
    pub _dmm_padding: [u128; MAX_DMMS as usize],
}
impl SeatManager {
    pub fn contains(&self, trader: &Pubkey) -> bool {
        self.designated_market_makers
            .iter()
            .take(self.num_makers as usize)
            .any(|dmm| dmm == trader)
    }
}
#[tokio::main]
async fn main() {
    let mut litesvm = LiteSVM::new().with_blockhash_check(true);
    let rpc = RpcClient::new("https://api.mainnet-beta.solana.com");
    let pool = Pubkey::from_str_const("4DoNfFBfF7UokCC2FQzriy7yHK6DY6NVdYpuekQ5pRgg"); //phoenix sol-usdc pool
    let base_mint = Pubkey::from_str_const("So11111111111111111111111111111111111111112"); //sol
    let quote_mint = Pubkey::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"); //usdc
    let base_account_address = get_associated_token_address(&WALLET, &base_mint);
    let quote_account_address = get_associated_token_address(&WALLET, &quote_mint);
    let base_account =
        get_dummy_token_account(&litesvm, WALLET, base_mint, spl_token::id(), SOL_BALANCE).unwrap();
    let quote_account =
        get_dummy_token_account(&litesvm, WALLET, quote_mint, spl_token::id(), USDC_BALANCE)
            .unwrap();
    let base_vault = Pubkey::from_str_const("8g4Z9d6PqGkgH31tMW6FwxGhwYJrXpxZHQrkikpLJKrG");
    let quote_vault = Pubkey::from_str_const("3HSYXeGc3LjEPCuzoNDjQN37F1ebsSiR4CqXVqQCdekZ");
    //derive nexessary pda's
    let strategy = Pubkey::find_program_address(
        &[b"phoenix_strategy".as_ref(), WALLET.as_ref()],
        &PROGRAM_ID,
    )
    .0;
    let seat = Pubkey::find_program_address(
        &[b"seat".as_ref(), pool.as_ref(), WALLET.as_ref()],
        &PHOENIX,
    )
    .0;
    let seat_manager = Pubkey::find_program_address(&[pool.as_ref()], &PHOENIX_SEAT_MANAGER).0;
    let seat_deposit_collector =
        Pubkey::find_program_address(&[pool.as_ref(), b"deposit".as_ref()], &PHOENIX_SEAT_MANAGER)
            .0;
    //fetching accounts from mainnet to hydrate svm
    let mut addresses = vec![
        WALLET,
        pool,
        seat_manager,
        seat_deposit_collector,
        base_mint,
        quote_mint,
        base_vault,
        quote_vault,
    ];
    let mut mainnet_accounts = rpc.get_multiple_accounts(&addresses).unwrap();

    for (address, maybe_account) in addresses.iter().zip(mainnet_accounts.clone().into_iter()) {
        if let Some(account) = maybe_account {
            litesvm.set_account(*address, account);
        }
    }
    //dummy token accounts
    litesvm.set_account(base_account_address, base_account);
    litesvm.set_account(quote_account_address, quote_account);

    // add necessary programs
    litesvm.add_program_from_file(PROGRAM_ID, "../target/deploy/phoenix_mm.so");
    litesvm.add_program_from_file(PHOENIX, "../phoenix.so");
    litesvm.add_program_from_file(PHOENIX_SEAT_MANAGER, "../phoniex_seat_manager.so");

    // ---InitalizeInstruction---
    //inital config
    let quote_edge_in_bps: u64 = 2;
    let quote_size_in_quote_atoms: u64 = 2 * 1_000_00;
    //price_improvement_behavior: bot behaves when placing prices compared to what’s already on the orderbook.
    let price_improvement_behavior: u8 = 0; //price improvment behaviour (0 ->join,1->Dime,2->Ignore)
    let post_only: u8 = false as u8;

    let mut accounts = vec![];
    accounts.push(AccountMeta::new(strategy, false));
    accounts.push(AccountMeta::new(WALLET, true));
    accounts.push(AccountMeta::new_readonly(pool, false));
    accounts.push(AccountMeta::new_readonly(system_program::id(), false));

    let mut data: Vec<u8> = vec![0u8];
    let initalize_params = StrategyParams {
        quote_edge_in_bps,
        quote_size_in_quote_atoms,
        price_improvement_behavior,
        post_only,
        padding: [0u8; 6],
    };
    data.extend_from_slice(unsafe { to_bytes(&initalize_params, 24) });
    execute_transaction(&mut litesvm, accounts, data, PROGRAM_ID).await;

    //refresh accounts for claim seat ix
    addresses = vec![
        WALLET,
        seat_manager,
        seat_deposit_collector,
        base_mint,
        quote_mint,
        base_vault,
        quote_vault,
    ];

    mainnet_accounts = rpc.get_multiple_accounts(&addresses).unwrap();

    for (address, maybe_account) in addresses.iter().zip(mainnet_accounts.clone().into_iter()) {
        if let Some(account) = maybe_account {
            litesvm.set_account(*address, account);
        }
    }
    accounts = vec![];
    accounts.push(AccountMeta::new_readonly(PHOENIX, false));
    accounts.push(AccountMeta::new_readonly(PHOENIX_LOG_AUTH, false));
    accounts.push(AccountMeta::new(pool, false));
    accounts.push(AccountMeta::new(seat_manager, false));
    accounts.push(AccountMeta::new(seat_deposit_collector, false));
    accounts.push(AccountMeta::new_readonly(WALLET, false));
    accounts.push(AccountMeta::new(WALLET, true));
    accounts.push(AccountMeta::new(seat, false));
    accounts.push(AccountMeta::new_readonly(system_program::id(), false));
    data = vec![1u8]; //phoniex_request_seat disc
    execute_transaction(&mut litesvm, accounts, data, PHOENIX_SEAT_MANAGER).await;

    let client = Client::new();
    for i in 0..5 {
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
        addresses = vec![
            WALLET,
            seat_manager,
            seat_deposit_collector,
            base_mint,
            quote_mint,
            base_vault,
            quote_vault,
        ];

        mainnet_accounts = rpc.get_multiple_accounts(&addresses).unwrap();

        for (address, maybe_account) in addresses.iter().zip(mainnet_accounts.into_iter()) {
            if let Some(account) = maybe_account {
                litesvm.set_account(*address, account);
            }
        }

        // ---UpdateInstruction
        accounts = vec![];
        accounts.push(AccountMeta::new(strategy, false));
        accounts.push(AccountMeta::new(pool, false));
        accounts.push(AccountMeta::new(WALLET, false));
        accounts.push(AccountMeta::new_readonly(PHOENIX, false));
        accounts.push(AccountMeta::new_readonly(PHOENIX_LOG_AUTH, false));
        accounts.push(AccountMeta::new(seat, false));
        accounts.push(AccountMeta::new(base_account_address, false));
        accounts.push(AccountMeta::new(quote_account_address, false));
        accounts.push(AccountMeta::new(base_vault, false));
        accounts.push(AccountMeta::new(quote_vault, false));
        accounts.push(AccountMeta::new_readonly(spl_token::id(), false));

        data = vec![1u8];
        data.extend_from_slice(&(price_u64 * 1_000_000u64).to_le_bytes()); //  fairPriceInQuoteAtomsPerRawBaseUnit: new BN(Math.floor(price * 1e6)),
        data.extend_from_slice(unsafe { to_bytes(&initalize_params, 24) });
        execute_transaction(&mut litesvm, accounts, data, PROGRAM_ID).await;
        // tokio::time::sleep(order_update_delay).await;
    }
}
