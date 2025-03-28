use std::collections::{BTreeMap, BTreeSet};


pub trait Logical {
    fn and(self, other: Self) -> Self;
    fn or(self, other: Self) -> Self;
    // fn diff(self, other: Self) -> Self;
}

#[derive(PartialEq, Eq, Debug)]
pub struct Container<A: Logical>(pub A);

impl<A: Logical> sp_std::ops::BitOr for Container<A> {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Container(self.0.or(rhs.0))
    }
}

impl<A: Logical> sp_std::ops::BitAnd for Container<A> {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Container(self.0.and(rhs.0))
    }
}

impl<A: Logical> sp_std::ops::Add for Container<A> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Container(self.0.or(rhs.0))
    }

}

impl<A, B> FromIterator<B> for Container<A> 
where A: Logical + FromIterator<B>
{
    fn from_iter<T: IntoIterator<Item = B>>(iter: T) -> Self {
        Container(A::from_iter(iter))
    }
}

// set

impl<A: Clone + Ord> Logical for BTreeSet<A> {
    fn and(self, other: Self) -> Self {
        self.intersection(&other).cloned().collect()
    }

    fn or(self, other: Self) -> Self {
        self.union(&other).cloned().collect()
    }
}

// multi set

#[derive(PartialEq, Eq, Debug)]
pub struct BTreeMultiSet<A>(pub BTreeMap<A, usize>);

impl<A> Default for BTreeMultiSet<A> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<A: Ord> BTreeMultiSet<A> {
    pub fn insert(&mut self, a: A) {
        *self.0.entry(a).or_insert(0) += 1;
    }
}

impl<A: Ord> FromIterator<A> for BTreeMultiSet<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut result = Self::default();
        for x in iter {
            result.insert(x);
        }
        result
    }
}

impl<A: Ord + Clone> Logical for BTreeMultiSet<A> {
    fn and(self, other: Self) -> Self {
        let mut result = BTreeMap::new();
        for (a, n) in &self.0 {
            let m = other.0.get(a).unwrap_or(&0);
            let min = sp_std::cmp::min(m, n);
            if *min > 0 {
                result.insert(a.clone(), *min);
            }
        }
        BTreeMultiSet(result)
    }

    fn or(self, other: Self) -> Self {
        let mut result = self.0.clone();
        for (a, n) in &other.0 {
            *result.entry(a.clone()).or_insert(0) += n;
        }
        BTreeMultiSet(result)
    }

    // fn diff(self, other: Self) -> Self {
    //     todo!()
    // }
}


