#![allow(unexpected_cfgs)]

use arrayvec::ArrayVec;
//use std::Vec;
use crate::utils::*;
use borsh::{BorshDeserialize, BorshSerialize};
use core::fmt::Debug;
use pinocchio_log::logger::{Argument, Log, Logger};
use pinocchio_system::instructions::CreateAccount;
//use crate::instruction::{self, MyProgramInstruction};
use bytemuck::{Pod, Zeroable, checked::try_from_bytes};
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

use sokoban::RedBlackTree;
use sokoban::node_allocator::{OrderedNodeAllocatorMap, SENTINEL, ZeroCopy};
// This is the entrypoint for the program.
program_entrypoint!(process_instruction);
default_allocator!();

//the project is a copy of phoniex mm by elipsis labs
#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
//24
pub struct MarketSizeParams {
    pub bids_size: u64,
    pub asks_size: u64,
    pub num_seats: u64,
}
impl ZeroCopy for MarketSizeParams {}
#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
//80
pub struct TokenParams {
    /// Number of decimals for the token (e.g. 9 for SOL, 6 for USDC).
    pub decimals: u32,

    /// Bump used for generating the PDA for the market's token vault.
    pub vault_bump: u32,

    /// Pubkey of the token mint.
    pub mint_key: Pubkey,

    /// Pubkey of the token vault.
    pub vault_key: Pubkey,
}
impl ZeroCopy for TokenParams {}
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
//8+8+24+80+8+80+8+32+32+8+32+ 4+ 4+ 8*32
pub struct MarketHeader {
    pub discriminant: u64,
    pub status: u64,
    pub market_size_params: MarketSizeParams,
    pub base_params: TokenParams,
    pub base_lot_size: u64,
    pub quote_params: TokenParams,
    pub quote_lot_size: u64,
    pub tick_size_in_quote_atoms_per_base_unit: u64,
    pub authority: Pubkey,
    pub fee_recipient: Pubkey,
    pub market_sequence_number: u64,
    pub successor: Pubkey,
    pub raw_base_units_per_base_unit: u32,
    _padding1: u32,
    _padding2: [u64; 32],
}
impl ZeroCopy for MarketHeader {}
impl MarketHeader {}
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
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

#[derive(BorshDeserialize, BorshSerialize, Copy, Clone, PartialEq, Eq, Debug)]
pub enum SelfTradeBehavior {
    Abort,
    CancelProvide,
    DecrementTake,
}
#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Bid,
    Ask,
}
impl Side {
    pub fn opposite(&self) -> Self {
        match *self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }

    pub fn from_order_sequence_number(order_id: u64) -> Self {
        match order_id.leading_zeros() {
            0 => Side::Bid,
            _ => Side::Ask,
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize, Copy, Clone, PartialEq, Eq, Debug)]
pub enum OrderPacket {
    PostOnly {
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        client_order_id: u128,
        reject_post_only: bool,
        use_only_deposited_funds: bool,
        last_valid_slot: Option<u64>,
        last_valid_unix_timestamp_in_seconds: Option<u64>,
        fail_silently_on_insufficient_funds: bool,
    },
    Limit {
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
        last_valid_slot: Option<u64>,
        last_valid_unix_timestamp_in_seconds: Option<u64>,
        fail_silently_on_insufficient_funds: bool,
    },
    ImmediateOrCancel {
        side: Side,
        price_in_ticks: Option<u64>,
        num_base_lots: u64,
        num_quote_lots: u64,
        min_base_lots_to_fill: u64,
        min_quote_lots_to_fill: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
        last_valid_slot: Option<u64>,
        last_valid_unix_timestamp_in_seconds: Option<u64>,
    },
}
impl OrderPacket {
    pub fn new_limit_order_default_with_client_order_id(
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        client_order_id: u128,
    ) -> Self {
        Self::new_limit_order(
            side,
            price_in_ticks,
            num_base_lots,
            SelfTradeBehavior::CancelProvide,
            None,
            client_order_id,
            false,
        )
    }

