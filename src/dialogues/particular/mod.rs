pub mod order_specify_price;
pub mod purchase;
pub mod redeem;
pub mod replenish;
pub mod sell;
pub mod writeoff;

use crate::prelude::*;
use teloxide::prelude::*;

pub fn handler() -> HandlerResult {
    dptree::entry()
        .branch(sell::handler())
        .branch(replenish::handler())
        .branch(writeoff::handler())
        .branch(purchase::handler())
        .branch(redeem::handler())
        .branch(order_specify_price::handler())
}

pub fn write_deps(deps: &mut DependencyMap) {
    sell::write_deps(deps);
    replenish::write_deps(deps);
    writeoff::write_deps(deps);
    purchase::write_deps(deps);
    redeem::write_deps(deps);
    order_specify_price::write_deps(deps);
}
