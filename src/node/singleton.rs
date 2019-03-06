#[derive(Clone)]
pub struct SingletonNode<K, V> {
    pub key: K,
    pub value: V,
}

impl<K, V> SingletonNode<K, V> {
    pub fn new(key: K, value: V) -> Self {
        Self { key, value }
    }

    pub fn print(&self, indent: usize)
    where
        K: std::fmt::Debug,
        V: std::fmt::Debug,
    {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        print!("{}singleton: ", tab);
        println!("({:?}, {:?})", self.key, self.value);
    }
}
