use crate::node::SingletonNode;
use crossbeam::epoch::{Atomic, Guard};
use std::{fmt::Debug, sync::atomic::Ordering};

#[derive(Clone)]
pub struct ListNode<K, V> {
    singleton: SingletonNode<K, V>,
    next: Option<Atomic<ListNode<K, V>>>,
}
