use crate::types::*;
use borsh::BorshSerialize;
use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    cpi::slice_invoke,
    instruction::{AccountMeta, Instruction},
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};
use sokoban::ZeroCopy;
pub const PHONIEX_PROGRAM_ID: [u8; 32] = [
    5, 208, 234, 79, 51, 115, 112, 19, 165, 99, 224, 147, 72, 237, 182, 244, 89, 61, 145, 252, 118,
    65, 249, 36, 124, 36, 65, 168, 66, 161, 187, 235,
];
macro_rules! fifo_market_mut {
    ($num_bids:literal, $num_asks:literal, $num_seats:literal, $bytes:expr) => {
        FIFOMarket::<Pubkey, $num_bids, $num_asks, $num_seats>::load_mut_bytes($bytes)
            .ok_or(ProgramError::InvalidInstructionData)?
            as &mut dyn WritableMarket<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>
    };
}

macro_rules! fifo_market {
    ($num_bids:literal, $num_asks:literal, $num_seats:literal, $market_bytes:expr) => {
        FIFOMarket::<Pubkey, $num_bids, $num_asks, $num_seats>::load_bytes($market_bytes)
            .ok_or(ProgramError::InvalidInstructionData)?
            as &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>
    };
}
pub fn deserialize_market_header(data: &[u8]) -> Result<MarketHeader, ProgramError> {
    let header = bytemuck::try_from_bytes::<MarketHeader>(data).map_err(|_| {
        msg!("Failed to parse Phoenix market header");
        ProgramError::InvalidInstructionData
    })?;

    Ok(*header)
}
pub fn deserialize_market<'a>(
    market_bytes: &'a [u8],
    market_size_params: &'a MarketSizeParams,
) -> Result<&'a dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>, ProgramError> {
    let (_, market_bytes) = market_bytes.split_at(size_of::<MarketHeader>());

    let market = match (
        market_size_params.bids_size,
        market_size_params.asks_size,
        market_size_params.num_seats,
    ) {
        (512, 512, 128) => fifo_market!(512, 512, 128, market_bytes),
        (512, 512, 1025) => fifo_market!(512, 512, 1025, market_bytes),
        (512, 512, 1153) => fifo_market!(512, 512, 1153, market_bytes),
        (1024, 1024, 128) => fifo_market!(1024, 1024, 128, market_bytes),
        (1024, 1024, 2049) => fifo_market!(1024, 1024, 2049, market_bytes),
        (1024, 1024, 2177) => fifo_market!(1024, 1024, 2177, market_bytes),
        (2048, 2048, 128) => fifo_market!(2048, 2048, 128, market_bytes),
        (2048, 2048, 4097) => fifo_market!(2048, 2048, 4097, market_bytes),
        (2048, 2048, 4225) => fifo_market!(2048, 2048, 4225, market_bytes),
        (4096, 4096, 128) => fifo_market!(4096, 4096, 128, market_bytes),
        (4096, 4096, 8193) => fifo_market!(4096, 4096, 8193, market_bytes),
        (4096, 4096, 8321) => fifo_market!(4096, 4096, 8321, market_bytes),
        _ => {
            //    phoenix_log!("Invalid parameters for market");
            return Err(ProgramError::InvalidInstructionData);
        }
    };
    Ok(MarketWrapper::<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>::new(market).inner)
}
pub fn deserialize_market_mut<'a>(
    market_bytes: &'a mut [u8],
    market_size_params: &'a MarketSizeParams,
) -> Result<
    &'a mut dyn WritableMarket<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
    ProgramError,
> {
    let (_, market_bytes) = market_bytes.split_at_mut(size_of::<MarketHeader>());

    let market = match (
        market_size_params.bids_size,
        market_size_params.asks_size,
        market_size_params.num_seats,
    ) {
        (512, 512, 128) => fifo_market_mut!(512, 512, 128, market_bytes),
        (512, 512, 1025) => fifo_market_mut!(512, 512, 1025, market_bytes),
        (512, 512, 1153) => fifo_market_mut!(512, 512, 1153, market_bytes),
        (1024, 1024, 128) => fifo_market_mut!(1024, 1024, 128, market_bytes),
        (1024, 1024, 2049) => fifo_market_mut!(1024, 1024, 2049, market_bytes),
        (1024, 1024, 2177) => fifo_market_mut!(1024, 1024, 2177, market_bytes),
        (2048, 2048, 128) => fifo_market_mut!(2048, 2048, 128, market_bytes),
        (2048, 2048, 4097) => fifo_market_mut!(2048, 2048, 4097, market_bytes),
        (2048, 2048, 4225) => fifo_market_mut!(2048, 2048, 4225, market_bytes),
        (4096, 4096, 128) => fifo_market_mut!(4096, 4096, 128, market_bytes),
        (4096, 4096, 8193) => fifo_market_mut!(4096, 4096, 8193, market_bytes),
        (4096, 4096, 8321) => fifo_market_mut!(4096, 4096, 8321, market_bytes),
        _ => {
            //    phoenix_log!("Invalid parameters for market");
            return Err(ProgramError::InvalidInstructionData);
        }
    };
    Ok(MarketWrapperMut::<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>::new(market).inner)
}
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
                &market,
                &trader,
                &base_account,
                &quote_account,
                &base_vault,
                &quote_vault,
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
                &market,
                &trader,
                &seat,
                &base_account,
                &quote_account,
                &base_vault,
                &quote_vault,
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
            &market,
            &trader,
            &seat,
            &base_account,
            &quote_account,
            &base_vault,
            &quote_vault,
            &token_program,
        ],
    )
}

pub fn get_bid_price_in_ticks(
    fair_price_in_quote_atoms_per_raw_base_unit: u64,
    header: &MarketHeader,
    edge_in_bps: u64,
) -> u64 {
    let fair_price_in_ticks = fair_price_in_quote_atoms_per_raw_base_unit
        * header.raw_base_units_per_base_unit as u64
        / header.tick_size_in_quote_atoms_per_base_unit;
    let edge_in_ticks = edge_in_bps * fair_price_in_ticks / 10_000;
    fair_price_in_ticks - edge_in_ticks
}

pub fn get_ask_price_in_ticks(
    fair_price_in_quote_atoms_per_raw_base_unit: u64,
    header: &MarketHeader,
    edge_in_bps: u64,
) -> u64 {
    let fair_price_in_ticks = fair_price_in_quote_atoms_per_raw_base_unit
        * header.raw_base_units_per_base_unit as u64
        / header.tick_size_in_quote_atoms_per_base_unit;
    let edge_in_ticks = edge_in_bps * fair_price_in_ticks / 10_000;
    fair_price_in_ticks + edge_in_ticks
}
