pub trait HasTypeIntrospection {
	fn is_empty_type() -> bool;
}

// -------------- primitives ---------------

#[duplicate::duplicate_item(Type; [()]; [bool]; [u8]; [u16]; [u32]; [u64]; [u128])]
impl HasTypeIntrospection for Type {
	fn is_empty_type() -> bool {
		false
	}
}

impl<A> HasTypeIntrospection for sp_std::marker::PhantomData<A> {
	fn is_empty_type() -> bool {
		false
	}
}
