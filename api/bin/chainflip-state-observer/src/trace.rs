
use std::{collections::BTreeMap, os::unix::process};

// #[derive(Debug)]
// pub enum Trace<K,V> {
//     Composite(V, BTreeMap<K, Trace<K,V>>),
//     // Single(V)
// }
// use Trace::*;

pub type Trace<K,V> = BTreeMap<Vec<K>,V>;

pub enum NodeDiff<V, W> {
    Left(V),
    Right(W),
    Both(V,W)
}

impl<V,W> NodeDiff<V,W> {

    pub fn get_left(&self) -> Option<&W> {
        match self {
            Left(_) => None,
            Right(a) => Some(a),
            Both(_, a) => Some(a),
        }
    }

    pub fn get_right(&self) -> Option<&W> {
        match self {
            Left(_) => None,
            Right(a) => Some(a),
            Both(_, a) => Some(a),
        }
    }
}

use static_str_ops::staticize;
use NodeDiff::*;

pub fn diff<K: Ord,V, W>(a: Trace<K,V>, b: Trace<K,W>) -> Trace<K,NodeDiff<V, W>> {
    zip_with(a, b, |v,w| match (v,w) {
        (None, None) => None,
        (None, Some(w)) => Some(Right(w)),
        (Some(v), None) => Some(Left(v)),
        (Some(v), Some(w)) => Some(Both(v,w)),
    })
}
pub fn fmap<K: Ord, V, W>(this: BTreeMap<K,V>, f: &impl Fn(V) -> W) -> BTreeMap<K,W> {
    this.into_iter().map(|(k,v)| (k, f(v))).collect()
}

// TODO! This has currently a hardcoded 10!
pub fn map_with_parent<K: Ord, V, W>(mut this: Trace<K,V>, f: impl Fn(&Vec<K>, Option<&W>, V) -> W) -> Trace<K,W> {
    let mut processed = BTreeMap::new();
    for length in (0..10) {
        for (key, value) in this.extract_if(|k,_| k.len() == length) {
            let p;
            if key.len() > 0 {
                let parent_key = &key[0..key.len() - 1];
                p = processed.get(parent_key);
            } else {
                p = None;
            }
            let v = f(&key, p, value);
            processed.insert(key, v);
        }
    }
    processed
}


pub fn get_key_name<K: std::fmt::Display>(key: &Vec<K>) -> &'static str{
    let name = key.last().map(|x| format!("{x}")).unwrap_or("root".into());
    staticize(&name)
}



// impl<K: Ord,V> Trace<K,V> {
//     pub fn fmap<W>(self, f: &impl Fn(V) -> W) -> Trace<K,W> {
//         match self {
//             Composite(x, xs) => Composite(f(x), xs.into_iter().map(|(k,x)| (k, x.fmap(f))).collect()),
//         }
//     }

//     pub fn filter_some(this: Trace<K,Option<V>>) -> Self {
//         match this {
//             Composite(None, btree_map) => Composite(None, BTreeMap::new()),
//             Composite(Some(a), btree_map) => ,
//         }
//     }

// }

// pub fn diff<K: Ord,V: PartialEq + Clone, W: PartialEq + Clone>(a: Option<Trace<K,V>>, b: Option<Trace<K,W>>) -> Option<Trace<K,NodeDiff<V, W>>> {
//     match (a, b) {
//         (None, None) => None,
//         (None, Some(Composite(y, ys))) => Some(Composite(Right(y), ys.into_iter().map(|(k,v)| (k, v.fmap(&Right))).collect())),
//         (Some(Composite(x, xs)), None) => Some(Composite(Left(x), xs.into_iter().map(|(k,v)| (k, v.fmap(&Left))).collect())),
//         (Some(Composite(x, xs)), Some(Composite(y,ys))) => Some(Composite(Both(x,y), {
//             zip_with(xs, ys, |v,w| match (v,w) {
//                 (None, None) => None,
//                 (None, Some(w)) => Some(w.fmap(&Right)),
//                 (Some(v), None) => Some(v.fmap(&Left)),
//                 (Some(v), Some(w)) => diff(Some(v),Some(w)),
//             })
//         })),
//     }
// }

// pub struct TraceFn<K,V,W> {
//     create: Box<dyn Fn(K, Option<W>) -> W>,
//     update: Box<dyn Fn(K,V) -> W>,
//     destroy: Box<dyn Fn(K, V)>,
// }


// ------ helpers -------

fn zip_with<K: Ord,V, W, X>(x: BTreeMap<K, V>, mut y: BTreeMap<K,W>, f: impl Fn(Option<V>, Option<W>) -> Option<X>) -> BTreeMap<K,X> {
    let mut result = BTreeMap::new();
    for (k, v) in x.into_iter() {
        if let Some(x) = f(Some(v), y.remove(&k)) {
            result.insert(k, x);
        } 
    }
    for (k,w) in y.into_iter() {
        if let Some(x) = f(None, Some(w)) {
            result.insert(k, x);
        }
    }
    result
}
