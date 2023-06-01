use frame_support::{
	dispatch::UnfilteredDispatchable,
	pallet_prelude::DispatchResultWithPostInfo,
	traits::{OnFinalize, OnIdle, OnInitialize},
	weights::Weight,
};
use sp_runtime::BuildStorage;

/// Basic [sp_io::TestExternalities] wrapper that provides a richer API for testing pallets.
struct RichExternalities<Runtime>(sp_io::TestExternalities, std::marker::PhantomData<Runtime>);

impl<
		Runtime: frame_system::Config,
		Pallets: OnInitialize<Runtime::BlockNumber>
			+ OnIdle<Runtime::BlockNumber>
			+ OnFinalize<Runtime::BlockNumber>,
	> RichExternalities<(Runtime, Pallets)>
{
	fn new(ext: sp_io::TestExternalities) -> Self {
		Self(ext, Default::default())
	}

	/// Executes a closure, preserving the result as test context.
	#[track_caller]
	fn execute_with<Ctx>(
		mut self,
		f: impl FnOnce() -> Ctx,
	) -> TestExternalities<Runtime, Pallets, Ctx> {
		let context = self.0.execute_with(f);
		TestExternalities { ext: self, context }
	}

	/// Increments the block number and executes the closure as a block, including all the runtime
	/// hooks.
	#[track_caller]
	fn execute_as_next_block<Ctx>(
		mut self,
		f: impl FnOnce() -> Ctx,
	) -> TestExternalities<Runtime, Pallets, Ctx> {
		let block_number =
			self.0.execute_with(|| frame_system::Pallet::<Runtime>::block_number()) + 1u32.into();
		self.execute_at_block::<Ctx>(block_number, f)
	}

	/// Sets the block number and executes the closure as a block, including all the runtime
	/// hooks.
	#[track_caller]
	fn execute_at_block<Ctx>(
		mut self,
		block_number: impl Into<Runtime::BlockNumber>,
		f: impl FnOnce() -> Ctx,
	) -> TestExternalities<Runtime, Pallets, Ctx> {
		let context = self.0.execute_with(|| {
			let block_number = block_number.into();
			frame_system::Pallet::<Runtime>::reset_events();
			frame_system::Pallet::<Runtime>::set_block_number(block_number);
			Pallets::on_initialize(block_number);
			let context = f();
			Pallets::on_idle(block_number, Weight::MAX);
			Pallets::on_finalize(block_number);
			context
		});
		TestExternalities { ext: self, context }
	}
}

/// A wrapper around [sp_io::TestExternalities] that provides a richer API for testing pallets.
pub struct TestExternalities<Runtime, Pallets, Ctx = ()> {
	ext: RichExternalities<(Runtime, Pallets)>,
	context: Ctx,
}

impl<Runtime, Pallets> TestExternalities<Runtime, Pallets>
where
	Runtime: frame_system::Config,
	Pallets: OnInitialize<Runtime::BlockNumber>
		+ OnIdle<Runtime::BlockNumber>
		+ OnFinalize<Runtime::BlockNumber>,
{
	/// Useful for backwards-compatibility. This is equivalent to the context-less execute_with from
	/// [sp_io::TestExternalities].
	#[track_caller]
	pub fn execute_with<Ctx>(
		self,
		f: impl FnOnce() -> Ctx,
	) -> TestExternalities<Runtime, Pallets, Ctx> {
		self.ext.execute_with(f)
	}
}

