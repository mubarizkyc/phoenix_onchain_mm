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
pub const PROGRAM_ID: Pubkey = solana_sdk::pubkey!("6RavfKEf7qqJLXmmwUWVBkaN56pZ71JtqCFfS99bHrpu");
const PHOENIX: Pubkey = pubkey!("PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY");
const PHOENIX_SEAT_MANAGER: Pubkey = pubkey!("PSMxQbAoDWDbvd9ezQJgARyq6R9L5kJAasaLDVcZwf1");
const PHOENIX_LOG_AUTH: Pubkey = pubkey!("7aDTsspkQNGKmrexAN7FLx9oxU3iPczSSvHNggyuqYkR");
const WALLET_PATH: &str = "/home/mubariz/wallnuts/mainnet-keypair.json";
pub const WALLET: Pubkey = pubkey!("5BvrQfDzwjFFjpaAys2KA1a7GuuhLXKJoCWykhsoyHet"); //replace with your actual wallet
pub unsafe fn to_bytes<T>(data: &T, len: usize) -> &[u8] {
    core::slice::from_raw_parts(data as *const T as *const u8, len)
}
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
pub struct EvictTraderAccountBackup {
    pub trader_pubkey: Pubkey,
    pub base_token_account_backup: Option<Pubkey>,
    pub quote_token_account_backup: Option<Pubkey>,
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
pub fn execute_transaction(
    litesvm: &mut LiteSVM,
    accounts: Vec<AccountMeta>,
    data: Vec<u8>,
    program_id: Pubkey,
) {
    let payer = Keypair::read_from_file(WALLET_PATH).unwrap();
    let ix = Instruction {
        program_id: program_id,
        accounts,
        data,
    };
    let message = Message::try_compile(&WALLET, &[ix], &[], litesvm.latest_blockhash()).unwrap();
    let tx =
        VersionedTransaction::try_new(solana_sdk::message::VersionedMessage::V0(message), &[payer])
            .unwrap();

    let reuslt = litesvm.send_transaction(tx).unwrap();
    println!("{:#?}", reuslt.logs);
}

#[tokio::main]
async fn main() {
    let rpc = RpcClient::new("https://api.mainnet-beta.solana.com");
    let pool = Pubkey::from_str_const("4DoNfFBfF7UokCC2FQzriy7yHK6DY6NVdYpuekQ5pRgg"); //phoenix sol-usdc pool
    let base_mint = Pubkey::from_str_const("So11111111111111111111111111111111111111112"); //sol
    let quote_mint = Pubkey::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"); //usdc
    let base_account = Pubkey::from_str_const("689gZnbWXCGDcTwqknp9CtRZGgrHxFmhQKBCFBcJWeJY"); //sol
    let quote_account = Pubkey::from_str_const("GSBto5i58DWh8jimTLqhq5eC1KUZKX5grNYFeYyGT8K"); //usdc
    //vault addresses can be derived from pool data too
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
    let mut mainnet_accounts = rpc
        .get_multiple_accounts(&[
            WALLET,
            pool,
            base_account,
            quote_account,
            base_vault,
            quote_vault,
            seat_manager,
            seat_deposit_collector,
            base_mint,
            quote_mint,
            base_vault,
            quote_vault,
        ])
        .unwrap();
    //new svm
    let mut litesvm = LiteSVM::new();
    // add necessary programs
    litesvm.add_program_from_file(PROGRAM_ID, "../target/deploy/phoenix_mm.so");
    litesvm.add_program_from_file(PHOENIX, "../phoenix.so");
    litesvm.add_program_from_file(PHOENIX_SEAT_MANAGER, "../phoniex_seat_manager.so");
    // add wallet ,pool accounts
    litesvm.set_account(WALLET, mainnet_accounts[0].clone().unwrap());
    litesvm.set_account(pool, mainnet_accounts[1].clone().unwrap());

    // ---InitalizeInstruction---
    //inital config
    let quote_edge_in_bps: u64 = 2;
    let quote_size_in_quote_atoms: u64 = 500 * 1_000_000;
    let price_improvement_behavior: u8 = 2; //price improvment behaviour (0 ->join,1->Dime,2->Ignore)
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
    execute_transaction(&mut litesvm, accounts, data, PROGRAM_ID);

    /*
    we need a seat to trade with market ,if the market is owned by phoniex seat manager then just do a claim set ix ,if its owned by some one else ,they have to grant it to you
    in both cases you may need evacuate seed ix
     */
    //check if we need to evacuate seat
    let seat_manager_account = mainnet_accounts[6].clone().unwrap();
    let market_data = mainnet_accounts[1].clone().unwrap().data;
    let (header_bytes, _) = market_data.split_at(size_of::<MarketHeader>());
    let market_header = bytemuck::try_from_bytes::<MarketHeader>(header_bytes).unwrap();
    let max_traders = market_header.market_size_params.num_seats;

    let market_state = deserialize_market(&market_data, &market_header.market_size_params).unwrap();

    let registered_traders = market_state.get_registered_traders();
    let num_traders = registered_traders.len() as u64;
    litesvm.set_account(seat_manager, seat_manager_account.clone());
    litesvm.set_account(seat_deposit_collector, mainnet_accounts[7].clone().unwrap());
    litesvm.set_account(base_mint, mainnet_accounts[8].clone().unwrap());
    litesvm.set_account(quote_mint, mainnet_accounts[9].clone().unwrap());
    litesvm.set_account(base_vault, mainnet_accounts[10].clone().unwrap());
    litesvm.set_account(quote_vault, mainnet_accounts[11].clone().unwrap());
    if num_traders == max_traders {
        let trader_tree = registered_traders
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect::<BTreeMap<_, _>>();
        let seat_manager_struct =
            bytemuck::try_from_bytes::<SeatManager>(&seat_manager_account.data).unwrap();
        for (trader_pubkey, trader_state) in trader_tree.iter() {
            let trader_pubkey_solana = Pubkey::new_from_array(*trader_pubkey);
            if trader_state.base_lots_locked.inner == 0 && trader_state.quote_lots_locked.inner == 0
            {
                // A DMM cannot be evicted directly. They must first be removed as a DMM. Skip DMMs in this search.
                if seat_manager_struct.contains(&trader_pubkey_solana) {
                    continue;
                }
                // we need to execute an evict_seat_instruction
                accounts = vec![];
                accounts.push(AccountMeta::new_readonly(PHOENIX, false));
                accounts.push(AccountMeta::new_readonly(PHOENIX_LOG_AUTH, false));
                accounts.push(AccountMeta::new(pool, false));
                accounts.push(AccountMeta::new(seat_manager, false));
                accounts.push(AccountMeta::new(seat_deposit_collector, false));
                accounts.push(AccountMeta::new_readonly(base_mint, false));
                accounts.push(AccountMeta::new_readonly(quote_mint, false));
                accounts.push(AccountMeta::new(base_vault, false));
                accounts.push(AccountMeta::new(quote_vault, false));
                accounts.push(AccountMeta::new_readonly(
                    spl_associated_token_account::id(),
                    false,
                ));
                accounts.push(AccountMeta::new_readonly(spl_token::id(), false));
                accounts.push(AccountMeta::new_readonly(system_program::id(), false));
                accounts.push(AccountMeta::new_readonly(WALLET, true));

                let base_account = get_associated_token_address(&trader_pubkey_solana, &base_mint);
                let quote_account =
                    get_associated_token_address(&trader_pubkey_solana, &quote_mint);

                let trader_seat = Pubkey::find_program_address(
                    &[b"seat".as_ref(), pool.as_ref(), trader_pubkey.as_ref()],
                    &PHOENIX,
                )
                .0;
                litesvm.set_account(base_account, rpc.get_account(&base_account).unwrap());
                litesvm.set_account(quote_account, rpc.get_account(&quote_account).unwrap());
                litesvm.set_account(trader_seat, rpc.get_account(&trader_seat).unwrap());
                accounts.push(AccountMeta::new(trader_pubkey_solana, false));
                accounts.push(AccountMeta::new(trader_seat, false));
                accounts.push(AccountMeta::new(base_account, false));
                accounts.push(AccountMeta::new(quote_account, false));

                data = vec![3u8];
                execute_transaction(&mut litesvm, accounts, data, PHOENIX_SEAT_MANAGER);
            }
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
    execute_transaction(&mut litesvm, accounts, data, PHOENIX_SEAT_MANAGER);
    println!("seat created succsefully");

    let client = Client::new();
    for i in 0..3 {
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
        let addresses = [
            strategy,
            pool,
            WALLET,
            PHOENIX_LOG_AUTH,
            seat,
            base_account,
            quote_account,
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
        accounts.push(AccountMeta::new(base_account, false));
        accounts.push(AccountMeta::new(quote_account, false));
        accounts.push(AccountMeta::new(base_vault, false));
        accounts.push(AccountMeta::new(quote_vault, false));
        accounts.push(AccountMeta::new_readonly(spl_token::id(), false));

        data = vec![1u8];
        data.extend_from_slice(&(price_u64 * 1_000_000u64).to_le_bytes()); //  fairPriceInQuoteAtomsPerRawBaseUnit: new BN(Math.floor(price * 1e6)),
        data.extend_from_slice(&quote_edge_in_bps.to_le_bytes());
        data.extend_from_slice(&(quote_size_in_quote_atoms).to_le_bytes());
        data.extend_from_slice(&price_improvement_behavior.to_le_bytes());
        data.extend_from_slice(&post_only.to_le_bytes());
        data.extend_from_slice(&[0u8; 6]);
        execute_transaction(&mut litesvm, accounts, data, PROGRAM_ID);
        litesvm.latest_blockhash();
    }
}
