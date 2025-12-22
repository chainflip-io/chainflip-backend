use super::*;

#[derive(Clone, Copy, Debug)]
enum FetchedPrice {
	Valid(Price),
	Stale(Price),
	Invalid,
}

#[derive(DefaultNoBound)]
pub struct OraclePriceCache<T> {
	cached_prices: core::cell::RefCell<BTreeMap<Asset, FetchedPrice>>,
	_phantom: PhantomData<T>,
}

impl<T: Config> OraclePriceCache<T> {
	fn get_price_inner(&self, asset: Asset, allow_stale_price: bool) -> Result<Price, Error<T>> {
		use sp_std::collections::btree_map::Entry;

		// `borrow_mut` is safe because we don't create any more references while holding it
		let cached_price = match self.cached_prices.borrow_mut().entry(asset) {
			Entry::Vacant(entry) => {
				// Price has never been requested this block, so we try to fetch it
				if let Some(oracle_price) = T::PriceApi::get_price(asset) {
					if oracle_price.price == Default::default() {
						*entry.insert(FetchedPrice::Invalid)
					} else if oracle_price.stale {
						*entry.insert(FetchedPrice::Stale(oracle_price.price))
					} else {
						*entry.insert(FetchedPrice::Valid(oracle_price.price))
					}
				} else {
					*entry.insert(FetchedPrice::Invalid)
				}
			},
			Entry::Occupied(price) => *price.get(),
		};

		match cached_price {
			FetchedPrice::Valid(price) => Ok(price),
			FetchedPrice::Invalid => Err(Error::<T>::OraclePriceUnavailable),
			FetchedPrice::Stale(price) =>
				if allow_stale_price {
					Ok(price)
				} else {
					Err(Error::<T>::OraclePriceUnavailable)
				},
		}
	}

	pub fn get_price(&self, asset: Asset) -> Result<Price, Error<T>> {
		self.get_price_inner(asset, false)
	}

	pub fn get_price_allow_stale(&self, asset: Asset) -> Result<Price, Error<T>> {
		self.get_price_inner(asset, true)
	}

	fn usd_value_inner(
		&self,
		asset: Asset,
		amount: AssetAmount,
		allow_stale_price: bool,
	) -> Result<AssetAmount, Error<T>> {
		let price_in_usd = if allow_stale_price {
			self.get_price_allow_stale(asset)?
		} else {
			self.get_price(asset)?
		};
		Ok(price_in_usd.output_amount_ceil(amount.into()).unique_saturated_into())
	}

	/// Uses oracle prices to calculate the USD value of the given asset amount
	pub fn usd_value_of(&self, asset: Asset, amount: AssetAmount) -> Result<AssetAmount, Error<T>> {
		self.usd_value_inner(asset, amount, false)
	}

	/// Uses oracle prices to calculate the USD value of the given asset amount, even if the price
	/// is stale.
	pub fn usd_value_of_allow_stale(
		&self,
		asset: Asset,
		amount: AssetAmount,
	) -> Result<AssetAmount, Error<T>> {
		self.usd_value_inner(asset, amount, true)
	}

	fn total_usd_value_of_inner(
		&self,
		assets_amounts: &BTreeMap<Asset, AssetAmount>,
		allow_stale_price: bool,
	) -> Result<AssetAmount, DispatchError> {
		let mut total_collateral_usd = 0;
		for (asset, amount) in assets_amounts {
			if allow_stale_price {
				total_collateral_usd
					.saturating_accrue(self.usd_value_of_allow_stale(*asset, *amount)?);
			} else {
				total_collateral_usd.saturating_accrue(self.usd_value_of(*asset, *amount)?);
			}
		}

		Ok(total_collateral_usd)
	}

	// Uses oracle prices to calculate the total USD value of the entire map of assets
	pub fn total_usd_value_of(
		&self,
		assets_amounts: &BTreeMap<Asset, AssetAmount>,
	) -> Result<AssetAmount, DispatchError> {
		self.total_usd_value_of_inner(assets_amounts, false)
	}

	// Uses oracle prices to calculate the total USD value of the entire map of assets, even if one
	// or more assets has a stale price.
	pub fn total_usd_value_of_allow_stale(
		&self,
		assets_amounts: &BTreeMap<Asset, AssetAmount>,
	) -> Result<AssetAmount, DispatchError> {
		self.total_usd_value_of_inner(assets_amounts, true)
	}

	/// Uses oracle prices to calculate the amount of `asset` that's equivalent in USD value to
	/// `amount` of USD
	pub fn amount_from_usd_value(
		&self,
		asset: Asset,
		usd_value: AssetAmount,
	) -> Result<AssetAmount, Error<T>> {
		// The "price" of USD in terms of the asset:
		let price = self.get_price(asset)?.invert();
		Ok(price.output_amount_ceil(usd_value.into()).unique_saturated_into())
	}
}
