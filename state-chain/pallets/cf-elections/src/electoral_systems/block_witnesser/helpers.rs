use core::{iter::Step, ops::RangeInclusive};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

pub trait Implies {
	fn implies(self, other: Self) -> Self;
}
impl Implies for bool {
	fn implies(self, other: Self) -> Self {
		// false implies false (false <= false)
		// false implies true  (false <= true)
		// true implies true   (true <= true)
		// true !implies false (! true <= false)
		self <= other
	}
}

// ------------ my helpers ---------------
pub trait KeySet<K> {
	fn key_set(&self) -> BTreeSet<K>;
}

impl<K: Ord + Clone, V> KeySet<K> for BTreeMap<K, V> {
	fn key_set(&self) -> BTreeSet<K> {
		self.keys().map(Clone::clone).collect()
	}
}

pub trait With<K> {
	fn with(self, k: K) -> Self;
	fn without(self, k: K) -> Self;
}

impl<K: Ord> With<K> for BTreeSet<K> {
	fn with(mut self, k: K) -> Self {
		self.insert(k);
		self
	}

	fn without(mut self, k: K) -> Self {
		self.remove(&k);
		self
	}
}

pub trait Merge {
	fn merge(self, other: Self) -> Self;
}

impl<K: Ord> Merge for BTreeSet<K> {
	fn merge(mut self, mut rhs: BTreeSet<K>) -> Self {
		self.append(&mut rhs);
		self
	}
}

pub trait IntoSet<X> {
	fn into_set(self) -> BTreeSet<X>;
}

impl<N: Step + Ord> IntoSet<N> for RangeInclusive<N> {
	fn into_set(self) -> BTreeSet<N> {
		BTreeSet::from_iter(self.into_iter())
	}
}

#[macro_export]
macro_rules! prop_do {
    (let $var:pat in $expr:expr; $($expr2:tt)+) => {
        $expr.prop_flat_map(move |$var| prop_do!($($expr2)+))
    };
    (return $($rest:tt)+) => {
        Just($($rest)+)
    };
    ($expr:expr) => {$expr};
    (let $var:pat = $expr:expr; $($expr2:tt)+ ) => {
        {
            let $var = $expr;
            prop_do!($($expr2)+)
        }
    };
    ($var:ident <<= $expr:expr; $($expr2:tt)+) => {
        $expr.prop_flat_map(move |$var| prop_do!($($expr2)+))
    };
}

#[macro_export]
macro_rules! asserts {
	($description:tt in $expr:expr; $($tail:tt)*) => {
		assert!($expr, $description);
		asserts!{$($tail)*}
	};
	($description:tt in $expr:expr, where {$($tt:tt)*} $($tail:tt)*) => {
		{
			$($tt)*
			assert!($expr, $description);
		}
		asserts!{$($tail)*}
	};
	(let $ident:ident = $expr:expr; $($tail:tt)*) => {
		let $ident = $expr;
		asserts!{$($tail)*}
	};
	() => {}
}
