use core::marker::PhantomData;

pub trait WeightInfo {}

pub struct PalletWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for PalletWeight<T> {}

// For backwards compatibility and tests
impl WeightInfo for () {}
