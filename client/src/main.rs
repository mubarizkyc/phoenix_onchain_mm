#![allow(warnings)]
pub mod utils;
use std::{env, vec};

use crate::utils::*;
use dotenvy::{dotenv, dotenv_override};
use litesvm::LiteSVM;
use phoenix_mm::{
    types::*,
    utils::{deserialize_market, deserialize_market_header},
};
use reqwest::{
    Client,
    header::{HeaderMap, HeaderValue, ORIGIN},
};
use solana_client::rpc_client::{RpcClient, RpcClientConfig};
use solana_rpc_client::http_sender::HttpSender;
use solana_sdk::{
    account::{self, Account},
    feature::create_account,
    instruction::AccountMeta,
    pubkey::Pubkey,
    system_program,
};
use solana_sdk::{entrypoint::HEAP_LENGTH, pubkey};
use spl_associated_token_account::get_associated_token_address;
const PROGRAM_ID: Pubkey = solana_sdk::pubkey!("6RavfKEf7qqJLXmmwUWVBkaN56pZ71JtqCFfS99bHrpu");
const PHOENIX: Pubkey = pubkey!("PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY");
const PHOENIX_SEAT_MANAGER: Pubkey = pubkey!("PSMxQbAoDWDbvd9ezQJgARyq6R9L5kJAasaLDVcZwf1");
const PHOENIX_LOG_AUTH: Pubkey = pubkey!("7aDTsspkQNGKmrexAN7FLx9oxU3iPczSSvHNggyuqYkR");
const WALLET_PATH: &str = "/home/mubariz/wallnuts/mainnet-keypair.json";
const WALLET: Pubkey = pubkey!("5BvrQfDzwjFFjpaAys2KA1a7GuuhLXKJoCWykhsoyHet"); //replace with your actual wallet

const SOL_BALANCE: u64 = 1000 * 1_000_000_000; //hehehe
const USDC_BALANCE: u64 = 10_000 * 1_000_000;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let rpc_url = env::var("RPC_URL").unwrap();
    let origin = env::var("ORIGIN_HEADER").unwrap();
    let price_fetch_client = Client::new();
    let mut headers = HeaderMap::new();
    headers.insert("origin", HeaderValue::from_str(&origin).unwrap());
    let req_client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    let http_sender_mainnet = HttpSender::new_with_client(rpc_url, req_client);
    let rpc = RpcClient::new_sender(http_sender_mainnet, RpcClientConfig::default());
    let mut litesvm = LiteSVM::new().with_blockhash_check(true);
    let market = Pubkey::from_str_const("4DoNfFBfF7UokCC2FQzriy7yHK6DY6NVdYpuekQ5pRgg"); //phoenix sol-usdc pool
    let base_mint = Pubkey::from_str_const("So11111111111111111111111111111111111111112"); //sol
    let quote_mint = Pubkey::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"); //usdc
    let base_account_address = get_associated_token_address(&WALLET, &base_mint);
    let quote_account_address = get_associated_token_address(&WALLET, &quote_mint);
    let base_account =
        get_dummy_token_account(&litesvm, WALLET, base_mint, spl_token::id(), SOL_BALANCE).unwrap();
    let quote_account =
        get_dummy_token_account(&litesvm, WALLET, quote_mint, spl_token::id(), USDC_BALANCE)
            .unwrap();
    //can also be derived from market data
    let base_vault = Pubkey::from_str_const("8g4Z9d6PqGkgH31tMW6FwxGhwYJrXpxZHQrkikpLJKrG");
    let quote_vault = Pubkey::from_str_const("3HSYXeGc3LjEPCuzoNDjQN37F1ebsSiR4CqXVqQCdekZ");
    //derive nexessary pda's
    let strategy = Pubkey::find_program_address(
        &[b"phoenix_strategy".as_ref(), WALLET.as_ref()],
        &PROGRAM_ID,
    )
    .0;
    let seat = Pubkey::find_program_address(
        &[b"seat".as_ref(), market.as_ref(), WALLET.as_ref()],
        &PHOENIX,
    )
    .0;
    let seat_manager = Pubkey::find_program_address(&[market.as_ref()], &PHOENIX_SEAT_MANAGER).0;
    let seat_deposit_collector = Pubkey::find_program_address(
        &[market.as_ref(), b"deposit".as_ref()],
        &PHOENIX_SEAT_MANAGER,
    )
    .0;

    // add necessary programs
    litesvm.add_program_from_file(PROGRAM_ID, "../target/deploy/phoenix_mm.so");
    litesvm.add_program_from_file(PHOENIX, "../phoenix.so");
    litesvm.add_program_from_file(PHOENIX_SEAT_MANAGER, "../phoniex_seat_manager.so");
    let market_account = add_seat_to_market(&litesvm, &rpc, market);
    //add seat account
    litesvm.set_account(seat, create_seat(&litesvm, market, WALLET));
    //add market account
    litesvm.set_account(market, market_account);
    //dummy token accounts
    litesvm.set_account(base_account_address, base_account);
    litesvm.set_account(quote_account_address, quote_account);

    // ---InitalizeInstruction---
    //inital config
    let initalize_params = StrategyParams {
        quote_edge_in_bps: 0,
        quote_size_in_quote_atoms: 500 * 1_000_000,
        price_improvement_behavior: 0,
        post_only: false as u8,
        padding: [0u8; 6],
    };
    //necessary accounts for initalize ix
    hydrate_with_mainnet(&rpc, &mut litesvm, vec![WALLET, market]);
    let mut accounts = vec![
        AccountMeta::new(strategy, false),
        AccountMeta::new(WALLET, true),
        AccountMeta::new_readonly(market, false),
        AccountMeta::new_readonly(system_program::id(), false),
    ];
    let mut data: Vec<u8> = vec![0u8];
    data.extend_from_slice(unsafe { to_bytes(&initalize_params, 24) });
    execute_transaction(&mut litesvm, accounts, data, PROGRAM_ID).await;

    for _ in 0..5 {
        let price = get_price(&price_fetch_client).await;

        println!("SOL/USD Price: ${}", price);
        hydrate_with_mainnet(
            &rpc,
            &mut litesvm,
            vec![
                WALLET,
                PHOENIX_LOG_AUTH,
                strategy,
                seat_manager,
                seat_deposit_collector,
                base_mint,
                quote_mint,
                base_vault,
                quote_vault,
            ],
        );
        //Note: I want the market data to be sycn with mainnet ,but my seat should be injected in it
        //considering a simple case where market is owned by seat_manager and no eviction  needed
        //add seat account

        litesvm.set_account(market, add_seat_to_market(&litesvm, &rpc, market));
        // ---UpdateInstruction
        accounts = vec![
            AccountMeta::new(strategy, false),
            AccountMeta::new(market, false),
            AccountMeta::new(WALLET, true),
            AccountMeta::new_readonly(PHOENIX, false),
            AccountMeta::new_readonly(PHOENIX_LOG_AUTH, false),
            AccountMeta::new(seat, false),
            AccountMeta::new(base_account_address, false),
            AccountMeta::new(quote_account_address, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ];
        data = vec![1u8];
        data.extend_from_slice(&(price * 1_000_000u64).to_le_bytes());
        data.extend_from_slice(unsafe { to_bytes(&initalize_params, 24) });
        execute_transaction(&mut litesvm, accounts, data, PROGRAM_ID).await;
    }
}
