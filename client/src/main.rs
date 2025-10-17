pub mod utils;
use std::vec;

use crate::utils::*;
use litesvm::LiteSVM;
use phoenix_mm::{
    types::*,
    utils::{deserialize_market, deserialize_market_header},
};
use reqwest::Client;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::{self, Account},
    instruction::AccountMeta,
    pubkey::{self, Pubkey},
    system_program,
};
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
    let client = Client::new();
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

    //dummy token accounts
    litesvm.set_account(base_account_address, base_account);
    litesvm.set_account(quote_account_address, quote_account);

    // add necessary programs
    litesvm.add_program_from_file(PROGRAM_ID, "../target/deploy/phoenix_mm.so");
    litesvm.add_program_from_file(PHOENIX, "../phoenix.so");
    litesvm.add_program_from_file(PHOENIX_SEAT_MANAGER, "../phoniex_seat_manager.so");
    let mut accounts: Vec<AccountMeta> = vec![];
    let mut market_account = rpc.get_account(&pool).unwrap();
    let mut market_data: &mut Vec<u8> = market_account.data;
    let (market_header_bytes, market_bytes) =
        market_account.data.split_at(size_of::<MarketHeader>());
    let market_size_params = deserialize_market_header(market_header_bytes)
        .unwrap()
        .market_size_params;
    let market = deserialize_market(&market_account.data, &market_size_params).unwrap();
    let mut registered_traders = market.get_registered_traders();
    if !registered_traders.contains(&WALLET.to_bytes()) {
        registered_traders
            .insert(WALLET.to_bytes(), TraderState::default())
            .unwrap();
    }
    let pool_account = Account {
        lamports: litesvm.minimum_balance_for_rent_exemption(market_account.data.len()), //size might be change after insertion
        data: market_account.data,
        owner: market_account.owner,
        executable: market_account.executable,
        rent_epoch: market_account.rent_epoch,
    };
    /*
    A special kind of manipulation to create a seat on market that is owned by seat_program + no evict of seat is needed
              undestand how to manipulate a market data to get a seat
            get seat_manager accounts,
            make sure seat manager belongs to market
        get seat manager seeds for the market
      1:  invoke request_seat_authorized_instruction (seeds will be usefull here)
        handlign of request seat authorized:
          let (seat_address, bump) = get_seat_address(market_key, trader);
        assert_with_msg(
            &seat_address == seat.key,
            ProgramError::InvalidAccountData,
            "Invalid seat address",
        )?;
        let space = size_of::<Seat>();
        let seeds = vec![
            b"seat".to_vec(),
            market_key.as_ref().to_vec(),
            trader.as_ref().to_vec(),
            vec![bump],
        ];
        create_account(
            payer,
            seat,
            system_program,
            &crate::id(),
            &Rent::get()?,
            space as u64,
            seeds,
        )?;
        let mut seat_bytes = seat.try_borrow_mut_data()?;
        *Seat::load_mut_bytes(&mut seat_bytes).ok_or(ProgramError::InvalidAccountData)? =
            Seat::new_init(*market_key, *trader)?;
             pub fn new_init(market: Pubkey, trader: Pubkey) -> Result<Self, ProgramError> {
            Ok(Self {
                discriminant: get_discriminant::<Seat>()?,
                market,
                trader,
                approval_status: SeatApprovalStatus::NotApproved as u64,
                _padding: [0; 6],
            })
        }

        //let see what happend after this tx

    2:pay double ata rent to seat deposit collector
        3:just edit seat data to change to approved seat


               */
    //Request a Seat from Phoniex Seat Manager Program
    //necessary accounts from claim seat
    /*
           hydrate_with_mainnet(
            &rpc,
            &mut litesvm,
            vec![
                WALLET,
                PHOENIX_LOG_AUTH,
                pool,
                seat_manager,
                seat_deposit_collector,
                base_mint,
                quote_mint,
                base_vault,
                quote_vault,
            ],
        );
        let mut accounts = vec![];
        accounts.push(AccountMeta::new_readonly(PHOENIX, false));
        accounts.push(AccountMeta::new_readonly(PHOENIX_LOG_AUTH, false));
        accounts.push(AccountMeta::new(pool, false));
        accounts.push(AccountMeta::new(seat_manager, false));
        accounts.push(AccountMeta::new(seat_deposit_collector, false));
        accounts.push(AccountMeta::new_readonly(WALLET, false));
        accounts.push(AccountMeta::new(WALLET, true));
        accounts.push(AccountMeta::new(seat, false));
        accounts.push(AccountMeta::new_readonly(system_program::id(), false));
        execute_transaction(&mut litesvm, accounts, vec![1u8], PHOENIX_SEAT_MANAGER).await;
    */
    // ---InitalizeInstruction---
    //inital config
    let initalize_params = StrategyParams {
        quote_edge_in_bps: 2,
        quote_size_in_quote_atoms: 500 * 1_000_000,
        price_improvement_behavior: 0,
        post_only: false as u8,
        padding: [0u8; 6],
    };
    //necessary accounts for initalize ix
    hydrate_with_mainnet(&rpc, &mut litesvm, vec![WALLET, pool]);
    accounts = vec![];
    accounts.push(AccountMeta::new(strategy, false));
    accounts.push(AccountMeta::new(WALLET, true));
    accounts.push(AccountMeta::new_readonly(pool, false));
    accounts.push(AccountMeta::new_readonly(system_program::id(), false));

    let mut data: Vec<u8> = vec![0u8];
    data.extend_from_slice(unsafe { to_bytes(&initalize_params, 24) });
    execute_transaction(&mut litesvm, accounts, data, PROGRAM_ID).await;
    for _ in 0..5 {
        let price = get_price(&client).await;

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
        data.extend_from_slice(&(price * 1_000_000u64).to_le_bytes());
        data.extend_from_slice(unsafe { to_bytes(&initalize_params, 24) });
        execute_transaction(&mut litesvm, accounts, data, PROGRAM_ID).await;
    }
}
