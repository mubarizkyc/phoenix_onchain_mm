use crate::*;
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
