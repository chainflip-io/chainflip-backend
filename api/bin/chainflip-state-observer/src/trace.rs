
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum Trace<K,V> {
    Composite(V, BTreeMap<K, Trace<K,V>>),
    Single(V)
}

pub struct TraceFn<K,V,W> {
    create: Box<dyn Fn(K, Option<W>) -> W>,
    update: Box<dyn Fn(K,V) -> W>,
    destroy: Box<dyn Fn(K, V)>,
}