    pub fn new_limit_order(
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::Limit {
            side,
            price_in_ticks: price_in_ticks,
            num_base_lots: num_base_lots,
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
            fail_silently_on_insufficient_funds: false,
        }
    }
}
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct StrategyParams {
    pub quote_edge_in_bps: u64,
    pub quote_size_in_quote_atoms: u64,
    pub price_improvement_behavior: u8, //0 ->join,1->Dime,2->Ignore
    pub post_only: u8,
    padding: [u8; 6],
}
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct OrderParams {
    pub fair_price_in_quote_atoms_per_raw_base_unit: u64,
    pub strategy_params: StrategyParams,
}
fn get_bid_price_in_ticks(
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

fn get_ask_price_in_ticks(
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
pub trait OrderId {
    fn price_in_ticks(&self) -> u64;
}
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy, PartialOrd, Ord, Zeroable, Pod)]
#[repr(transparent)]
pub struct Ticks {
    inner: u64,
}

#[repr(C)]
#[derive(Eq, PartialEq, Debug, Default, Copy, Clone, Zeroable, Pod, PartialOrd, Ord)]
pub struct FIFOOrderId {
    /// The price of the order, in ticks. Each market has a designated
    /// tick size (some number of quote lots per base unit) that is used to convert the price to ticks.
    /// For example, if the tick size is 0.01, then a price of 1.23 is converted to 123 ticks.
    /// If the quote lot size is 0.001, this means that there is a spacing of 10 quote lots
    /// in between each tick.
    pub price_in_ticks: Ticks,

    /// This is the unique identifier of the order, which is used to determine the side of the order.
    /// It is derived from the sequence number of the market.
    ///
    /// If the order is a bid, the sequence number will have its bits inverted, and if it is an ask,
    /// the sequence number will be used as is.
    ///
    /// The way to identify the side of the order is to check the leading bit of `order_id`.
    /// A leading bit of 0 indicates an ask, and a leading bit of 1 indicates a bid. See Side::from_order_id.
    pub order_sequence_number: u64,
}

impl OrderId for FIFOOrderId {
    fn price_in_ticks(&self) -> u64 {
        self.price_in_ticks.inner
    }
}
impl FIFOOrderId {
    pub fn new_from_untyped(price_in_ticks: u64, order_sequence_number: u64) -> Self {
        FIFOOrderId {
            price_in_ticks: Ticks {
                inner: price_in_ticks,
            },
            order_sequence_number,
        }
    }
}
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy, PartialOrd, Ord, Zeroable, Pod)]
#[repr(transparent)]
pub struct QuoteLots {
    inner: u64,
}
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy, PartialOrd, Ord, Zeroable, Pod)]
#[repr(transparent)]
pub struct BaseLots {
    inner: u64,
}

#[repr(C)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Zeroable, Pod)]
pub struct FIFORestingOrder {
    pub trader_index: u64,
    pub num_base_lots: u64, // Number of base lots quoted
    pub last_valid_slot: u64,
    pub last_valid_unix_timestamp_in_seconds: u64,
}
impl RestingOrder for FIFORestingOrder {
    fn size(&self) -> u64 {
        self.num_base_lots
    }

    fn last_valid_slot(&self) -> Option<u64> {
        if self.last_valid_slot == 0 {
            None
        } else {
            Some(self.last_valid_slot)
        }
    }

    fn last_valid_unix_timestamp_in_seconds(&self) -> Option<u64> {
        if self.last_valid_unix_timestamp_in_seconds == 0 {
            None
        } else {
            Some(self.last_valid_unix_timestamp_in_seconds)
        }
    }

    fn is_expired(&self, current_slot: u64, current_unix_timestamp_in_seconds: u64) -> bool {
        (self.last_valid_slot != 0 && self.last_valid_slot < current_slot)
            || (self.last_valid_unix_timestamp_in_seconds != 0
                && self.last_valid_unix_timestamp_in_seconds < current_unix_timestamp_in_seconds)
    }
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Zeroable, Pod)]
pub struct TraderState {
    pub quote_lots_locked: QuoteLots,
    pub quote_lots_free: QuoteLots,
    pub base_lots_locked: BaseLots,
    pub base_lots_free: BaseLots,
    _padding: [u64; 8],
}
#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable)]
pub struct FIFOMarket<
    MarketTraderId: PartialOrd + Ord + Default + Copy + Clone + Zeroable + Pod + Debug,
    const BIDS_SIZE: usize,
    const ASKS_SIZE: usize,
    const NUM_SEATS: usize,
