use crate::*;
use anyhow::{Error, Result};
use bytemuck::{Pod, Zeroable};
use core::num;
use litesvm::LiteSVM;
use phoenix_mm::types::*;
use phoenix_mm::utils::*;
use reqwest::Client;
use serde::Deserialize;
use solana_client::rpc_client;
use solana_client::rpc_client::RpcClient;
use solana_sdk::client;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    keccak,
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
//Coin base api structure
#[derive(Deserialize, Debug)]
pub struct PriceData {
    pub data: PriceInner,
}

#[derive(Deserialize, Debug)]
pub struct PriceInner {
    pub amount: String,
    base: String,
    currency: String,
}
pub unsafe fn to_bytes<T>(data: &T, len: usize) -> &[u8] {
    core::slice::from_raw_parts(data as *const T as *const u8, len)
}
pub fn get_dummy_token_account(
    svm: &LiteSVM,
    owner: Pubkey,
    mint: Pubkey,
    token_program: Pubkey,
    amount: u64,
) -> Result<Account, Error> {
    let token_account_data = TokenAccount {
        mint: mint,
        owner: owner,
        amount,
        delegate: None.into(),
        state: spl_token::state::AccountState::Initialized,
        is_native: None.into(),
        delegated_amount: 0,
        close_authority: None.into(),
    };
    let mut token_account_data_bytes = vec![0; TokenAccount::LEN];
    TokenAccount::pack(token_account_data, &mut token_account_data_bytes).unwrap();
    // Grab the minimum amount of lamports to make it rent exempt
    let lamports = svm.minimum_balance_for_rent_exemption(TokenAccount::LEN);

    Ok(Account {
        lamports,
        data: token_account_data_bytes.clone(),
        owner: token_program,
        executable: false,
        rent_epoch: 0,
    })
}
pub fn hydrate_with_mainnet(rpc: &RpcClient, litesvm: &mut LiteSVM, addresses: Vec<Pubkey>) {
    let mainnet_accounts = rpc.get_multiple_accounts(&addresses).unwrap();

    for (address, maybe_account) in addresses.iter().zip(mainnet_accounts.into_iter()) {
        if let Some(account) = maybe_account {
            litesvm.set_account(*address, account).unwrap();
        }
    }
}
pub async fn execute_transaction(
    litesvm: &mut LiteSVM,
    accounts: Vec<AccountMeta>,
    data: Vec<u8>,
    program_id: Pubkey,
) -> anyhow::Result<()> {
    let payer = Keypair::read_from_file(WALLET_PATH).unwrap();
    let ix = Instruction {
        program_id: program_id,
        accounts,
        data,
    };
    let blockhash = litesvm.latest_blockhash();
    let message = Message::try_compile(&WALLET, &[ix], &[], blockhash).unwrap();
    let tx =
        VersionedTransaction::try_new(solana_sdk::message::VersionedMessage::V0(message), &[payer])
            .unwrap();

    println!("BlockHash : {:#?}", blockhash);
    println!("Signature : {:#?}", tx.signatures[0]);
    let reuslt = litesvm.send_transaction(tx).unwrap();
    println!("{:#?}", reuslt.logs);
    litesvm.expire_blockhash();
    Ok(())
}
//hardcoded for sol/usdc for now
pub async fn get_price(client: &Client) -> u64 {
    let resp = client
        .get("https://api.coinbase.com/v2/prices/SOL-USD/spot")
        .send()
        .await
        .unwrap()
        .json::<PriceData>()
        .await
        .unwrap();
    let price_f64: f64 = resp.data.amount.parse().unwrap();
    (price_f64).round() as u64
}
pub fn add_seat_to_market(litesvm: &LiteSVM, rpc: &RpcClient, market: Pubkey) -> Account {
    let mainnet_market_account = rpc.get_account(&market).unwrap();
    let mut bytes = mainnet_market_account.data;
    let (market_header_bytes, _) = bytes.split_at_mut(size_of::<MarketHeader>());
    let market_size_params = deserialize_market_header(market_header_bytes)
        .unwrap()
        .market_size_params;
    let market = deserialize_market_mut(&mut bytes, &market_size_params).unwrap();
    market.get_or_register_trader(&WALLET.to_bytes());
    let pool_account = Account {
        lamports: litesvm.minimum_balance_for_rent_exemption(bytes.len()), //size might be change after insertion
        data: bytes.to_vec(),
        owner: mainnet_market_account.owner,
        executable: mainnet_market_account.executable,
        rent_epoch: mainnet_market_account.rent_epoch,
    };
    pool_account
}
pub fn create_seat(litesvm: &LiteSVM, market: Pubkey, trader: Pubkey) -> Account {
    let discriminant = u64::from_le_bytes(
        keccak::hashv(&[
            PHOENIX.as_ref(),
            "phoenix::program::accounts::Seat".as_bytes(),
        ])
        .as_ref()[..8]
            .try_into()
            .unwrap(),
    );
    let mut data = Vec::with_capacity(128);
    data.extend_from_slice(&discriminant.to_le_bytes());
    data.extend_from_slice(market.as_ref());
    data.extend_from_slice(trader.as_ref());
    // Append approval_status (1 = Approved)
    data.extend_from_slice(&1u64.to_le_bytes());
    data.extend_from_slice(&[0u8; 48]);
    Account {
        lamports: litesvm.minimum_balance_for_rent_exemption(128),
        data,
        owner: PHOENIX,
        executable: false,
        rent_epoch: 0,
    }
}
/*
pub fn get_seat_manager_seeds(
    market: &Pubkey,
    seat_manager: &Pubkey,
    program_id: &Pubkey,
) -> Vec<Vec<u8>> {
    let mut seeds = vec![market.to_bytes().to_vec()];
    let (seat_manager_key, bump) = Pubkey::find_program_address(
        seeds
            .iter()
            .map(|seed| seed.as_slice())
            .collect::<Vec<&[u8]>>()
            .as_slice(),
        program_id,
    );
    seeds.push(vec![bump]);

    if seat_manager_key == *seat_manager {
        Ok(seeds)
    } else {

        //invlaid ix data
    }
}
*/
