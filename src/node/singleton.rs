use std::fmt::Debug;

/// A node that represents a single entry in a ctrie.
///
/// Contains a key and a corresponding value.
#[derive(Clone)]
pub struct SingletonNode<K, V> {
    key: K,
    value: V,
}

impl<K, V> SingletonNode<K, V> {
    /// Creates a new singleton node with the given key and value.
    pub fn new(key: K, value: V) -> Self {
        Self { key, value }
    }

    /// Returns the key of the singleton node.
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Returns the value of the singleton node.
    pub fn value(&self) -> &V {
        &self.value
    }

    pub fn print(&self, indent: usize)
    where
        K: Debug,
        V: Debug,
    {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        println!("{}snode: ({:?}, {:?})", tab, self.key, self.value);
    }
}
