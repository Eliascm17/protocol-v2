use anchor_lang::prelude::Pubkey;

use drift::state::user::Order;

pub type DLOBOrders = Vec<DLOBOrder>;

#[derive(Debug, Copy, Clone)]
pub struct DLOBOrder {
    pub user: Pubkey,
    pub order: Order,
}
