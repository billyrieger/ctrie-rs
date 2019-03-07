use crate::node::SingletonNode;
use crossbeam::epoch::{Atomic, Guard};
use std::{fmt::Debug, sync::atomic::Ordering};

#[derive(Clone)]
pub struct ListNode<K, V> {
    singleton: SingletonNode<K, V>,
    next: Option<Atomic<ListNode<K, V>>>,
}

impl<K, V> ListNode<K, V>
where
    K: Clone + Eq,
    V: Clone,
{
    pub fn new(singleton: SingletonNode<K, V>) -> Self {
        Self {
            singleton,
            next: None,
        }
    }

    fn with_next(snode: SingletonNode<K, V>, next: Atomic<ListNode<K, V>>) -> Self {
        Self {
            singleton: snode,
            next: Some(next),
        }
    }

    pub fn inserted(&self, key: K, value: V) -> Self {
        let snode = SingletonNode::new(key, value);
        Self::with_next(snode, Atomic::new(self.clone()))
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

    fn print<'g>(&self, indent: usize, ordering: Ordering, guard: &'g Guard)
    where
        K: Debug,
        V: Debug,
    {
        println!(
            "{}lnode:",
            std::iter::repeat(' ').take(indent).collect::<String>()
        );
        self.singleton.print(indent);
        if let Some(lnode) = &self.next {
            let lnode = unsafe { lnode.load(ordering, guard).deref() };
            lnode.print(indent + 2, ordering, guard);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let guard = &crossbeam::epoch::pin();
        let snode = SingletonNode::new('a', 1);
        let lnode = ListNode::new(snode);
        let lnode = lnode.inserted('b', 2);
        let lnode = lnode.inserted('c', 3);
        lnode.print(0, Ordering::SeqCst, guard);
    }
}
