use crate::node::SingletonNode;
use crossbeam::epoch::{Atomic, Guard};
use std::sync::atomic::Ordering;

#[derive(Clone)]
pub struct ListNode<K, V> {
    singleton: SingletonNode<K, V>,
    next: Option<Atomic<ListNode<K, V>>>,
}

impl<K, V> ListNode<K, V>
where
    K: Eq,
{
    pub fn new(singleton: SingletonNode<K, V>) -> Self {
        Self {
            singleton,
            next: None,
        }
    }

    pub fn lookup<'g>(&'g self, guard: &'g Guard, key: &K) -> Option<&'g V> {
        if self.singleton.key == *key {
            Some(&self.singleton.value)
        } else if let Some(next) = &self.next {
            let next_pointer = next.load(Ordering::SeqCst, guard);
            let next = unsafe { next_pointer.deref() };
            next.lookup(guard, key)
        } else {
            None
        }
    }
}
