use crate::{
    node::{Branch, CtrieNode, IndirectionNode, ListNode, SingletonNode, TombNode},
    Generation, Key, Value, LAST_W_BITS, W,
};
use crossbeam::epoch::{Atomic, Guard};
use std::cmp;
use std::fmt::Debug;

#[derive(Clone)]
pub enum MainNodeKind<K, V> {
    Ctrie(CtrieNode<K, V>),
    List(ListNode<K, V>),
    Tomb(TombNode<K, V>),
    Failed,
}

#[derive(Clone)]
pub struct MainNode<K, V> {
    kind: MainNodeKind<K, V>,
    prev: Atomic<MainNode<K, V>>,
}

impl<K, V> MainNode<K, V>
where
    K: Key,
    V: Value,
{
    pub fn new(
        x: SingletonNode<K, V>,
        x_hash: u64,
        y: SingletonNode<K, V>,
        y_hash: u64,
        level: usize,
        generation: Generation,
    ) -> Self {
        if level < 64 {
            let x_index = (x_hash >> level) & LAST_W_BITS;
            let y_index = (y_hash >> level) & LAST_W_BITS;
            let x_flag = 1 << x_index;
            let y_flag = 1 << y_index;
            let bitmap = x_flag | y_flag;

            match x_index.cmp(&y_index) {
                cmp::Ordering::Less => Self {
                    kind: MainNodeKind::Ctrie(CtrieNode::new(
                        bitmap,
                        vec![Branch::Singleton(x), Branch::Singleton(y)],
                        generation,
                    )),
                    prev: Atomic::null(),
                },
                cmp::Ordering::Greater => Self {
                    kind: MainNodeKind::Ctrie(CtrieNode::new(
                        bitmap,
                        vec![Branch::Singleton(y), Branch::Singleton(x)],
                        generation,
                    )),
                    prev: Atomic::null(),
                },
                cmp::Ordering::Equal => {
                    let main = Self::new(x, x_hash, y, y_hash, level + W, generation.clone());
                    let inode = IndirectionNode::new(Atomic::new(main), generation.clone());
                    Self {
                        kind: MainNodeKind::Ctrie(CtrieNode::new(
                            bitmap,
                            vec![Branch::Indirection(inode)],
                            generation,
                        )),
                        prev: Atomic::null(),
                    }
                }
            }
        } else {
            unimplemented!()
        }
    }

    pub fn failed(prev: Atomic<MainNode<K, V>>) -> Self {
        Self {
            kind: MainNodeKind::Failed,
            prev,
        }
    }

    pub fn from_ctrie_node(cnode: CtrieNode<K, V>) -> Self {
        Self {
            kind: MainNodeKind::Ctrie(cnode),
            prev: Atomic::null(),
        }
    }

    pub fn from_list_node(lnode: ListNode<K, V>) -> Self {
        Self {
            kind: MainNodeKind::List(lnode),
            prev: Atomic::null(),
        }
    }

    pub fn from_tomb_node(tnode: TombNode<K, V>) -> Self {
        Self {
            kind: MainNodeKind::Tomb(tnode),
            prev: Atomic::null(),
        }
    }

    pub fn kind(&self) -> &MainNodeKind<K, V> {
        &self.kind
    }

    pub fn prev(&self) -> &Atomic<MainNode<K, V>> {
        &self.prev
    }

    pub fn print<'g>(&self, indent: usize, guard: &'g Guard) where K: Debug, V: Debug {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        println!("{}main:", tab);
        match &self.kind {
            MainNodeKind::Ctrie(cnode) => cnode.print(indent, guard),
            _ => unimplemented!(),
        }
    }
}
