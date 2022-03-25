use crate::*;
use sp_std::marker::PhantomData;

/// My first migration.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {}
