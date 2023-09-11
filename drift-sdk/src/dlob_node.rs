use std::{fmt::Debug, sync::Arc};

use anchor_lang::prelude::Pubkey;
use drift::{
    controller::position::PositionDirection,
    math::constants::{AMM_RESERVE_PRECISION, PRICE_PRECISION},
    state::{
        oracle::OraclePriceData,
        user::{Order, OrderTriggerCondition},
    },
};

use crate::{conversion::convert_to_number, node_list::get_order_signature};

pub trait DLOBNode: Debug {
    fn get_price(&self, oracle_price_data: &OraclePriceData, slot: u64) -> i128;
    fn is_vamm_node(&self) -> bool;
    fn is_base_filled(&self) -> bool;
    fn have_filled(&self) -> bool;
    fn order(&self) -> Option<&Order>;
    fn user_account(&self) -> Option<&Pubkey>;
    fn sort_value(&self) -> i128;
}

#[derive(Debug, Clone)]
pub struct OrderNode {
    pub order: Order,
    pub user_account: Pubkey,
    pub sort_value: i128,
    pub have_filled: bool,
    pub have_trigger: bool,
}

impl OrderNode {
    pub fn new(order: Order, user_account: Pubkey) -> Self {
        let sort_value = Self::get_sort_value(&order);
        Self {
            order,
            user_account,
            sort_value,
            have_filled: false,
            have_trigger: false,
        }
    }

    pub fn get_sort_value(order: &Order) -> i128 {
        order.price as i128
    }

    pub fn get_label(&self) -> String {
        let mut msg = format!(
            "Order {}",
            get_order_signature(self.order.order_id, &self.user_account)
        );
        msg += if let PositionDirection::Long = self.order.direction {
            " LONG "
        } else {
            " SHORT "
        };
        msg += &format!(
            "{:.3}",
            convert_to_number(self.order.base_asset_amount, AMM_RESERVE_PRECISION)
        );
        if self.order.price > 0 {
            msg += &format!(
                " @ {:.3}",
                convert_to_number(self.order.price, PRICE_PRECISION)
            );
        }
        if self.order.trigger_price > 0 {
            msg += match self.order.trigger_condition {
                OrderTriggerCondition::Below => " BELOW ",
                _ => " ABOVE ",
            };
            msg += &format!(
                "{:.3}",
                convert_to_number(self.order.trigger_price, PRICE_PRECISION)
            );
        }
        msg
    }
}

#[derive(Debug, Clone)]
pub enum DLOBNodeOrders {
    RestingLimit(OrderNode),
    TakingLimit(OrderNode),
    FloatingLimit(OrderNode),
    Market(OrderNode),
    Trigger(OrderNode),
}

impl DLOBNode for DLOBNodeOrders {
    fn get_price(&self, oracle_price_data: &OraclePriceData, slot: u64) -> i128 {
        oracle_price_data.price as i128
    }

    fn is_vamm_node(&self) -> bool {
        false
    }

    fn is_base_filled(&self) -> bool {
        match self {
            DLOBNodeOrders::RestingLimit(order_node)
            | DLOBNodeOrders::TakingLimit(order_node)
            | DLOBNodeOrders::FloatingLimit(order_node)
            | DLOBNodeOrders::Market(order_node)
            | DLOBNodeOrders::Trigger(order_node) => order_node
                .order
                .base_asset_amount_filled
                .eq(&order_node.order.base_asset_amount),
        }
    }

    fn have_filled(&self) -> bool {
        match self {
            DLOBNodeOrders::RestingLimit(order_node)
            | DLOBNodeOrders::TakingLimit(order_node)
            | DLOBNodeOrders::FloatingLimit(order_node)
            | DLOBNodeOrders::Market(order_node)
            | DLOBNodeOrders::Trigger(order_node) => order_node.have_filled,
        }
    }

    fn order(&self) -> Option<&Order> {
        match self {
            DLOBNodeOrders::RestingLimit(order_node)
            | DLOBNodeOrders::TakingLimit(order_node)
            | DLOBNodeOrders::FloatingLimit(order_node)
            | DLOBNodeOrders::Market(order_node)
            | DLOBNodeOrders::Trigger(order_node) => Some(&order_node.order),
        }
    }

    fn user_account(&self) -> Option<&Pubkey> {
        match self {
            DLOBNodeOrders::RestingLimit(order_node)
            | DLOBNodeOrders::TakingLimit(order_node)
            | DLOBNodeOrders::FloatingLimit(order_node)
            | DLOBNodeOrders::Market(order_node)
            | DLOBNodeOrders::Trigger(order_node) => Some(&order_node.user_account),
        }
    }

    fn sort_value(&self) -> i128 {
        match self {
            DLOBNodeOrders::RestingLimit(order_node)
            | DLOBNodeOrders::TakingLimit(order_node)
            | DLOBNodeOrders::FloatingLimit(order_node)
            | DLOBNodeOrders::Market(order_node)
            | DLOBNodeOrders::Trigger(order_node) => order_node.sort_value,
        }
    }
}

pub fn create_node(
    node_type: DLOBNodeType,
    order: Order,
    user_account: Pubkey,
) -> Arc<dyn DLOBNode> {
    let order_node = OrderNode::new(order, user_account);
    let node = match node_type {
        DLOBNodeType::RestingLimit => DLOBNodeOrders::RestingLimit(order_node),
        DLOBNodeType::TakingLimit => DLOBNodeOrders::TakingLimit(order_node),
        DLOBNodeType::FloatingLimit => DLOBNodeOrders::FloatingLimit(order_node),
        DLOBNodeType::Market => DLOBNodeOrders::Market(order_node),
        DLOBNodeType::Trigger => DLOBNodeOrders::Trigger(order_node),
    };
    Arc::new(node)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DLOBNodeType {
    RestingLimit,
    TakingLimit,
    FloatingLimit,
    Market,
    Trigger,
}

impl From<DLOBNodeOrders> for DLOBNodeType {
    fn from(node_orders: DLOBNodeOrders) -> Self {
        match node_orders {
            DLOBNodeOrders::RestingLimit(_) => DLOBNodeType::RestingLimit,
            DLOBNodeOrders::TakingLimit(_) => DLOBNodeType::TakingLimit,
            DLOBNodeOrders::FloatingLimit(_) => DLOBNodeType::FloatingLimit,
            DLOBNodeOrders::Market(_) => DLOBNodeType::Market,
            DLOBNodeOrders::Trigger(_) => DLOBNodeType::Trigger,
        }
    }
}