> {
    /// Padding
    pub _padding: [u64; 32],

    /// Number of base lots in a base unit. For example, if the lot size is 0.001 SOL, then base_lots_per_base_unit is 1000.
    pub base_lots_per_base_unit: u64,

    /// Tick size in quote lots per base unit. For example, if the tick size is 0.01 USDC and the quote lot size is 0.001 USDC, then tick_size_in_quote_lots_per_base_unit is 10.
    pub tick_size_in_quote_lots_per_base_unit: u64,

    /// The sequence number of the next event.
    order_sequence_number: u64,

    /// There are no maker fees. Taker fees are charged on the quote lots transacted in the trade, in basis points.
    pub taker_fee_bps: u64,

    /// Amount of fees collected from the market in its lifetime, in quote lots.
    collected_quote_lot_fees: QuoteLots,

    /// Amount of unclaimed fees accrued to the market, in quote lots.
    unclaimed_quote_lot_fees: QuoteLots,

    /// Red-black tree representing the bids in the order book.
    pub bids: RedBlackTree<FIFOOrderId, FIFORestingOrder, BIDS_SIZE>,

    /// Red-black tree representing the asks in the order book.
    pub asks: RedBlackTree<FIFOOrderId, FIFORestingOrder, ASKS_SIZE>,

    /// Red-black tree representing the authorized makers in the market.
    pub traders: RedBlackTree<MarketTraderId, TraderState, NUM_SEATS>,
}
unsafe impl<
    MarketTraderId: Debug + PartialOrd + Ord + Default + Copy + Clone + Zeroable + Pod,
    const BIDS_SIZE: usize,
    const ASKS_SIZE: usize,
    const NUM_SEATS: usize,
> Pod for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
}
impl<
    MarketTraderId: Debug + PartialOrd + Ord + Default + Copy + Clone + Zeroable + Pod,
    const BIDS_SIZE: usize,
    const ASKS_SIZE: usize,
    const NUM_SEATS: usize,
> ZeroCopy for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
}
impl<
    MarketTraderId: Debug + PartialOrd + Ord + Default + Copy + Clone + Zeroable + Pod,
    const BIDS_SIZE: usize,
    const ASKS_SIZE: usize,
    const NUM_SEATS: usize,
> Market<MarketTraderId, FIFOOrderId, FIFORestingOrder, OrderPacket>
    for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
    fn get_base_lots_per_base_unit(&self) -> u64 {
        self.base_lots_per_base_unit
    }

    #[inline(always)]
    fn get_book(&self, side: Side) -> &dyn OrderedNodeAllocatorMap<FIFOOrderId, FIFORestingOrder> {
        match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        }
    }
    fn get_tick_size(&self) -> u64 {
        self.tick_size_in_quote_lots_per_base_unit
    }
    #[inline(always)]
    fn get_trader_index(&self, trader_id: &MarketTraderId) -> Option<u32> {
        let addr = self.traders.get_addr(trader_id);
        if addr == SENTINEL { None } else { Some(addr) }
    }
}

pub struct MarketWrapper<'a, MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket> {
    pub inner: &'a dyn Market<MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>,
}
impl<'a, MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>
    MarketWrapper<'a, MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>
{
    pub fn new(
        market: &'a dyn Market<MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>,
    ) -> Self {
        Self { inner: market }
    }
}

pub trait RestingOrder {
    fn size(&self) -> u64;
    fn last_valid_slot(&self) -> Option<u64>;
    fn last_valid_unix_timestamp_in_seconds(&self) -> Option<u64>;
    fn is_expired(&self, current_slot: u64, current_unix_timestamp_in_seconds: u64) -> bool;
}
pub trait OrderPacketMetadata {
    fn is_take_only(&self) -> bool {
        self.is_ioc() || self.is_fok()
    }

    fn is_ioc(&self) -> bool;
    fn is_fok(&self) -> bool;

    fn is_post_only(&self) -> bool;
    fn no_deposit_or_withdrawal(&self) -> bool;
}
impl OrderPacketMetadata for OrderPacket {
    fn is_ioc(&self) -> bool {
        matches!(self, OrderPacket::ImmediateOrCancel { .. })
    }

    fn is_fok(&self) -> bool {
        match self {
            &Self::ImmediateOrCancel {
                num_base_lots,
                num_quote_lots,
                min_base_lots_to_fill,
                min_quote_lots_to_fill,
                ..
            } => {
                num_base_lots > 0 && num_base_lots == min_base_lots_to_fill
                    || num_quote_lots > 0 && num_quote_lots == min_quote_lots_to_fill
            }
            _ => false,
        }
    }