impl<Runtime, Pallets, Ctx> TestExternalities<Runtime, Pallets, Ctx>
where
	Runtime: frame_system::Config,
	Pallets: OnInitialize<Runtime::BlockNumber>
		+ OnIdle<Runtime::BlockNumber>
		+ OnFinalize<Runtime::BlockNumber>,
	Ctx: Clone,
{
	/// Initialises a new TestExternalities with the given genesis config at block number 1.
	#[track_caller]
	pub fn new<GenesisConfig: BuildStorage>(
		config: GenesisConfig,
	) -> TestExternalities<Runtime, Pallets> {
		let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();
		ext.execute_with(|| {
			frame_system::Pallet::<Runtime>::set_block_number(1u32.into());
		});
		TestExternalities { ext: RichExternalities::new(ext), context: () }
	}

	#[track_caller]
	/// Transforms the test context.
	pub fn map_context<R>(
		self,
		f: impl FnOnce(Ctx) -> R,
	) -> TestExternalities<Runtime, Pallets, R> {
		TestExternalities { ext: self.ext, context: f(self.context) }
	}

	/// Execute a closure. The return value is preserved as test context.
	#[track_caller]
	pub fn then_execute_with<R>(
		self,
		f: impl FnOnce(Ctx) -> R,
	) -> TestExternalities<Runtime, Pallets, R> {
		let context = self.context;
		self.ext.execute_with(move || f(context))
	}

	/// Access the storage without touching test context.
	#[track_caller]
	pub fn inspect_storage(self, f: impl FnOnce(&Ctx)) -> TestExternalities<Runtime, Pallets, Ctx> {
		self.then_execute_with(|context| {
			f(&context);
			context
		})
	}

	/// Inspect the test context without accessing storage.
	#[track_caller]
	pub fn inspect_context(self, f: impl FnOnce(&Ctx)) -> TestExternalities<Runtime, Pallets, Ctx> {
		f(&self.context);
		self
	}

	/// Execute the given closure as the next block.
	///
	/// The closure's return value is next context.
	///
	/// Prefer to use `then_apply_extrinsics` if testing extrinsics.
	#[track_caller]
	pub fn then_execute_as_next_block<R>(
		self,
		f: impl FnOnce(Ctx) -> R,
	) -> TestExternalities<Runtime, Pallets, R> {
		let context = self.context;
		self.ext.execute_as_next_block(move || f(context))
	}

	/// Execute the given closure at a specific block number.
	///
	/// The closure's return value is next context.
	#[track_caller]
	pub fn then_execute_at_block<R>(
		self,
		block_number: impl Into<Runtime::BlockNumber>,
		f: impl FnOnce(Ctx) -> R,
	) -> TestExternalities<Runtime, Pallets, R> {
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
	) -> TestExternalities<Runtime, Pallets, (Ctx, Vec<R>)> {
		let context = self.context.clone();
		self.ext.execute_with(move || {
			let r = frame_system::Pallet::<Runtime>::events()
				.into_iter()
				.filter_map(|e| f(context.clone(), e.event))
				.collect();
			(context, r)
		})
	}

	/// Applies the provided extrinsics in a block.
	///
	/// Adds a Vec of tuples containing each call and its result to the test context.
	#[allow(clippy::type_complexity)]
	#[track_caller]
	pub fn then_apply_extrinsics<
		C: UnfilteredDispatchable<RuntimeOrigin = Runtime::RuntimeOrigin> + Clone,
		I: IntoIterator<Item = (Runtime::RuntimeOrigin, C)>,
	>(
		self,
		f: impl FnOnce(&Ctx) -> I,
	) -> TestExternalities<Runtime, Pallets, (Ctx, Vec<(C, DispatchResultWithPostInfo)>)> {
		let r = self.ext.execute_as_next_block(|| {
			f(&self.context)
				.into_iter()
				.map(|(origin, call)| (call.clone(), call.dispatch_bypass_filter(origin)))
				.collect()
		});
		TestExternalities { ext: r.ext, context: (self.context, r.context) }
	}

	/// Keeps executing blocks until the given predicate returns true.
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
				self = next.then_execute_as_next_block(|context| context);
			}
		}
	}
}

#[cfg(test)]
mod test_examples {
	use super::*;
	use frame_support::traits::OriginTrait;
	use sp_core::{ConstU16, ConstU64, H256};
	use sp_runtime::{
		testing::Header,
		traits::{BlakeTwo256, IdentityLookup},
	};

	type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountId = u64;

	// Configure a mock runtime to test the pallet.
	frame_support::construct_runtime!(
		pub enum Test where
			Block = Block,
			NodeBlock = Block,
			UncheckedExtrinsic = UncheckedExtrinsic,
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
		type Index = u64;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
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

	// Note AllPalletsWithSystem is an alias generated by the construct_runtime macro.
	fn new_test_ext() -> TestExternalities<Test, AllPalletsWithSystem> {
		TestExternalities::<Test, AllPalletsWithSystem>::new(GenesisConfig::default())
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
			.then_execute_as_next_block(|n| {
				assert_eq!(
					System::block_number(),
					n + 1,
					"Block number should increment when we execute_as_next_block."
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
			// Alternatively, we can use `then_process_blocks_until` to execute blocks until a
			.then_process_blocks_until(|_| System::block_number() == 20)
			.then_apply_extrinsics(|_| {
				[
					(
						OriginTrait::signed(ALICE),
						RuntimeCall::from(frame_system::Call::remark_with_event {
							remark: vec![1, 2, 3],
						}),
					),
					// None is not a valid origin so this should fail:
					(
						OriginTrait::none(),
						RuntimeCall::from(frame_system::Call::remark_with_event {
							remark: vec![1, 2, 3],
						}),
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
			.then_process_events(|(previous_result, extrinsic_results), event| {
				assert_eq!(previous_result, "HeyHey", "Context has been passed through.");
				assert_eq!(System::block_number(), 21, "We processed up to the desired block.");
				assert_eq!(extrinsic_results.len(), 2, "We have two extrinsic results.");
				assert_eq!(
					extrinsic_results.iter().filter(|(_call, result)| result.is_ok()).count(),
					1,
					"One of them should have succeeded."
				);
				match event {
					RuntimeEvent::System(system_event) => match system_event {
						frame_system::Event::Remarked { sender, .. } => Some(sender),
						_ => None,
					},
				}
			})
			.inspect_context(|((previous_result, extrinsic_results), event_results)| {
				assert_eq!(extrinsic_results.len(), 2, "We have two extrinsic results.");
				assert_eq!(
					*previous_result, "HeyHey",
					"Context has been passed through from previous then_execute_at_block closure."
				);
				assert_eq!(event_results[..], vec![ALICE]);
			});
	}
}
