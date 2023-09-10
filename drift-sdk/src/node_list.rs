use anchor_lang::prelude::Pubkey;
use drift::state::user::{MarketType, Order, OrderStatus};
use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use crate::dlob_node::{create_node, DLOBNode, DLOBNodeType};

pub fn get_order_signature(order_id: u32, user_account: &Pubkey) -> String {
    format!("{}-{}", user_account, order_id)
}

#[derive(Debug, Clone)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug)]
pub struct NodeWrapper {
    node: Arc<dyn DLOBNode>,
    next: Mutex<Option<Arc<NodeWrapper>>>,
    previous: Mutex<Option<Arc<NodeWrapper>>>,
}

pub struct NodeList {
    head: Option<Arc<NodeWrapper>>,
    node_type: DLOBNodeType,
    length: usize,
    node_map: HashMap<String, Arc<dyn DLOBNode>>,
    sort_direction: SortDirection,
}

impl NodeList {
    pub fn new(node_type: DLOBNodeType, sort_direction: SortDirection) -> Self {
        Self {
            head: None,
            node_type,
            length: 0,
            node_map: HashMap::new(),
            sort_direction,
        }
    }

    pub fn clear(&mut self) {
        self.head = None;
        self.length = 0;
        self.node_map.clear();
    }

    pub fn insert(&mut self, order: Order, _market_type: MarketType, user_account: Pubkey) {
        if matches!(order.status, OrderStatus::Init) {
            return;
        }

        let new_node = Arc::new(NodeWrapper {
            node: create_node(self.node_type.clone(), order, user_account),
            next: Mutex::new(None),
            previous: Mutex::new(None),
        });

        let order_signature = get_order_signature(order.order_id, &user_account);

        if self.node_map.contains_key(&order_signature) {
            return;
        }

        self.node_map
            .insert(order_signature.clone(), new_node.node.clone());
        self.length += 1;

        if self.head.is_none() {
            self.head = Some(new_node.clone());
            return;
        }

        let mut current_node = self.head.clone();

        while let Some(current) = &current_node.clone() {
            let should_prepend = current
                .next
                .lock()
                .unwrap()
                .as_ref()
                .map_or(false, |next| self.prepend_node(&next.node, &new_node.node));

            if should_prepend {
                let next = current.next.lock().unwrap().clone().unwrap();
                *new_node.next.lock().unwrap() = Some(next.clone());
                *next.previous.lock().unwrap() = Some(new_node.clone());
                *current.next.lock().unwrap() = Some(new_node.clone());
                *new_node.previous.lock().unwrap() = Some(current.clone());
                return;
            }

            current_node = current.next.lock().unwrap().clone();
        }

        if let Some(last_node) = &current_node {
            *new_node.previous.lock().unwrap() = Some(last_node.clone());
            *last_node.next.lock().unwrap() = Some(new_node.clone());
        }
    }
    fn prepend_node(&self, current_node: &Arc<dyn DLOBNode>, new_node: &Arc<dyn DLOBNode>) -> bool {
        let current_order_sort_price = current_node.sort_value();
        let new_order_sort_price = new_node.sort_value();

        match self.sort_direction {
            SortDirection::Asc => new_order_sort_price < current_order_sort_price,
            SortDirection::Desc => new_order_sort_price > current_order_sort_price,
        }
    }

    pub fn update(&mut self, order: Order, user_account: Pubkey) {
        let order_signature = get_order_signature(order.order_id, &user_account);
        if self.node_map.contains_key(&order_signature) {
            let new_node = create_node(self.node_type.clone(), order, user_account);
            self.node_map.insert(order_signature, new_node);
        }
    }

    pub fn remove(&mut self, order: Order, user_account: Pubkey) {
        let order_signature = get_order_signature(order.order_id, &user_account);
        self.node_map.remove(&order_signature);
        self.length -= 1;
    }

    pub fn has(&self, order: Order, user_account: Pubkey) -> bool {
        let order_signature = get_order_signature(order.order_id, &user_account);
        self.node_map.contains_key(&order_signature)
    }

    pub fn get(&self, order_signature: &str) -> Option<&Arc<dyn DLOBNode>> {
        self.node_map.get(order_signature)
    }

    pub fn print(&self) {
        // TODO
    }

    pub fn print_top(&self) {
        // TODO
    }
}