    fn is_post_only(&self) -> bool {
        matches!(self, OrderPacket::PostOnly { .. })
    }
    fn no_deposit_or_withdrawal(&self) -> bool {
        match *self {
            Self::PostOnly {
                use_only_deposited_funds,
                ..
            } => use_only_deposited_funds,
            Self::Limit {
                use_only_deposited_funds,
                ..
            } => use_only_deposited_funds,
            Self::ImmediateOrCancel {
                use_only_deposited_funds,
                ..
            } => use_only_deposited_funds,
        }
    }
}
pub trait Market<
    MarketTraderId: Copy + Debug,
    MarketOrderId: OrderId,
    MarketRestingOrder: RestingOrder,
    MarketOrderPacket: OrderPacketMetadata,
>
{
    fn get_data_size(&self) -> usize {
        unimplemented!()
    }
    fn get_collected_fee_amount(&self) -> QuoteLots {
        unimplemented!()
    }
    fn get_uncollected_fee_amount(&self) -> QuoteLots {
        unimplemented!()
    }
    fn get_tick_size(&self) -> u64;

    fn get_book(
        &self,
        side: Side,
    ) -> &dyn OrderedNodeAllocatorMap<MarketOrderId, MarketRestingOrder>;
    fn get_trader_index(&self, trader: &MarketTraderId) -> Option<u32>;
    fn get_base_lots_per_base_unit(&self) -> u64;
}
fn get_best_bid_and_ask(
    market: &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
    trader_index: u64,
) -> (u64, u64) {
    let best_bid = market
        .get_book(Side::Bid)
        .iter()
        .find(|(_, o)| o.trader_index != trader_index)
        .map(|(o, _)| o.price_in_ticks.inner)
        .unwrap_or_else(|| 1);
    let best_ask = market
        .get_book(Side::Ask)
        .iter()
        .find(|(_, o)| o.trader_index != trader_index)
        .map(|(o, _)| o.price_in_ticks.inner)
        .unwrap_or_else(|| u64::MAX);
    (best_bid, best_ask)
}

#[derive(Debug)]
pub enum PriceImprovementBehavior {
    Join,
    Dime,
    Ignore,
}
impl PriceImprovementBehavior {
    pub fn to_u8(&self) -> u8 {
        match self {
            PriceImprovementBehavior::Join => 0,
            PriceImprovementBehavior::Dime => 1,
            PriceImprovementBehavior::Ignore => 2,
        }
    }

