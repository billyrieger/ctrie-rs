use crate::{node::SingletonNode, Key, Value, LOAD_ORD};
use crossbeam::epoch::{Atomic, Guard};

/// A node that represents an immutable linked list of singleton nodes.
///
/// Used in the case of hash collisions.
#[derive(Clone)]
pub struct ListNode<K, V> {
    head: SingletonNode<K, V>,
    tail: Atomic<ListNode<K, V>>,
}

impl<K, V> ListNode<K, V>
where
    K: Key,
    V: Value,
{
    /// Creates a new list node with one singleton node with the given key and value.
    pub fn new(key: K, value: V) -> Self {
        Self {
            head: SingletonNode::new(key, value),
            tail: Atomic::null(),
        }
    }

    /// Returns the number of nodes in the list.
    ///
    /// Guaranteed to be at least one.
    pub fn length<'g>(&self, guard: &'g Guard) -> usize {
        // list node always contains at one element: self.head
        let mut length = 1;
        let mut tail_ptr = self.tail.load(LOAD_ORD, guard);
        // traverse the rest of the list, incrementing length each time
        loop {
            if tail_ptr.is_null() {
                // end of the list
                return length;
            } else {
                // at this point tail_ptr is not null
                let tail = unsafe { tail_ptr.deref() };
                tail_ptr = tail.tail.load(LOAD_ORD, guard);
                length += 1;
            }
        }
    }

    /// Adds a new singleton node with the given key and value to the beginning of the list.
    ///
    /// Returns the new list.
    pub fn add(&self, key: K, value: V) -> Self {
        Self {
            head: SingletonNode::new(key, value),
            tail: Atomic::new(self.clone()),
        }
    }

    /// Removes the element corresponding to the given key from the list.
    ///
    /// Returns the new list or `None` if the new list is empty. Also returns a boolean
    /// representing if anything was removed.
    pub fn remove<'g>(&'g self, key: &K, guard: &'g Guard) -> (Option<Self>, bool) {
        let tail_ptr = self.tail.load(LOAD_ORD, guard);
        if key == self.head.key() {
            // key found
            if tail_ptr.is_null() {
                // no tail, so there's no remaining list to return
                (None, true)
            } else {
                // at this point tail_ptr is not null
                let tail = unsafe { tail_ptr.deref() };
                // return the remainder of the list
                (Some(tail.clone()), true)
            }
        } else {
            // keep searching
            if tail_ptr.is_null() {
                // we're done searching and didn't find the key
                (Some(self.clone()), false)
            } else {
                // at this point tail_ptr is not null
                let tail = unsafe { tail_ptr.deref() };
                // recursively remove the element from the tail
                let (maybe_new_tail, did_remove) = tail.remove(key, guard);
                let new_list = if let Some(new_tail) = maybe_new_tail {
                    Self { 
                        head: self.head.clone(),
                        tail: Atomic::new(new_tail),
                    }
                } else {
                    Self { 
                        head: self.head.clone(),
                        tail: Atomic::null(),
                    }
                };
                (Some(new_list), did_remove)
            }
        }
    }

    /// Attempts to locate the singleton node with the given key in the list, returning its
    /// corresponding value if found.
    pub fn lookup<'g>(&'g self, key: &K, guard: &'g Guard) -> Option<&V> {
        if key == self.head.key() {
            // key found
            Some(self.head.value())
        } else {
            // traverse the rest of the list searching for the key
            let mut tail_ptr = self.tail.load(LOAD_ORD, guard);
            loop {
                if tail_ptr.is_null() {
                    // end of the list
                    break None;
                } else {
                    // at this point tail_ptr is not null
                    let tail = unsafe { tail_ptr.deref() };
                    if key == tail.head.key() {
                        // key found
                        break Some(tail.head.value());
                    } else {
                        // continue searching
                        tail_ptr = tail.tail.load(LOAD_ORD, guard);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crossbeam::epoch;

    #[test]
    fn add_lookup_remove() {
        let list = ListNode::new('c', 3).add('b', 2).add('a', 1);

        let guard = &epoch::pin();

        assert_eq!(list.length(guard), 3);
        assert_eq!(list.lookup(&'a', guard), Some(&1));
        assert_eq!(list.lookup(&'b', guard), Some(&2));
        assert_eq!(list.lookup(&'c', guard), Some(&3));
        assert_eq!(list.lookup(&'d', guard), None);

        let (list, did_remove) = list.remove(&'d', guard);
        assert!(!did_remove);
        let list = list.unwrap();
        assert_eq!(list.length(guard), 3);

        let (list, did_remove) = list.remove(&'b', guard);
        assert!(did_remove);
        let list = list.unwrap();
        assert_eq!(list.length(guard), 2);

        let (list, did_remove) = list.remove(&'c', guard);
        assert!(did_remove);
        let list = list.unwrap();
        assert_eq!(list.length(guard), 1);

        let (list, did_remove) = list.remove(&'a', guard);
        assert!(did_remove);
        assert!(list.is_none());
    }
}
