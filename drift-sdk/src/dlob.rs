use std::collections::{HashMap, HashSet};

use anchor_lang::prelude::Pubkey;
use drift::{
    controller::position::PositionDirection,
    error::DriftResult,
    state::{
        events::OrderRecord,
        user::{Order, OrderStatus, OrderTriggerCondition, OrderType},
        user_map::UserMap,
    },
};

use crate::{
    dlob_node::DLOBNodeType,
    dlob_orders::DLOBOrders,
    node_list::{get_order_signature, NodeList, SortDirection},
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Side {
    Bid,
    Ask,
}

// custom enum because the original doesn't impl Hash
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub enum MarketType {
    Spot,
    Perp,
}

impl From<drift::state::user::MarketType> for MarketType {
    fn from(market_type: drift::state::user::MarketType) -> Self {
        match market_type {
            drift::state::user::MarketType::Spot => MarketType::Spot,
            drift::state::user::MarketType::Perp => MarketType::Perp,
        }
    }
}

pub enum MarketNodeLists {
    RestingLimit(SideNodeList),
    FloatingLimit(SideNodeList),
    TakingLimit(SideNodeList),
    Market(SideNodeList),
    Trigger(TriggerNodeList),
}

#[derive(Debug, Clone)]
pub struct SideNodeList {
    pub ask: NodeList,
    pub bid: NodeList,
}

pub struct TriggerNodeList {
    pub above: NodeList,
    pub below: NodeList,
}

pub struct DLOB {
    open_orders: HashMap<MarketType, HashSet<String>>,
    order_lists: HashMap<MarketType, HashMap<u16, MarketNodeLists>>,
    max_slot_for_resting_limit_orders: u32,
    initialized: bool,
}

impl Default for DLOB {
    fn default() -> Self {
        let mut open_orders = HashMap::new();
        open_orders.insert(MarketType::Perp, HashSet::new());
        open_orders.insert(MarketType::Spot, HashSet::new());

        let mut order_lists = HashMap::new();
        order_lists.insert(MarketType::Perp, HashMap::new());
        order_lists.insert(MarketType::Spot, HashMap::new());

        Self {
            open_orders,
            order_lists,
            max_slot_for_resting_limit_orders: 0,
            initialized: false,
        }
    }
}

impl DLOB {
    pub fn new() -> DriftResult<Self> {
        Ok(DLOB::default())
    }

    pub fn initialize(&mut self) -> DriftResult<()> {
        self.initialized = true;
        Ok(())
    }

    pub fn clear(&mut self) -> DriftResult<()> {
        for market_type in self.open_orders.keys().cloned().collect::<Vec<_>>() {
            self.open_orders.get_mut(&market_type).unwrap().clear();
        }
        self.open_orders.clear();

        for market_type in self.order_lists.keys().cloned().collect::<Vec<_>>() {
            if let Some(market_node_lists_map) = self.order_lists.get_mut(&market_type) {
                for market_node_lists in market_node_lists_map.values_mut() {
                    match market_node_lists {
                        MarketNodeLists::RestingLimit(side_node_list)
                        | MarketNodeLists::FloatingLimit(side_node_list)
                        | MarketNodeLists::TakingLimit(side_node_list)
                        | MarketNodeLists::Market(side_node_list) => {
                            side_node_list.ask.clear();
                            side_node_list.bid.clear();
                        }
                        MarketNodeLists::Trigger(trigger_node_list) => {
                            trigger_node_list.above.clear();
                            trigger_node_list.below.clear();
                        }
                    }
                }
            }
        }
        self.order_lists.clear();

        self.max_slot_for_resting_limit_orders = 0;

        self.initialize()?;

        Ok(())
    }

    //TODO
    fn init_from_user_map(&mut self, user_map: UserMap, slot: u64) -> DriftResult<bool> {
        if self.initialized {
            return Ok(false);
        }

        Ok(true)
    }

    pub fn init_from_orders(&mut self, dlob_orders: DLOBOrders, slot: u64) -> DriftResult<bool> {
        if self.initialized {
            return Ok(false);
        }

        for dlob_order in dlob_orders {
            self.insert_order(dlob_order.order, dlob_order.user, slot)?;
        }

        self.initialize()?;
        Ok(true)
    }

    pub fn handle_order_record(&mut self, record: OrderRecord, slot: u64) -> DriftResult<()> {
        self.insert_order(record.order, record.user, slot)
    }

    pub fn insert_order(
        &mut self,
        order: Order,
        user_account: Pubkey,
        slot: u64,
    ) -> DriftResult<()> {
        if matches!(order.status, OrderStatus::Init) {
            return Ok(());
        }

        if !matches!(
            order.order_type,
            OrderType::Market
                | OrderType::Limit
                | OrderType::TriggerMarket
                | OrderType::TriggerLimit
                | OrderType::Oracle
        ) {
            return Ok(());
        }

        let market_type = order.market_type;

        if !self.order_lists.contains_key(&market_type.into()) {
            self.add_order_list(market_type.into(), order.market_index);
        }

        if matches!(order.status, OrderStatus::Open) {
            let order_signature = format!("{}-{}", user_account, order.order_id);
            self.open_orders
                .entry(market_type.into())
                .or_insert_with(HashSet::new)
                .insert(order_signature);
        }

        if let Some(mut list) = self.get_list_for_order(order, slot) {
            list.insert(order, user_account)?;
        }

        Ok(())
    }

    fn add_order_list(&mut self, market_type: MarketType, market_index: u16) {
        let resting_limit = MarketNodeLists::RestingLimit(SideNodeList {
            ask: NodeList::new(DLOBNodeType::RestingLimit, SortDirection::Asc),
            bid: NodeList::new(DLOBNodeType::RestingLimit, SortDirection::Desc),
        });
        let floating_limit = MarketNodeLists::FloatingLimit(SideNodeList {
            ask: NodeList::new(DLOBNodeType::FloatingLimit, SortDirection::Asc),
            bid: NodeList::new(DLOBNodeType::FloatingLimit, SortDirection::Desc),
        });
        let taking_limit = MarketNodeLists::TakingLimit(SideNodeList {
            ask: NodeList::new(DLOBNodeType::TakingLimit, SortDirection::Asc),
            bid: NodeList::new(DLOBNodeType::TakingLimit, SortDirection::Asc),
        });
        let market = MarketNodeLists::Market(SideNodeList {
            ask: NodeList::new(DLOBNodeType::Market, SortDirection::Asc),
            bid: NodeList::new(DLOBNodeType::Market, SortDirection::Asc),
        });
        let trigger = MarketNodeLists::Trigger(TriggerNodeList {
            above: NodeList::new(DLOBNodeType::Trigger, SortDirection::Asc),
            below: NodeList::new(DLOBNodeType::Trigger, SortDirection::Desc),
        });

        let market_node_lists = vec![resting_limit, floating_limit, taking_limit, market, trigger];

        if let Some(market_node_lists_map) = self.order_lists.get_mut(&market_type) {
            for market_node_list in market_node_lists {
                market_node_lists_map.insert(market_index, market_node_list);
            }
        } else {
            let mut new_market_node_lists_map = HashMap::new();
            for market_node_list in market_node_lists {
                new_market_node_lists_map.insert(market_index, market_node_list);
            }
            self.order_lists
                .insert(market_type, new_market_node_lists_map);
        }
    }

    fn get_list_for_order(&self, order: Order, slot: u64) -> Option<NodeList> {
        let node_type = determine_node_type(&order, slot);
        let is_inactive_trigger_order = node_type == DLOBNodeType::Trigger;
        let order_sub_type = determine_sub_type(&order, is_inactive_trigger_order);

        self.order_lists
            .get(&order.market_type.into())
            .and_then(|d| d.get(&order.market_index))
            .and_then(|market_node_lists| match market_node_lists {
                MarketNodeLists::RestingLimit(list)
                | MarketNodeLists::FloatingLimit(list)
                | MarketNodeLists::TakingLimit(list)
                | MarketNodeLists::Market(list) => {
                    if let OrderSubType::Side(side) = order_sub_type {
                        match side {
                            Side::Ask => Some(&list.ask),
                            Side::Bid => Some(&list.bid),
                        }
                    } else {
                        None
                    }
                }
                MarketNodeLists::Trigger(list) => {
                    if let OrderSubType::Trigger(trigger) = order_sub_type {
                        match trigger {
                            OrderTriggerCondition::Above => Some(&list.above),
                            OrderTriggerCondition::Below => Some(&list.below),
                            _ => None,
                        }
                    } else {
                        None
                    }
                }
            })
            .cloned()
    }

    fn delete(&mut self, order: Order, user_account: Pubkey, slot: u64) -> DriftResult<()> {
        if order.status == OrderStatus::Init {
            return Ok(());
        }

        self.update_resting_limit_orders(slot)?;

        if let Some(mut list) = self.get_list_for_order(order, slot) {
            list.remove(order, user_account)?
        }

        Ok(())
    }

    fn trigger(&mut self, order: Order, user_account: Pubkey, slot: u64) -> DriftResult<()> {
        if order.status == OrderStatus::Init {
            return Ok(());
        }

        self.update_resting_limit_orders(slot)?;

        if order.trigger_condition == OrderTriggerCondition::Above
            || order.trigger_condition == OrderTriggerCondition::Below
        {
            return Ok(());
        }

        if let Some(market_node_lists) = self.order_lists.get_mut(&order.market_type.into()) {
            if let Some(node_list) = market_node_lists.get_mut(&order.market_index) {
                let trigger_list = match node_list {
                    MarketNodeLists::Trigger(trigger_node_list) => {
                        Some(if order.trigger_condition == OrderTriggerCondition::Above {
                            &mut trigger_node_list.above
                        } else {
                            &mut trigger_node_list.below
                        })
                    }
                    _ => None,
                };

                if let Some(trigger_list) = trigger_list {
                    trigger_list.remove(order, user_account)?;
                }

                if let Some(mut node_list) = self.get_list_for_order(order, slot) {
                    node_list.insert(order, user_account)?;
                }
            }
        }

        Ok(())
    }

    fn update_order(
        &mut self,
        order: Order,
        user_account: Pubkey,
        slot: u64,
        cumulative_base_asset_amount_filled: u64,
    ) -> DriftResult<()> {
        self.update_resting_limit_orders(slot)?;

        if order
            .base_asset_amount
            .eq(&cumulative_base_asset_amount_filled)
        {
            self.delete(order, user_account, slot)?;
            return Ok(());
        }

        if order
            .base_asset_amount_filled
            .eq(&cumulative_base_asset_amount_filled)
        {
            return Ok(());
        }

        let mut new_order = order;

        new_order.base_asset_amount = cumulative_base_asset_amount_filled;

        if let Some(mut node_list) = self.get_list_for_order(order, slot) {
            node_list.update(new_order, user_account)?;
        }

        Ok(())
    }

    fn update_resting_limit_orders(&mut self, slot: u64) -> DriftResult<()> {
        if slot <= self.max_slot_for_resting_limit_orders as u64 {
            return Ok(());
        }

        self.max_slot_for_resting_limit_orders = 0;

        self.update_resting_limit_orders_for_market_type(slot, MarketType::Perp)?;
        self.update_resting_limit_orders_for_market_type(slot, MarketType::Spot)?;

        Ok(())
    }

    fn update_resting_limit_orders_for_market_type(
        &mut self,
        slot: u64,
        market_type: MarketType,
    ) -> DriftResult<()> {
        if let Some(map) = self.order_lists.get_mut(&market_type) {
            for market_node_lists in map.values_mut() {
                let mut nodes_to_update = Vec::new();

                if let MarketNodeLists::TakingLimit(taking_limit) = market_node_lists {
                    for node in taking_limit.ask.iter() {
                        if let Some(order) = node.order() {
                            if !order.is_resting_limit_order(slot).unwrap() {
                                continue;
                            }
                        }
                        nodes_to_update.push((Side::Ask, node));
                    }

                    for node in taking_limit.bid.iter() {
                        if let Some(order) = node.order() {
                            if !order.is_resting_limit_order(slot).unwrap() {
                                continue;
                            }
                        }
                        nodes_to_update.push((Side::Bid, node));
                    }
                }

                for (side, node) in nodes_to_update {
                    if let MarketNodeLists::RestingLimit(resting_limit) = market_node_lists {
                        match side {
                            Side::Ask => {
                                if let Some(order) = node.order() {
                                    if let Some(user_account) = node.user_account() {
                                        resting_limit.ask.remove(*order, *user_account)?;
                                        resting_limit.ask.insert(*order, *user_account)?;
                                    }
                                }
                            }
                            Side::Bid => {
                                if let Some(order) = node.order() {
                                    if let Some(user_account) = node.user_account() {
                                        resting_limit.bid.remove(*order, *user_account)?;
                                        resting_limit.bid.insert(*order, *user_account)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn get_order(&self, order_id: u32, user_account: Pubkey) -> DriftResult<Option<Order>> {
        let order_sig = get_order_signature(order_id, &user_account);
        for node_list in self.get_node_lists() {
            if let Some(node) = node_list.get(&order_sig) {
                if let Some(order) = node.order() {
                    return Ok(Some(*order));
                }
            }
        }
        Ok(None)
    }

    pub fn get_node_lists(&self) -> Vec<NodeList> {
        let perp_node_lists: Vec<_> = self
            .order_lists
            .get(&MarketType::Perp)
            .unwrap_or(&HashMap::new())
            .values()
            .flat_map(|market_node_list| match market_node_list {
                MarketNodeLists::RestingLimit(list) => vec![list.ask.clone(), list.bid.clone()],
                MarketNodeLists::FloatingLimit(list) => vec![list.ask.clone(), list.bid.clone()],
                MarketNodeLists::TakingLimit(list) => vec![list.ask.clone(), list.bid.clone()],
                MarketNodeLists::Market(list) => vec![list.ask.clone(), list.bid.clone()],
                MarketNodeLists::Trigger(list) => vec![list.above.clone(), list.below.clone()],
            })
            .collect();

        let spot_node_lists: Vec<_> = self
            .order_lists
            .get(&MarketType::Spot)
            .unwrap_or(&HashMap::new())
            .values()
            .flat_map(|market_node_list| match market_node_list {
                MarketNodeLists::RestingLimit(list) => vec![list.ask.clone(), list.bid.clone()],
                MarketNodeLists::FloatingLimit(list) => vec![list.ask.clone(), list.bid.clone()],
                MarketNodeLists::TakingLimit(list) => vec![list.ask.clone(), list.bid.clone()],
                MarketNodeLists::Market(list) => vec![list.ask.clone(), list.bid.clone()],
                MarketNodeLists::Trigger(list) => vec![list.above.clone(), list.below.clone()],
            })
            .collect();

        let mut all_node_lists = perp_node_lists;
        all_node_lists.extend(spot_node_lists);

        all_node_lists
    }
}

pub enum OrderSubType {
    Trigger(OrderTriggerCondition),
    Side(Side),
}

fn determine_sub_type(order: &Order, is_inactive_trigger_order: bool) -> OrderSubType {
    if is_inactive_trigger_order {
        OrderSubType::Trigger(match order.trigger_condition {
            OrderTriggerCondition::Above => OrderTriggerCondition::Above,
            _ => OrderTriggerCondition::Below,
        })
    } else {
        OrderSubType::Side(match order.direction {
            PositionDirection::Long => Side::Bid,
            _ => Side::Ask,
        })
    }
}

fn determine_node_type(order: &Order, slot: u64) -> DLOBNodeType {
    if matches!(
        order.trigger_condition,
        OrderTriggerCondition::TriggeredAbove | OrderTriggerCondition::TriggeredBelow
    ) && order.must_be_triggered()
    {
        DLOBNodeType::Trigger
    } else if matches!(
        order.order_type,
        OrderType::Market | OrderType::TriggerMarket | OrderType::Oracle
    ) {
        DLOBNodeType::Market
    } else if order.oracle_price_offset != 0 {
        DLOBNodeType::FloatingLimit
    } else if order.is_resting_limit_order(slot).unwrap() {
        DLOBNodeType::RestingLimit
    } else {
        DLOBNodeType::TakingLimit
    }
}
