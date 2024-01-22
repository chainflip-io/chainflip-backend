use frame_support::{
	assert_noop, assert_ok,
	pallet_prelude::DispatchResult,
	traits::{IntegrityTest, OnFinalize, OnIdle, OnInitialize, UnfilteredDispatchable},
	weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::BuildStorage;

/// Convenience trait to link a runtime with its corresponding AllPalletsWithSystem struct.
pub trait HasAllPallets: frame_system::Config {
	type AllPalletsWithSystem: OnInitialize<BlockNumberFor<Self>>
		+ OnIdle<BlockNumberFor<Self>>
		+ OnFinalize<BlockNumberFor<Self>>
		+ IntegrityTest;

	fn on_initialize(block_number: BlockNumberFor<Self>) {
		<Self::AllPalletsWithSystem as OnInitialize<BlockNumberFor<Self>>>::on_initialize(
			block_number,
		);
	}
	fn on_idle(block_number: BlockNumberFor<Self>, weight: Weight) {
		<Self::AllPalletsWithSystem as OnIdle<BlockNumberFor<Self>>>::on_idle(block_number, weight);
	}
	fn on_finalize(block_number: BlockNumberFor<Self>) {
		<Self::AllPalletsWithSystem as OnFinalize<BlockNumberFor<Self>>>::on_finalize(block_number);
	}
	fn integrity_test() {
		<Self::AllPalletsWithSystem as IntegrityTest>::integrity_test();
	}
}

/// Basic [sp_io::TestExternalities] wrapper that provides a richer API for testing pallets.
struct RichExternalities<Runtime>(sp_io::TestExternalities, std::marker::PhantomData<Runtime>);

impl<Runtime: HasAllPallets> RichExternalities<Runtime> {
	fn new(ext: sp_io::TestExternalities) -> Self {
		Self(ext, Default::default())
	}

	/// Executes a closure, preserving the result as test context.
	#[track_caller]
	fn execute_with<Ctx>(mut self, f: impl FnOnce() -> Ctx) -> TestExternalities<Runtime, Ctx> {
		let context = self.0.execute_with(f);
		TestExternalities { ext: self, context }
	}

	/// Increments the block number and executes the closure as a block, including all the runtime
	/// hooks.
	#[track_caller]
	fn execute_at_next_block<Ctx>(
		mut self,
		f: impl FnOnce() -> Ctx,
	) -> TestExternalities<Runtime, Ctx> {
		let block_number =
			self.0.execute_with(|| frame_system::Pallet::<Runtime>::block_number()) + 1u32.into();
		self.execute_at_block::<Ctx>(block_number, f)
	}

	/// Sets the block number and executes the closure as a block, including all the runtime
	/// hooks.
	#[track_caller]
	fn execute_at_block<Ctx>(
		mut self,
		block_number: impl Into<BlockNumberFor<Runtime>>,
		f: impl FnOnce() -> Ctx,
	) -> TestExternalities<Runtime, Ctx> {
		let context = self.0.execute_with(|| {
			let block_number = block_number.into();
			frame_system::Pallet::<Runtime>::reset_events();
			frame_system::Pallet::<Runtime>::set_block_number(block_number);
			Runtime::on_initialize(block_number);
			let context = f();
			Runtime::on_idle(block_number, Weight::MAX);
			Runtime::on_finalize(block_number);
			Runtime::integrity_test();
			context
		});
		TestExternalities { ext: self, context }
	}
}

/// A wrapper around [sp_io::TestExternalities] that provides a richer API for testing pallets.
pub struct TestExternalities<Runtime: HasAllPallets, Ctx = ()> {
	ext: RichExternalities<Runtime>,
	context: Ctx,
}

impl<Runtime> TestExternalities<Runtime>
where
	Runtime: HasAllPallets,
{
	/// Useful for backwards-compatibility. This is equivalent to the context-less execute_with from
	/// [sp_io::TestExternalities].
	#[track_caller]
	pub fn execute_with<Ctx>(self, f: impl FnOnce() -> Ctx) -> TestExternalities<Runtime, Ctx> {
		self.ext.execute_with(f)
	}
}

impl<Runtime, Ctx> TestExternalities<Runtime, Ctx>
where
	Runtime: HasAllPallets,
	Ctx: Clone,
{
	/// Initialises new [TestExternalities] with the given genesis config at block number 1.
	#[track_caller]
	pub fn new<GenesisConfig: BuildStorage>(config: GenesisConfig) -> TestExternalities<Runtime> {
		let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();
		ext.execute_with(|| {
			frame_system::Pallet::<Runtime>::set_block_number(1u32.into());
			Runtime::integrity_test();
		});
		TestExternalities { ext: RichExternalities::new(ext), context: () }
	}

	/// Transforms the test context. Analogous to [std::iter::Iterator::map].
	///
	/// Storage is not accessible in this closure. This means that assert_noop! won't work. If
	/// storage access is required, use `inspect_storage`.
	#[track_caller]
	pub fn map_context<R>(self, f: impl FnOnce(Ctx) -> R) -> TestExternalities<Runtime, R> {
		TestExternalities { ext: self.ext, context: f(self.context) }
	}

	/// Execute a closure. The return value of the closure is preserved as test context.
	#[track_caller]
	pub fn then_execute_with<R>(self, f: impl FnOnce(Ctx) -> R) -> TestExternalities<Runtime, R> {
		let context = self.context;
		self.ext.execute_with(move || f(context))
	}

	/// Access the storage without changing the test context.
	///
	/// Use this for assertions, for example testing invariants.
	#[track_caller]
	pub fn inspect_storage(self, f: impl FnOnce(&Ctx)) -> TestExternalities<Runtime, Ctx> {
		self.then_execute_with(|context| {
			f(&context);
			context
		})
	}

	/// Inspect the test context without accessing storage.
	#[track_caller]
	pub fn inspect_context(self, f: impl FnOnce(&Ctx)) -> TestExternalities<Runtime, Ctx> {
		f(&self.context);
		self
	}

	/// Consume the test externalities and return the context.
	pub fn into_context(self) -> Ctx {
		self.context
	}

	/// Execute the given closure as if it was an extrinsic in the next block.
	///
	/// The closure's return value is next context.
	///
	/// Prefer to use `then_apply_extrinsics` if testing extrinsics.
	#[track_caller]
	pub fn then_execute_at_next_block<R>(
		self,
		f: impl FnOnce(Ctx) -> R,
	) -> TestExternalities<Runtime, R> {
		let context = self.context;
		self.ext.execute_at_next_block(move || f(context))
	}

	/// Execute the given closure as if it was an extrinsic at a specific block number.
	///
	/// The closure's return value is next context.
	#[track_caller]
	pub fn then_execute_at_block<R>(
		self,
		block_number: impl Into<BlockNumberFor<Runtime>>,
		f: impl FnOnce(Ctx) -> R,
	) -> TestExternalities<Runtime, R> {
		let context = self.context;
		self.ext.execute_at_block(block_number, move || f(context))
	}

	/// Execute the given closure against all the runtime events.
	///
	/// The collected closure results are added to the test context.
	#[track_caller]
	pub fn then_process_events<R>(
		self,
		mut f: impl FnMut(Ctx, Runtime::RuntimeEvent) -> Option<R>,
	) -> TestExternalities<Runtime, (Ctx, Vec<R>)> {
		let context = self.context.clone();
		self.ext.execute_with(move || {
			let r = frame_system::Pallet::<Runtime>::events()
				.into_iter()
				.filter_map(|e| f(context.clone(), e.event))
				.collect();
			(context, r)
		})
	}

	/// Applies the provided extrinsics in the next block, asserting the expected result.
	#[allow(clippy::type_complexity)]
	#[track_caller]
	pub fn then_apply_extrinsics<
		C: UnfilteredDispatchable<RuntimeOrigin = Runtime::RuntimeOrigin> + Clone,
		I: IntoIterator<Item = (Runtime::RuntimeOrigin, C, DispatchResult)>,
	>(
		self,
		f: impl FnOnce(&Ctx) -> I,
	) -> TestExternalities<Runtime, Ctx> {
		let r = self.ext.execute_at_next_block(|| {
			for (origin, call, expected_result) in f(&self.context) {
				match expected_result {
					Ok(_) => {
						assert_ok!(call.dispatch_bypass_filter(origin));
					},
					Err(e) => {
						assert_noop!(call.dispatch_bypass_filter(origin), e);
					},
				}
			}
		});
		TestExternalities { ext: r.ext, context: self.context }
	}

	/// Keeps executing pallet hooks until the given predicate returns true.
	///
	/// Preserves the context.
	#[track_caller]
	pub fn then_process_blocks_until<F: Fn(Ctx) -> bool>(mut self, predicate: F) -> Self {
		loop {
			let context = self.context.clone();
			let TestExternalities { ext: next_ext, context: should_break } =
				self.then_execute_with(&predicate);
			let next = Self { ext: next_ext, context };
			if should_break {
				break next
			} else {
				self = next.then_execute_at_next_block(|context| context);
			}
		}
	}

	/// Commits storage changes to the DB
	#[track_caller]
	pub fn commit_all(mut self) -> Self {
		assert_ok!(self.ext.0.commit_all());
		self
	}
}

#[cfg(test)]
mod test_examples {
	use super::*;
	use frame_support::traits::OriginTrait;
	use sp_core::{ConstU16, ConstU64, H256};
	use sp_runtime::{
		traits::{BlakeTwo256, IdentityLookup},
		DispatchError,
	};

	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountId = u64;

	// Configure a mock runtime to test the pallet.
	frame_support::construct_runtime!(
		pub enum Test
		{
			System: frame_system,
		}
	);

	impl frame_system::Config for Test {
		type BaseCallFilter = frame_support::traits::Everything;
		type BlockWeights = ();
		type BlockLength = ();
		type DbWeight = ();
		type RuntimeOrigin = RuntimeOrigin;
		type RuntimeCall = RuntimeCall;
		type Nonce = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Block = Block;
		type RuntimeEvent = RuntimeEvent;
		type BlockHashCount = ConstU64<250>;
		type Version = ();
		type PalletInfo = PalletInfo;
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type SystemWeightInfo = ();
		type SS58Prefix = ConstU16<42>;
		type OnSetCode = ();
		type MaxConsumers = frame_support::traits::ConstU32<5>;
	}

	impl HasAllPallets for Test {
		type AllPalletsWithSystem = AllPalletsWithSystem;
	}

	// Note AllPalletsWithSystem is an alias generated by the construct_runtime macro.
	fn new_test_ext() -> TestExternalities<Test> {
		TestExternalities::<Test>::new(RuntimeGenesisConfig::default())
	}

	const ALICE: u64 = 1;

	#[test]
	fn example_1() {
		new_test_ext()
			.execute_with(|| {
				// First block is always one.
				assert_eq!(System::block_number(), 1);
				"HELLO"
			})
			.then_execute_with(|context| {
				assert_eq!(
					context, "HELLO",
					"The result of the previous closure is passed through to the next"
				);
				assert_eq!(
					System::block_number(),
					1,
					"Block number does not increment unless we use the at_block/as_block methods."
				);
				System::block_number()
			})
			.then_execute_at_next_block(|n| {
				assert_eq!(
					System::block_number(),
					n + 1,
					"Block number should increment when we execute_at_next_block."
				);
			})
			// This can be useful for testing eg. expiry logic.
			.then_execute_at_block(10u32, |_| {
				assert_eq!(
					System::block_number(),
					10,
					"Block numbers are skipped when we execute_at_block."
				);
				"HeyHey"
			})
			// Alternatively, we can use `then_process_blocks_until` to execute blocks until some
			// condition is met.
			.then_process_blocks_until(|_| {
				assert!(System::block_number() <= 20);
				System::block_number() == 20
			})
			.then_apply_extrinsics(|_| {
				[
					(
						OriginTrait::signed(ALICE),
						RuntimeCall::from(frame_system::Call::remark_with_event {
							remark: vec![1, 2, 3],
						}),
						Ok(()),
					),
					// None is not a valid origin so this should fail:
					(
						OriginTrait::none(),
						RuntimeCall::from(frame_system::Call::remark_with_event {
							remark: vec![1, 2, 3],
						}),
						Err(DispatchError::BadOrigin),
					),
				]
			})
			// Use inspect when you don't want to write to storage.
			.inspect_storage(|_| {
				assert!(matches!(
					System::events().into_iter().map(|e| e.event).collect::<Vec<_>>().as_slice(),
					[
						RuntimeEvent::System(frame_system::Event::Remarked { sender, .. }),
					]
					if *sender == ALICE
				));
			})
			.then_process_events(|previous_result, event| {
				assert_eq!(previous_result, "HeyHey", "Context has been passed through.");
				assert_eq!(System::block_number(), 21, "We processed up to the desired block.");
				match event {
					RuntimeEvent::System(system_event) => match system_event {
						frame_system::Event::Remarked { sender, .. } => Some(sender),
						_ => None,
					},
				}
			})
			.inspect_context(|(previous_result, event_results)| {
				assert_eq!(
					*previous_result, "HeyHey",
					"Context has been passed through from previous then_execute_at_block closure."
				);
				assert_eq!(event_results[..], vec![ALICE]);
			});
	}
}
