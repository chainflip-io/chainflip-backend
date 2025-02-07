
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum Trace<K,V> {
    Composite(V, BTreeMap<K, Trace<K,V>>),
    // Single(V)
}
use Trace::*;

pub enum NodeDiff<V> {
    Left(V),
    Right(V),
    Both(V,V)
}

use NodeDiff::*;

pub fn diff<K,V: PartialEq + Clone>(a: Option<Trace<K,V>>, b: Option<Trace<K,V>>) -> Option<Trace<K,NodeDiff<V>>> {
    match (a, b) {
        (None, None) => None,
        (None, Some(Composite(y, ys))) => todo!(), // Composite(Right(y), ),
        (Some(_), None) => todo!(),
        (Some(_), Some(_)) => todo!(),

        // (Trace::Composite(x, xs), Trace::Composite(y, ys)) if x == y => {
        //     Composite(x.clone(), NodeDiff::Modified())
        // },
    }
}

pub struct TraceFn<K,V,W> {
    create: Box<dyn Fn(K, Option<W>) -> W>,
    update: Box<dyn Fn(K,V) -> W>,
    destroy: Box<dyn Fn(K, V)>,
}
