//! Well typed registry for types with generics

use std::collections::BTreeMap;

trait Term {
	type WithVars<A>;

	// fn sub(t: Term<A>, )
}

struct Registry<Name, Value> {
	values: BTreeMap<Name, Value>,
}
