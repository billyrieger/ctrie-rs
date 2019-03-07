use crate::{
    node::MainNode,
    Generation,
};
use crossbeam::epoch::{Atomic};

#[derive(Clone)]
pub struct IndirectionNode<K, V> {
    main: Atomic<MainNode<K, V>>,
    generation: Generation,
}
