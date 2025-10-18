use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use core::fmt::Debug;
use pinocchio::pubkey::Pubkey;
use sokoban::node_allocator::{OrderedNodeAllocatorMap, ZeroCopy};
use sokoban::{RedBlackTree, SENTINEL};
#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
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
    pub padding: [u8; 6],
}

#[derive(BorshDeserialize, BorshSerialize, Copy, Clone, PartialEq, Eq)]
pub enum SelfTradeBehavior {
    Abort,
    CancelProvide,
    DecrementTake,
}
#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, PartialEq, Eq)]
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

#[derive(BorshDeserialize, BorshSerialize, Copy, Clone, PartialEq, Eq)]
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
    pub padding: [u8; 6],
}
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct OrderParams {
    pub fair_price_in_quote_atoms_per_raw_base_unit: u64,
    pub strategy_params: StrategyParams,
}
pub trait OrderId {}
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy, PartialOrd, Ord, Zeroable, Pod)]
#[repr(transparent)]
pub struct Ticks {
    pub inner: u64,
}

#[repr(C)]
#[derive(Eq, PartialEq, Default, Copy, Clone, Debug, Zeroable, Pod, PartialOrd, Ord)]
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

impl OrderId for FIFOOrderId {}
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
#[derive(Default, PartialEq, Eq, Clone, Copy, PartialOrd, Ord, Zeroable, Pod)]
#[repr(transparent)]
pub struct QuoteLots {
    pub inner: u64,
}
#[derive(Default, PartialEq, Eq, Clone, Copy, PartialOrd, Ord, Zeroable, Pod)]
#[repr(transparent)]
pub struct BaseLots {
    pub inner: u64,
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
#[derive(Default, Copy, Clone, PartialEq, Eq, Zeroable, Pod)]
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
    MarketTraderId: PartialOrd + Ord + Default + Copy + Clone + Zeroable + Pod + Debug,
    const BIDS_SIZE: usize,
    const ASKS_SIZE: usize,
    const NUM_SEATS: usize,
> Pod for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
}
impl<
    MarketTraderId: PartialOrd + Ord + Default + Copy + Clone + Zeroable + Pod + Debug,
    const BIDS_SIZE: usize,
    const ASKS_SIZE: usize,
    const NUM_SEATS: usize,
> ZeroCopy for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
}
impl<
    MarketTraderId: PartialOrd + Ord + Default + Copy + Clone + Zeroable + Pod + Debug,
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
    fn get_registered_traders(&self) -> &dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState> {
        &self.traders as &dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState>
    }
}

impl<
    MarketTraderId: Debug
        + PartialOrd
        + Ord
        + Default
        + Copy
        + Clone
        + Zeroable
        + Pod
        + BorshDeserialize
        + BorshSerialize,
    const BIDS_SIZE: usize,
    const ASKS_SIZE: usize,
    const NUM_SEATS: usize,
> WritableMarket<MarketTraderId, FIFOOrderId, FIFORestingOrder, OrderPacket>
    for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
    fn get_registered_traders_mut(
        &mut self,
    ) -> &mut dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState> {
        &mut self.traders as &mut dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState>
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
pub trait Market<
    MarketTraderId: Copy,
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
    fn get_registered_traders(&self) -> &dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState>;
}
pub trait WritableMarket<
    MarketTraderId: BorshDeserialize + BorshSerialize + Copy,
    MarketOrderId: OrderId,
    MarketRestingOrder: RestingOrder,
    MarketOrderPacket: OrderPacketMetadata,
>: Market<MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>
{
    fn get_registered_traders_mut(
        &mut self,
    ) -> &mut dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState>;
    fn get_or_register_trader(&mut self, trader: &MarketTraderId) -> Option<u32> {
        let registered_traders = self.get_registered_traders_mut();
        if !registered_traders.contains(trader) {
            registered_traders.insert(*trader, TraderState::default())?;
        }
        self.get_trader_index(trader)
    }
}

pub(crate) struct MarketWrapperMut<
    'a,
    MarketTraderId,
    MarketOrderId,
    MarketRestingOrder,
    MarketOrderPacket,
> {
    pub inner: &'a mut dyn WritableMarket<MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>,
}

impl<'a, MarketTraderId, MarketOrderPacket, MarketRestingOrder, MarketOrderId>
    MarketWrapperMut<'a, MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>
{
    pub(crate) fn new(
        market: &'a mut dyn WritableMarket<
            MarketTraderId,
            MarketOrderId,
            MarketRestingOrder,
            MarketOrderPacket,
        >,
    ) -> Self {
        Self { inner: market }
    }
}
pub fn get_best_bid_and_ask(
    market: &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
    trader_index: u64,
) -> (u64, u64) {
    let best_bid = market
        .get_book(Side::Bid) // Get the bid order book
        .iter() // Iterate through all bids
        .find(|(_, o)| o.trader_index != trader_index) // Find the first order NOT from this trader
        .map(|(o, _)| o.price_in_ticks.inner) // Get the price in ticks
        .unwrap_or_else(|| 1); // Default to 1 if no order is found

    let best_ask = market
        .get_book(Side::Ask) // Get the ask order book
        .iter() // Iterate through all asks
        .find(|(_, o)| o.trader_index != trader_index) // Skip your own orders
        .map(|(o, _)| o.price_in_ticks.inner) // Extract the price
        .unwrap_or_else(|| u64::MAX); // Default to max if no order is found

    (best_bid, best_ask)
}
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
#[derive(BorshDeserialize, BorshSerialize)]
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
#[derive(BorshDeserialize, BorshSerialize, Clone)]
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
#[derive(BorshDeserialize, BorshSerialize)]
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
