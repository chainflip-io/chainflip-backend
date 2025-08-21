pub struct Registry<Name, Value> {
	entries: BTreeMap<Name, Value>,
}

impl<Name, Value> Registry<Name, Value> {
	pub fn insert(&mut self, name: Name, value: Value) {
		self.entries.insert(name, value);
	}
}
