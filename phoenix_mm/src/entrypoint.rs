#![allow(unexpected_cfgs)]
use crate::types::*;
use crate::utils::*;
use pinocchio_log::logger::{Argument, Log, Logger};
use pinocchio_system::instructions::CreateAccount;
//use crate::instruction::{self, MyProgramInstruction};
use bytemuck::checked::try_from_bytes;
use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    default_allocator,
    instruction::{Seed, Signer},
    msg, program_entrypoint,
    program_error::ProgramError,
    pubkey::Pubkey,
    pubkey::find_program_address,
    sysvars::{Sysvar, clock::Clock, rent::Rent},
};

// This is the entrypoint for the program.
program_entrypoint!(process_instruction);
default_allocator!();

#[inline(always)]
fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let (ix_disc, instruction_data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;

    match ix_disc {
        0 => {
            msg!("Ix:0");

            initialize(accounts, instruction_data)?;
            Ok(())
        }
        1 => {
            msg!("Ix:1");
            update_quotes(accounts, instruction_data)?;

            Ok(())
        }
        _ => return Err(ProgramError::InvalidInstructionData),
    }
}
pub static PHOENIX_STRATEGY_SEED: &[u8] = b"phoenix_strategy";
/*
create a strategy account that will save our bot config
*/
pub fn initialize(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let [phoenix_strategy_account, user, market, system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let params = try_from_bytes::<StrategyParams>(&data).unwrap();
    let clock = Clock::get()?;

    let phoenix_strategy = PhoenixStrategyState {
        trader: *user.key(),
        market: *market.key(),
        bid_order_sequence_number: 0,
        bid_price_in_ticks: 0,
        initial_bid_size_in_base_lots: 0,
        ask_order_sequence_number: 0,
        ask_price_in_ticks: 0,
        initial_ask_size_in_base_lots: 0,
        last_update_slot: clock.slot,
        last_update_unix_timestamp: clock.unix_timestamp,
        quote_edge_in_bps: params.quote_edge_in_bps,
        quote_size_in_quote_atoms: params.quote_size_in_quote_atoms,
        post_only: params.post_only,
        price_improvement_behavior: params.price_improvement_behavior,
        padding: [0; 6],
    };
    //create phoniex strategy account
    let space = core::mem::size_of::<PhoenixStrategyState>();
    let lamports = Rent::get()?.minimum_balance(space);
    let seeds: [&[u8]; 2] = [b"phoenix_strategy".as_ref(), user.key().as_ref()];

    let bump = find_program_address(&seeds, &crate::ID).1;

    let bump = [bump];
    let seeds = [
        Seed::from(PHOENIX_STRATEGY_SEED),
        Seed::from(user.key().as_ref()),
        Seed::from(&bump),
    ];

    let signers: [Signer; 1] = [Signer::from(&seeds)];

    CreateAccount {
        from: user,
        to: phoenix_strategy_account,
        space: core::mem::size_of::<PhoenixStrategyState>() as u64,
        owner: &crate::ID,
        lamports,
    }
    .invoke_signed(&signers)?;

    let mut dst = phoenix_strategy_account.try_borrow_mut_data()?;
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&phoenix_strategy as *const PhoenixStrategyState) as *const u8,
            size_of::<PhoenixStrategyState>(),
        )
    };
    dst[..size_of::<PhoenixStrategyState>()].copy_from_slice(bytes);
    Ok(())
}
pub fn update_quotes(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let mut logger = Logger::<100>::default();
    let [
        phoniex_strategy,
        pool,
        user,
        phoenix_program,
        phoenix_log_auth,
        seat,
        base_account,
        quote_account,
        base_vault,
        quote_vault,
        token_program,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let clock = Clock::get()?;
    //OrderParams
    let params = try_from_bytes::<OrderParams>(&data).unwrap();

    //Strategy Account
    let mut phoenix_strategy =
        *try_from_bytes::<PhoenixStrategyState>(&phoniex_strategy.try_borrow_data()?).unwrap();
    //track last update
    phoenix_strategy.last_update_slot = clock.slot;
    phoenix_strategy.last_update_unix_timestamp = clock.unix_timestamp;

    let edge = params.strategy_params.quote_edge_in_bps;
    if edge > 0 {
        phoenix_strategy.quote_edge_in_bps = edge; //how far from mid-price to quote
    }
    phoenix_strategy.quote_size_in_quote_atoms = params.strategy_params.quote_size_in_quote_atoms; //order size
    phoenix_strategy.post_only = params.strategy_params.post_only;
    phoenix_strategy.price_improvement_behavior = params.strategy_params.price_improvement_behavior; //undercut competitors or stay passive.
    let market_data = pool.try_borrow_data()?;
    let market_header = deserialize_market_header(pool)?;
    let market = deserialize_market(&market_data, &market_header.market_size_params)?;
    // Compute quote prices
    //each phoniex market has a tick size (min inc allowed )
    // the price where the bot will buy
    // fair_price_in_ticks=(fair price *raw base units)/tick_size
    // edge_in_ticks = edge_in_bps * fair_price_in_ticks / 10_000;
    let mut bid_price_in_ticks = get_bid_price_in_ticks(
        params.fair_price_in_quote_atoms_per_raw_base_unit,
        &market_header,
        phoenix_strategy.quote_edge_in_bps,
    );
    //simiilar math
    //the price where the bot will sell
    let mut ask_price_in_ticks = get_ask_price_in_ticks(
        params.fair_price_in_quote_atoms_per_raw_base_unit,
        &market_header,
        phoenix_strategy.quote_edge_in_bps,
    );
    //Bid=fair_price*(1-edge_bps/10_000)
    //Ask=fair_price*(1+edge_bps/10_000)
    // Returns the best bid and ask prices that are not placed by the trader
    let trader_index = market.get_trader_index(user.key()).unwrap_or(u32::MAX) as u64;
    let (best_bid, best_ask) = get_best_bid_and_ask(market, trader_index);
    logger.append("Current Market: ");
    logger.log();
    logger.clear();
    logger.append("Best Bid: ");
    logger.append_with_args(best_bid, &[Argument::Precision(0)]);
    logger.log();
    logger.clear();
    logger.append("Best Ask: ");
    logger.append_with_args(best_ask, &[Argument::Precision(0)]);
    logger.log();
    logger.clear();

    let price_improvement_behavior =
        PriceImprovementBehavior::from_u8(phoenix_strategy.price_improvement_behavior);
    match price_improvement_behavior {
        PriceImprovementBehavior::Join => {
            // If price_improvement_behavior is set to Join, we will always join the best bid and ask
            // if our quote prices are within the spread
            ask_price_in_ticks = ask_price_in_ticks.max(best_ask);
            bid_price_in_ticks = bid_price_in_ticks.min(best_bid);
        }
        PriceImprovementBehavior::Dime => {
            // If price_improvement_behavior is set to Dime, we will never price improve by more than 1 tick
            ask_price_in_ticks = ask_price_in_ticks.max(best_ask - 1);
            bid_price_in_ticks = bid_price_in_ticks.min(best_bid + 1);
        }
        PriceImprovementBehavior::Ignore => {
            // If price_improvement_behavior is set to Ignore, we will not update our quotes based off the current
            // market prices
        }
    }
    // Compute quote amounts in base lots
    let size_in_quote_lots =
        phoenix_strategy.quote_size_in_quote_atoms / market_header.quote_lot_size;

    //size_in_base_lots=(quote_lots*base_lots+per_unit)/(price_in_ticks*tick_size);
    let bid_size_in_base_lots = size_in_quote_lots * market.get_base_lots_per_base_unit()
        / (bid_price_in_ticks * market.get_tick_size());
    let ask_size_in_base_lots = size_in_quote_lots * market.get_base_lots_per_base_unit()
        / (ask_price_in_ticks * market.get_tick_size());

    logger.append("Our Market: ");
    logger.log();
    logger.clear();
    logger.append_with_args(bid_size_in_base_lots, &[Argument::Precision(0)]);
    logger.log();
    logger.clear();
    logger.append_with_args(bid_price_in_ticks, &[Argument::Precision(0)]);
    logger.log();
    logger.clear();
    logger.append_with_args(ask_price_in_ticks, &[Argument::Precision(0)]);
    logger.log();
    logger.clear();
    logger.append_with_args(ask_size_in_base_lots, &[Argument::Precision(0)]);
    logger.log();
    logger.clear();
    let mut update_bid = true;
    let mut update_ask = true;
    let orders_to_cancel = [
        (
            Side::Bid,
            bid_price_in_ticks,
            FIFOOrderId::new_from_untyped(
                phoenix_strategy.bid_price_in_ticks,
                phoenix_strategy.bid_order_sequence_number,
            ),
            phoenix_strategy.initial_bid_size_in_base_lots,
        ),
        (
            Side::Ask,
            ask_price_in_ticks,
            FIFOOrderId::new_from_untyped(
                phoenix_strategy.ask_price_in_ticks,
                phoenix_strategy.ask_order_sequence_number,
            ),
            phoenix_strategy.initial_ask_size_in_base_lots,
        ),
    ]
    .iter()
    .filter_map(|(side, price, order_id, initial_size)| {
        if let Some(resting_order) = market.get_book(*side).get(order_id) {
            // The order is 100% identical, do not cancel it
            if resting_order.num_base_lots == *initial_size
                && order_id.price_in_ticks.inner == *price
            {
                match side {
                    Side::Bid => update_bid = false,
                    Side::Ask => update_ask = false,
                }
                return None;
            }
            // The order has been partially filled or reduced
            logger.append("Found partially filled resting order with sequence number: ");
            logger.append_with_args(order_id.order_sequence_number, &[Argument::Precision(0)]);
            logger.log();
            logger.clear();
            return Some(*order_id);
        }
        logger.append("Failed to found resting order with sequence number: ");
        logger.append_with_args(order_id.order_sequence_number, &[Argument::Precision(0)]);
        logger.log();
        logger.clear();
        // The order has been fully filled
        None
    })
    .collect::<Vec<FIFOOrderId>>();

    // Drop reference prior to invoking
    drop(market_data);
    // Cancel the old orders
    if !orders_to_cancel.is_empty() {
        //cpi to create_cancel_multiple_orders_by_id_with_free_funds_instruction
        let params = &CancelMultipleOrdersByIdParams {
            orders: orders_to_cancel
                .iter()
                .map(|o_id| CancelOrderParams {
                    order_sequence_number: o_id.order_sequence_number,
                    price_in_ticks: o_id.price_in_ticks.inner,
                    side: Side::from_order_sequence_number(o_id.order_sequence_number),
                })
                .collect::<Vec<_>>(),
        };
        create_cancel_multiple_orders_by_id_with_free_funds_instruction(
            phoenix_program,
            phoenix_log_auth,
            pool,
            user,
            params,
        )?;
    }
    // Don't update quotes if the price is invalid or if the sizes are 0
    update_bid &= bid_price_in_ticks > 1 && bid_size_in_base_lots > 0;
    update_ask &= ask_price_in_ticks < u64::MAX && ask_size_in_base_lots > 0;
    let client_order_id = u128::from_le_bytes(accounts[2].key()[..16].try_into().unwrap());

    if !update_ask && !update_bid && orders_to_cancel.is_empty() {
        msg!("No orders to update");
        return Ok(());
    }
    let order_ids: Vec<FIFOOrderId> = vec![];
    if phoenix_strategy.post_only == 1
        || !matches!(price_improvement_behavior, PriceImprovementBehavior::Join)
    {
        // Send multiple post-only orders in a single instruction
        let multiple_order_packet = MultipleOrderPacket::new(
            if update_bid {
                vec![CondensedOrder::new_default(
                    bid_price_in_ticks,
                    bid_size_in_base_lots,
                )]
            } else {
                vec![]
            },
            if update_ask {
                vec![CondensedOrder::new_default(
                    ask_price_in_ticks,
                    ask_size_in_base_lots,
                )]
            } else {
                vec![]
            },
            Some(client_order_id),
            false,
        );
        msg!("cpi to place multipule post only orders");
        //cpi to place multipule post only orders
        create_new_multiple_order_with_custom_token_accounts(
            phoenix_program,
            pool,
            user,
            seat,
            base_account,
            quote_account,
            base_vault,
            quote_vault,
            phoenix_log_auth,
            token_program,
            &multiple_order_packet,
        )?;
    } else {
        if update_bid {
            msg!("update bid and create_new_order_with_custom_token_accounts");
            create_new_order_with_custom_token_accounts(
                phoenix_program,
                pool,
                user,
                seat,
                base_account,
                quote_account,
                base_vault,
                quote_vault,
                phoenix_log_auth,
                token_program,
                &OrderPacket::new_limit_order_default_with_client_order_id(
                    Side::Bid,
                    bid_price_in_ticks,
                    bid_size_in_base_lots,
                    client_order_id,
                ),
            )?;
        }
        if update_ask {
            msg!("update ask and create_new_order_with_custom_token_accounts");
            create_new_order_with_custom_token_accounts(
                phoenix_program,
                pool,
                user,
                seat,
                base_account,
                quote_account,
                base_vault,
                quote_vault,
                phoenix_log_auth,
                token_program,
                &OrderPacket::new_limit_order_default_with_client_order_id(
                    Side::Ask,
                    ask_price_in_ticks,
                    ask_size_in_base_lots,
                    client_order_id,
                ),
            )?;
        }
    }

    let market_data = pool.try_borrow_data()?;
    let market = deserialize_market(&market_data, &market_header.market_size_params)?;

    for order_id in order_ids.iter() {
        if let Some(order) = market
            .get_book(Side::from_order_sequence_number(
                order_id.order_sequence_number,
            ))
            .get(order_id)
        {
            match Side::from_order_sequence_number(order_id.order_sequence_number) {
                Side::Ask => {
                    phoenix_strategy.ask_price_in_ticks = order_id.price_in_ticks.inner;
                    phoenix_strategy.ask_order_sequence_number = order_id.order_sequence_number;
                    phoenix_strategy.initial_ask_size_in_base_lots = order.num_base_lots;
                    msg!("Placed Ask Order with sequence number: ");
                }
                Side::Bid => {
                    phoenix_strategy.bid_price_in_ticks = order_id.price_in_ticks.inner;
                    phoenix_strategy.bid_order_sequence_number = order_id.order_sequence_number;
                    phoenix_strategy.initial_bid_size_in_base_lots = order.num_base_lots;
                    msg!("Placed Ask Order with sequence number: ");
                }
            }
        } else {
            msg!("Order not found ");
        }
        logger.append_with_args(order_id.order_sequence_number, &[Argument::Precision(0)]);
        logger.log();
        logger.clear()
    }

    Ok(())
}
