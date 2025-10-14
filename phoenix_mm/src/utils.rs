use crate::entrypoint::*;
use borsh::{BorshDeserialize, BorshSerialize};
use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    cpi::slice_invoke,
    default_panic_handler,
    instruction::{AccountMeta, Instruction},
    msg, no_allocator, program_entrypoint,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::Sysvar,
    sysvars::clock::Clock,
};
pub const PHONIEX_PROGRAM_ID: [u8; 32] = [
    5, 208, 234, 79, 51, 115, 112, 19, 165, 99, 224, 147, 72, 237, 182, 244, 89, 61, 145, 252, 118,
    65, 249, 36, 124, 36, 65, 168, 66, 161, 187, 235,
];
pub fn create_accounts(payer: &AccountInfo, account: &AccountInfo) {}
pub fn create_cancel_multiple_orders_by_id_with_free_funds_instruction(
    phoniex_program: &AccountInfo,
    phoenix_log_authority: &AccountInfo,
    market: &AccountInfo,
    trader: &AccountInfo,
    params: &CancelMultipleOrdersByIdParams,
) -> ProgramResult {
    let data = [
        (11 as u8).try_to_vec().unwrap(),
        params.try_to_vec().unwrap(),
    ]
    .concat();
    let account_metas = [
        AccountMeta::new(phoniex_program.key(), false, false), // phoenix program
        AccountMeta::new(phoenix_log_authority.key(), false, false), // log authority
        AccountMeta::new(trader.key(), true, true),            // user
        AccountMeta::new(market.key(), true, false),           // market
    ];
    let ix = Instruction {
        program_id: &PHONIEX_PROGRAM_ID,
        accounts: &account_metas,
        data: &data,
    };
    slice_invoke(
        &ix,
        &[&phoniex_program, &phoenix_log_authority, &trader, &market],
    )
}
pub fn create_new_order_with_custom_token_accounts(
    phoniex_program: &AccountInfo,
    market: &AccountInfo,
    trader: &AccountInfo,
    seat: &AccountInfo,
    base_account: &AccountInfo,
    quote_account: &AccountInfo,
    base_vault: &AccountInfo,
    quote_vault: &AccountInfo,
    phoenix_log_authority: &AccountInfo,
    token_program: &AccountInfo,
    order_packet: &OrderPacket,
) -> ProgramResult {
    if order_packet.is_take_only() {
        let ix = Instruction {
            program_id: &PHONIEX_PROGRAM_ID,
            accounts: &[
                AccountMeta::new(&PHONIEX_PROGRAM_ID, false, false),
                AccountMeta::new(phoenix_log_authority.key(), false, false),
                AccountMeta::new(market.key(), true, false),
                AccountMeta::new(trader.key(), false, true),
                AccountMeta::new(base_account.key(), true, false),
                AccountMeta::new(quote_account.key(), true, false),
                AccountMeta::new(&base_vault.key(), true, false),
                AccountMeta::new(&quote_vault.key(), true, false),
                AccountMeta::new(token_program.key(), false, false),
            ],
            data: &[
                (0 as u8).try_to_vec().unwrap(),
                order_packet.try_to_vec().unwrap(),
            ]
            .concat(),
        };
        slice_invoke(
            &ix,
            &[
                &phoniex_program,
                &phoenix_log_authority,
                &trader,
                &market,
                &seat,
                &quote_account,
                &base_account,
                &quote_vault,
                &base_vault,
                &token_program,
            ],
        )
    } else {
        let ix = Instruction {
            program_id: &PHONIEX_PROGRAM_ID,
            accounts: &[
                AccountMeta::new(&PHONIEX_PROGRAM_ID, false, false),
                AccountMeta::new(phoenix_log_authority.key(), false, false),
                AccountMeta::new(market.key(), true, false),
                AccountMeta::new(trader.key(), true, true),
                AccountMeta::new(&seat.key(), false, false),
                AccountMeta::new(base_account.key(), true, false),
                AccountMeta::new(quote_account.key(), true, false),
                AccountMeta::new(&base_vault.key(), true, false),
                AccountMeta::new(&quote_vault.key(), true, false),
                AccountMeta::new(token_program.key(), false, false),
            ],
            data: &[
                (2 as u8).try_to_vec().unwrap(),
                order_packet.try_to_vec().unwrap(),
            ]
            .concat(),
        };
        slice_invoke(
            &ix,
            &[
                &phoniex_program,
                &phoenix_log_authority,
                &trader,
                &market,
                &seat,
                &quote_account,
                &base_account,
                &quote_vault,
                &base_vault,
                &token_program,
            ],
        )
    }
}

pub fn create_new_multiple_order_with_custom_token_accounts(
    phoniex_program: &AccountInfo,
    market: &AccountInfo,
    trader: &AccountInfo,
    seat: &AccountInfo,
    base_account: &AccountInfo,
    quote_account: &AccountInfo,
    base_vault: &AccountInfo,
    quote_vault: &AccountInfo,
    phoenix_log_authority: &AccountInfo,
    token_program: &AccountInfo,
    multiple_order_packet: &MultipleOrderPacket,
) -> ProgramResult {
    //cpi to place multipule post only orders

    let data = [
        (16 as u8).try_to_vec().unwrap(),
        multiple_order_packet.try_to_vec().unwrap(),
    ]
    .concat();

    let account_metas = [
        AccountMeta::new(phoniex_program.key(), false, false), // phoenix program
        AccountMeta::new(phoenix_log_authority.key(), false, false), // log authority
        AccountMeta::new(market.key(), true, false),           // market
        AccountMeta::new(trader.key(), true, true),            // trader
        AccountMeta::new(&seat.key(), true, false),            // seat
        AccountMeta::new(base_account.key(), true, false),     // base_account
        AccountMeta::new(quote_account.key(), true, false),    //quote_account
        AccountMeta::new(&base_vault.key(), true, false),      // base_vault
        AccountMeta::new(&quote_vault.key(), true, false),     // quote_vault
        AccountMeta::new(token_program.key(), false, false),   // token program
    ];
    let ix = Instruction {
        program_id: &PHONIEX_PROGRAM_ID,
        accounts: &account_metas,
        data: &data,
    };

    slice_invoke(
        &ix,
        &[
            &phoniex_program,
            &phoenix_log_authority,
            &trader,
            &market,
            &seat,
            &quote_account,
            &base_account,
            &quote_vault,
            &base_vault,
            &token_program,
        ],
    )
}
