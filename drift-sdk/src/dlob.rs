use std::collections::{HashMap, HashSet};

use anchor_lang::prelude::Pubkey;
use drift::state::{
    events::OrderRecord,
    user::{Order, OrderStatus, OrderType, User},
    user_map::UserMap,
};

use crate::{dlob_orders::DLOBOrders, node_list::NodeList};

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

pub struct MarketNodeLists {
    pub resting_limit: SideNodeList,
    pub floating_limit: SideNodeList,
    pub taking_limit: SideNodeList,
    pub market: SideNodeList,
    pub trigger: TriggerNodeList,
}

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
    order_list: HashMap<MarketType, HashMap<i32, MarketNodeLists>>,
    max_slot_for_resting_limit_orders: u32,
    initialized: bool,
}

impl Default for DLOB {
    fn default() -> Self {
        let mut open_orders = HashMap::new();
        open_orders.insert(MarketType::Perp, HashSet::new());
        open_orders.insert(MarketType::Spot, HashSet::new());

        let mut order_list = HashMap::new();
        order_list.insert(MarketType::Perp, HashMap::new());
        order_list.insert(MarketType::Spot, HashMap::new());

        Self {
            open_orders,
            order_list,
            max_slot_for_resting_limit_orders: 0,
            initialized: false,
        }
    }
}

impl DLOB {
    pub fn new() -> Self {
        DLOB::default()
    }

    pub fn initialize(&mut self) {
        self.initialized = true;
    }

    pub fn clear(&mut self) {
        for market_type in self.open_orders.keys().cloned().collect::<Vec<_>>() {
            self.open_orders.get_mut(&market_type).unwrap().clear();
        }
        self.open_orders.clear();

        for market_type in self.order_list.keys().cloned().collect::<Vec<_>>() {
            if let Some(market_node_lists_map) = self.order_list.get_mut(&market_type) {
                for market_node_lists in market_node_lists_map.values_mut() {
                    market_node_lists.resting_limit.ask.clear();
                    market_node_lists.resting_limit.bid.clear();
                    market_node_lists.floating_limit.ask.clear();
                    market_node_lists.floating_limit.bid.clear();
                    market_node_lists.taking_limit.ask.clear();
                    market_node_lists.taking_limit.bid.clear();
                    market_node_lists.market.ask.clear();
                    market_node_lists.market.bid.clear();
                    market_node_lists.trigger.above.clear();
                    market_node_lists.trigger.below.clear();
                }
            }
        }
        self.order_list.clear();

        self.max_slot_for_resting_limit_orders = 0;

        self.initialize();
    }

    //TODO
    fn init_from_user_map(&mut self, user_map: UserMap, slot: u64) -> bool {
        if self.initialized {
            return false;
        }

        true
    }

    pub fn init_from_orders(&mut self, dlob_orders: DLOBOrders, slot: u64) -> bool {
        if self.initialized {
            return false;
        }

        for dlob_order in dlob_orders {
            self.insert_order(dlob_order.order.clone(), dlob_order.user.clone(), slot);
        }

        self.initialized = true;
        true
    }

    pub fn handle_order_record(&mut self, record: OrderRecord, slot: u64) {
        self.insert_order(record.order, record.user, slot);
    }

    pub fn insert_order(&mut self, order: Order, user_account: Pubkey, slot: u64) {
        if matches!(order.status, OrderStatus::Init) {
            return;
        }

        match order.order_type {
            OrderType::Market => {}
            OrderType::Limit => {}
            OrderType::TriggerMarket => {}
            OrderType::TriggerLimit => {}
            OrderType::Oracle => {}
            _ => return,
        }

        let market_type = order.market_type;

        if !self.order_list.contains_key(&market_type.into()) {
            self.add_order_list(market_type.into(), order.market_index);
        }

        if matches!(order.status, OrderStatus::Open) {
            let order_signature = format!("{}-{}", user_account, order.order_id);
            self.open_orders
                .entry(market_type.into())
                .or_insert_with(HashSet::new)
                .insert(order_signature);
        }

        if let Some(mut list) = self.get_list_for_order(&order, slot) {
            list.insert(order, market_type, user_account);
        }
    }

    //TODO
    fn add_order_list(&mut self, market_type: MarketType, market_index: u16) {}

    //TODO
    fn get_list_for_order(&self, order: &Order, slot: u64) -> Option<NodeList> {
        None
    }
}
