#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use codec::Codec;
    use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	use sp_runtime::{PerThing, Percent, app_crypto::RuntimePublic};

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config
	{
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		// Note: `Member` is defined in frame_support::pallet_prelude and is a helper trait for
		// arguments to callables defined under the `pallet:call` macro.
		type Amount: Member + Codec;
		type AutoSwap: Member + Codec;
		type Bips: Member + Codec + PerThing;
		type BlockHash: Member + Codec;
		type BlockNumber: Member + Codec;
		type Chain: Member + Codec;
		type Crypto: Member + Codec + RuntimePublic;
		type EthereumPubKey: Member + Codec + RuntimePublic;
		type LiquidityPubKey: Member + Codec + RuntimePublic;
		type OutputAddress: Member + Codec;
		type OutputId: Member + Codec;
		type QuoteId: Member + Codec;
		type SlashData: Member + Codec;
		type SlashReason: Member + Codec;
		type Ticker: Member + Codec;
		type TxHash: Member + Codec;
	}

	// Short-hand type definitions.
	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
			// TODO_MAYBE_WHERE_CLAUSE
	{
			// TODO_ON_FINALIZE
			// TODO_ON_INITIALIZE
			// TODO_ON_RUNTIME_UPGRADE
			// TODO_INTEGRITY_TEST
			// TODO_OFFCHAIN_WORKER
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	{
		/// Record a quote for a swap on the state chain.
		#[pallet::weight(10_000 )]
		pub fn quote_swap(
			origin: OriginFor<T>, 
			incoming_asset: T::Ticker,
			outgoing_asset: T::Ticker,
			outgoing_address: T::OutputAddress,
			max_slippage_bips: Option<T::Bips>,
			refund_address: Option<T::OutputAddress>,
			auth_public_key: Option<T::LiquidityPubKey>
		) -> DispatchResultWithPostInfo {
			// Check that the extrinsic was signed and get the signer.
			// This function will return an error if the extrinsic is not signed.
			// https://substrate.dev/docs/en/knowledgebase/runtime/origin
			let _who = ensure_signed(origin)?;
			
			todo!()
		}

		/// Record a quote for liquidity provisioning on the state chain.
		#[pallet::weight(10_000)]
		pub fn quote_provision(
			origin: OriginFor<T>,
			base_asset: T::Ticker,
			pair_asset: T::Ticker,
			base_asset_refund_address: T::OutputAddress,
			pair_asset_refund_address: T::OutputAddress,
			auto_swap: T::AutoSwap,
			auth_public_key: T::LiquidityPubKey,
			max_slippage_bips: Option<T::Bips>
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Records a witness on the state chain pursuant to a particular quote.
		#[pallet::weight(10_000)]
		pub fn witness(
			origin: OriginFor<T>,
			asset: T::Ticker,
			atomic_amount: T::Amount,
			block_number: <T as Config>::BlockNumber,
			block_hash: T::BlockHash,
			transaction_hash: T::TxHash,
			quote_id: Option<T::QuoteId>
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Claim a refund if your swap could not be processed.
		#[pallet::weight(10_000)]
		pub fn claim_swap_refund(
			origin: OriginFor<T>,
			quote_id: T::QuoteId,
			refund_asset_address: T::OutputAddress,
			signature: <T::LiquidityPubKey as RuntimePublic>::Signature
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Claim the liquidity that you have deposited with an optional implicit
		/// swap if base_asset_withdraw_percent and pair_asset_withdraw_percent are not the
		/// same value. LPs can also withdraw to neither the base or pair assets by
		/// swapping to the desired asset.
		#[pallet::weight(10_000)]
		pub fn claim_provision(
			origin: OriginFor<T>,
			auth_public_key: T::LiquidityPubKey,
			base_asset: T::Ticker,
			pair_asset: T::Ticker,
			base_asset_withdraw_percent: Percent,
			pair_asset_withdraw_percent: Percent,
			nonce: u32,
			signature: <T::LiquidityPubKey as RuntimePublic>::Signature,
			output_asset: Option<T::Ticker>,
			base_asset_address: Option<T::OutputAddress>,
			pair_asset_address: Option<T::OutputAddress>,
			output_asset_address: Option<T::OutputAddress>,
			max_slippage_bips: Option<T::Bips>
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Saves an output "batch" on the state chain and nominates a sender to kick off the signing processing.
		/// It should be the final extrinsic processed in any block, and there should only be one instance per block.
		/// this extrinsic should result in an unsigned transaction being recorded on the state chain.
		#[pallet::weight(10_000)]
		pub fn batch_outputs(
			origin: OriginFor<T>,
			output_ids: Vec<T::OutputId>,
			base_chain: T::Chain,
			gas_fee: u32
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Get FLIP that is held for me by the system, signed by validator key.
		#[pallet::weight(10_000)]
		pub fn claim_flip(
			origin: OriginFor<T>,
			percentage: Percent
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Record a liveness proof for a validator
		#[pallet::weight(10_000)]
		pub fn i_am_online(
			origin: OriginFor<T>,
			latest_block_hash: T::BlockHash,
			signatures: Vec<(T::Chain, <T::Crypto as RuntimePublic>::Signature)>
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Called as a witness for a new stake submitted through the StakeManager contract.
		#[pallet::weight(10_000)]
		pub fn stake(
			origin: OriginFor<T>,
			validator_id: T::AccountId,
			staked_amount: T::Amount,
			eth_pubkey: T::EthereumPubKey,
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Called by nodes who want to unbond their stake at the end of this vault's life.
		#[pallet::weight(10_000)]
		pub fn unstake(
			origin: OriginFor<T>,
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Slash someone. 
		#[pallet::weight(10_000)]
		pub fn slash(
			origin: OriginFor<T>,
			validator_id: T::AccountId,
			reason: T::SlashReason,
			data: Option<T::SlashData>
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Start the creation of a new vault. 
		#[pallet::weight(10_000)]
		pub fn create_vault(
			origin: OriginFor<T>,
			chain: T::Chain
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}

		/// Start the rotation of funds to a new vault.
		#[pallet::weight(10_000)]
		pub fn rotate_vault(
			origin: OriginFor<T>,
			chain: T::Chain
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			todo!()
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [something, who]
		SomethingStored(u32, AccountId<T>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
	}

	#[pallet::storage]
	#[pallet::getter(fn something)]
	pub(super) type Something<T: Config> = StorageValue<_, u32>;
}