    pub fn from_u8(byte: u8) -> Self {
        match byte {
            0 => PriceImprovementBehavior::Join,
            1 => PriceImprovementBehavior::Dime,
            2 => PriceImprovementBehavior::Ignore,
            _ => panic!("Invalid PriceImprovementBehavior"),
        }
    }
}
#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct CancelMultipleOrdersByIdParams {
    pub orders: Vec<CancelOrderParams>,
}
#[derive(BorshDeserialize, BorshSerialize, Clone, Copy)]
pub struct CancelOrderParams {
    pub side: Side,
    pub price_in_ticks: u64,
    pub order_sequence_number: u64,
}
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub enum FailedMultipleLimitOrderBehavior {
    /// Orders will never cross the spread. Instead they will be amended to the closest non-crossing price.
    /// The entire transaction will fail if matching engine returns None for any order, which indicates an error.
    ///
    /// If an order has insufficient funds, the entire transaction will fail.
    FailOnInsufficientFundsAndAmendOnCross,

    /// If any order crosses the spread or has insufficient funds, the entire transaction will fail.
    FailOnInsufficientFundsAndFailOnCross,

    /// Orders will be skipped if the user has insufficient funds.
    /// Crossing orders will be amended to the closest non-crossing price.
    SkipOnInsufficientFundsAndAmendOnCross,

    /// Orders will be skipped if the user has insufficient funds.
    /// If any order crosses the spread, the entire transaction will fail.
    SkipOnInsufficientFundsAndFailOnCross,
}
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct CondensedOrder {
    pub price_in_ticks: u64,
    pub size_in_base_lots: u64,
    pub last_valid_slot: Option<u64>,
    pub last_valid_unix_timestamp_in_seconds: Option<u64>,
}
impl CondensedOrder {
    pub fn new_default(price_in_ticks: u64, size_in_base_lots: u64) -> Self {
        CondensedOrder {
            price_in_ticks,
            size_in_base_lots,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        }
    }
}
/// Struct to send a vector of bids and asks as PostOnly orders in a single packet.
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct MultipleOrderPacket {
    /// Bids and asks are in the format (price in ticks, size in base lots)
    pub bids: Vec<CondensedOrder>,
    pub asks: Vec<CondensedOrder>,
    pub client_order_id: Option<u128>,
    pub failed_multiple_limit_order_behavior: FailedMultipleLimitOrderBehavior,
}
impl MultipleOrderPacket {
    pub fn new(
        bids: Vec<CondensedOrder>,
        asks: Vec<CondensedOrder>,
        client_order_id: Option<u128>,
        reject_post_only: bool,
    ) -> Self {
        MultipleOrderPacket {
            bids,
            asks,
            client_order_id,
            failed_multiple_limit_order_behavior: if reject_post_only {
                FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross
            } else {
                FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndAmendOnCross
            },
        }
    }
}

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
pub static PHOENIX_STRATEGY_SEED: &[u8] = b"phoniex_strategy";
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
    let seeds: [&[u8]; 2] = [b"phoniex_strategy".as_ref(), user.key().as_ref()];

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
    let mut bid_price_in_ticks = get_bid_price_in_ticks(
        params.fair_price_in_quote_atoms_per_raw_base_unit,
        &market_header,
        phoenix_strategy.quote_edge_in_bps,
    );

    let mut ask_price_in_ticks = get_ask_price_in_ticks(
        params.fair_price_in_quote_atoms_per_raw_base_unit,
        &market_header,
        phoenix_strategy.quote_edge_in_bps,
    );

    // Returns the best bid and ask prices that are not placed by the trader
    let trader_index = market.get_trader_index(user.key()).unwrap_or(u32::MAX) as u64;
    let (best_bid, best_ask) = get_best_bid_and_ask(market, trader_index);
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

    let bid_size_in_base_lots = size_in_quote_lots * market.get_base_lots_per_base_unit()
        / (bid_price_in_ticks * market.get_tick_size());
    let ask_size_in_base_lots = size_in_quote_lots * market.get_base_lots_per_base_unit()
        / (ask_price_in_ticks * market.get_tick_size());
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
            return Some(*order_id);
        }
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
            phoenix_program,  //phoniex
            phoenix_log_auth, //phoniex_log_auth
            pool,             //pool
            user,             //user
            params,
        );
    }
    // Don't update quotes if the price is invalid or if the sizes are 0
    update_bid &= bid_price_in_ticks > 1 && bid_size_in_base_lots > 0;
    update_ask &= ask_price_in_ticks < u64::MAX && ask_size_in_base_lots > 0;
    let client_order_id = u128::from_le_bytes(accounts[2].key()[..16].try_into().unwrap());
    if !update_ask && !update_bid && orders_to_cancel.is_empty() {
        msg!("No orders to update");
        return Ok(());
    }
    let mut order_ids: Vec<FIFOOrderId> = vec![];
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
        );
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
            );
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
            );
        }
    }

    let market_data = pool.try_borrow_data()?;
    let market = deserialize_market(&market_data, &market_header.market_size_params)?;

    for order_id in order_ids.iter() {
        let side = Side::from_order_sequence_number(order_id.order_sequence_number);
        match side {
            Side::Ask => {
                market
                    .get_book(Side::Ask)
                    .get(&order_id)
                    .map(|order| {
                        phoenix_strategy.ask_price_in_ticks = order_id.price_in_ticks.inner;
                        phoenix_strategy.ask_order_sequence_number = order_id.order_sequence_number;
                        phoenix_strategy.initial_ask_size_in_base_lots = order.num_base_lots;
                    })
                    .unwrap_or_else(|| {
                        msg!("Ask order not found");
                    });
            }
            Side::Bid => {
                market
                    .get_book(Side::Bid)
                    .get(&order_id)
                    .map(|order| {
                        phoenix_strategy.bid_price_in_ticks = order_id.price_in_ticks.inner;
                        phoenix_strategy.bid_order_sequence_number = order_id.order_sequence_number;
                        phoenix_strategy.initial_bid_size_in_base_lots = order.num_base_lots;
                    })
                    .unwrap_or_else(|| {
                        msg!("Bid order not found");
                    });
            }
        }
    }

    Ok(())
}
/*
let mut logger = Logger::<100>::default();
logger.append("expected size: ");
logger.append_with_args(
    core::mem::size_of::<MarketHeader>(),
    &[Argument::Precision(0)],
);
logger.log();
logger.clear();
logger.append("got size: ");
logger.append_with_args(market_data.len(), &[Argument::Precision(0)]);
logger.log();
logger.clear();
*/
