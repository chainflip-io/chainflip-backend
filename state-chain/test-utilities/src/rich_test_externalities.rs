use core::fmt::Debug;
use frame_support::{
	assert_noop, assert_ok,
	pallet_prelude::DispatchResult,
	traits::{IntegrityTest, OnFinalize, OnIdle, OnInitialize, OriginTrait},
	weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::H256;
use sp_runtime::{
	traits::{CheckedSub, Dispatchable, UniqueSaturatedInto},
	BuildStorage, DispatchError, StateVersion,
};

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

/// Basic [sp_state_machine::TestExternalities] wrapper that provides a richer API for testing
/// pallets.
struct RichExternalities<Runtime: frame_system::Config>(
	sp_state_machine::TestExternalities<Runtime::Hashing>,
	std::marker::PhantomData<Runtime>,
);

impl<Runtime: HasAllPallets + frame_system::Config> RichExternalities<Runtime> {
	fn new(ext: sp_state_machine::TestExternalities<Runtime::Hashing>) -> Self {
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
		let block_number = self.0.execute_with(
			#[track_caller]
			|| frame_system::Pallet::<Runtime>::block_number(),
		) + 1u32.into();
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
		let context = self.0.execute_with(
			#[track_caller]
			|| {
				let block_number = block_number.into();
				frame_system::Pallet::<Runtime>::reset_events();
				frame_system::Pallet::<Runtime>::set_block_number(block_number);
				Runtime::on_initialize(block_number);
				let context = f();
				Runtime::on_idle(block_number, Weight::MAX);
				Runtime::on_finalize(block_number);
				Runtime::integrity_test();
				context
			},
		);
		TestExternalities { ext: self, context }
	}
}

/// A wrapper around [sp_state_machine::TestExternalities] that provides a richer API for testing
/// pallets.
pub struct TestExternalities<Runtime: HasAllPallets + frame_system::Config, Ctx = ()> {
	ext: RichExternalities<Runtime>,
	context: Ctx,
}

impl<Runtime, Ctx> AsRef<sp_state_machine::TestExternalities<Runtime::Hashing>>
	for TestExternalities<Runtime, Ctx>
where
	Runtime: HasAllPallets + frame_system::Config,
{
	fn as_ref(&self) -> &sp_state_machine::TestExternalities<Runtime::Hashing> {
		&self.ext.0
	}
}

impl<Runtime> TestExternalities<Runtime>
where
	Runtime: HasAllPallets + frame_system::Config,
{
	/// Initialises new [TestExternalities] with the given genesis config at block number 1.
	#[track_caller]
	pub fn new<GenesisConfig: BuildStorage>(config: GenesisConfig) -> Self {
		let mut ext: sp_state_machine::TestExternalities<Runtime::Hashing> =
			config.build_storage().unwrap().into();
		ext.execute_with(|| {
			frame_system::Pallet::<Runtime>::set_block_number(1u32.into());
			Runtime::integrity_test();
		});
		TestExternalities { ext: RichExternalities::new(ext), context: () }
	}

	pub fn from_raw_snapshot(
		raw_storage: Vec<(Vec<u8>, (Vec<u8>, i32))>,
		storage_root: Runtime::Hash,
		state_version: StateVersion,
	) -> Self {
		sp_state_machine::TestExternalities::from_raw_snapshot(
			raw_storage,
			storage_root,
			state_version,
		)
		.into()
	}

	/// Useful for backwards-compatibility. This is equivalent to the context-less execute_with from
	/// [sp_state_machine::TestExternalities].
	#[track_caller]
	pub fn execute_with<Ctx>(self, f: impl FnOnce() -> Ctx) -> TestExternalities<Runtime, Ctx> {
		self.ext.execute_with(f)
	}
}

impl<Runtime> From<sp_state_machine::TestExternalities<Runtime::Hashing>>
	for TestExternalities<Runtime>
where
	Runtime: HasAllPallets + frame_system::Config,
{
	fn from(ext: sp_state_machine::TestExternalities<Runtime::Hashing>) -> Self {
		TestExternalities { ext: RichExternalities::new(ext), context: () }
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
		ext.execute_with(
			#[track_caller]
			|| {
				frame_system::Pallet::<Runtime>::set_block_number(1u32.into());
				Runtime::integrity_test();
			},
		);
		TestExternalities { ext: RichExternalities::new(ext), context: () }
	}

	/// Transforms the test context. Analogous to [std::iter::Iterator::map].
	///
	/// Storage is not accessible in this closure. This means that assert_noop! won't work. If
	/// storage access is required, use `then_execute_with` or `then_execute_with_keep_context`.
	#[track_caller]
	pub fn map_context<R>(self, f: impl FnOnce(Ctx) -> R) -> TestExternalities<Runtime, R> {
		TestExternalities { ext: self.ext, context: f(self.context) }
	}

	/// Execute a closure. The return value of the closure is preserved as test context.
	#[track_caller]
	pub fn then_execute_with<R>(self, f: impl FnOnce(Ctx) -> R) -> TestExternalities<Runtime, R> {
		let context = self.context;
		self.ext.execute_with(
			#[track_caller]
			move || f(context),
		)
	}

	/// Access the storage without changing the test context.
	///
	/// Use this when you want to read or mutate the storage without
	/// changing the test context. Also useful for assertions,
	/// for example for testing invariants.
	#[track_caller]
	pub fn then_execute_with_keep_context(
		self,
		f: impl FnOnce(&Ctx),
	) -> TestExternalities<Runtime, Ctx> {
		self.then_execute_with(
			#[track_caller]
			|context| {
				f(&context);
				context
			},
		)
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

	pub fn context(&self) -> &Ctx {
		&self.context
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
		self.ext.execute_at_next_block(
			#[track_caller]
			move || f(context),
		)
	}

	/// Process the next `n` blocks, including hooks.
	#[track_caller]
	pub fn then_process_blocks(mut self, n: u32) -> TestExternalities<Runtime, Ctx> {
		for _ in 0..n {
			self = self.then_process_next_block();
		}
		self
	}

	/// Keep processing blocks up to and including the given block number.
	pub fn then_process_blocks_until_block(
		mut self,
		block_number: impl Into<BlockNumberFor<Runtime>>,
	) -> Self {
		let current_block =
			self.ext.0.execute_with(|| frame_system::Pallet::<Runtime>::block_number());
		let target_block: BlockNumberFor<Runtime> = block_number.into();
		self.then_process_blocks(
			target_block
				.checked_sub(&current_block)
				.expect("cannot rewind blocks")
				.unique_saturated_into(),
		)
	}

	/// Process the next block, including hooks.
	#[track_caller]
	pub fn then_process_next_block(self) -> TestExternalities<Runtime, Ctx> {
		self.then_execute_at_next_block(|context| context)
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
		self.ext.execute_at_block(
			block_number,
			#[track_caller]
			move || f(context),
		)
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
		self.ext.execute_with(
			#[track_caller]
			move || {
				let r = frame_system::Pallet::<Runtime>::events()
					.into_iter()
					.filter_map(|e| f(context.clone(), e.event))
					.collect();
				(context, r)
			},
		)
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
				self = next.then_process_next_block();
			}
		}
	}

	/// Commits storage changes to the DB
	#[track_caller]
	pub fn commit_all(mut self) -> Self {
		assert_ok!(self.ext.0.commit_all());
		self
	}

	pub fn snapshot(mut self) -> Snapshot<Ctx> {
		self.ext.0.commit_all().expect("Failed to commit storage changes");
		Snapshot { raw_snapshot: self.ext.0.into_raw_snapshot(), context: self.context.clone() }
	}

	pub fn from_snapshot(snapshot: Snapshot<Ctx>) -> Self {
		let ext = sp_io::TestExternalities::from_raw_snapshot(
			snapshot.raw_snapshot.0,
			snapshot.raw_snapshot.1,
			Default::default(),
		);
		TestExternalities { ext: RichExternalities::new(ext), context: snapshot.context }
	}
}

pub type RawSnapshot = (Vec<(Vec<u8>, (Vec<u8>, i32))>, H256);

#[derive(Clone)]
pub struct Snapshot<Ctx> {
	raw_snapshot: RawSnapshot,
	context: Ctx,
}

impl<Runtime, Ctx> TestExternalities<Runtime, Ctx>
where
	Runtime: HasAllPallets,
	Ctx: Clone,
	<Runtime::RuntimeCall as Dispatchable>::PostInfo: Debug + Default,
{
	/// Applies the provided extrinsics in the next block, asserting the expected result.
	#[track_caller]
	pub fn then_apply_extrinsics<
		C: Into<Runtime::RuntimeCall>,
		I: IntoIterator<Item = (Runtime::RuntimeOrigin, C, DispatchResult)>,
	>(
		self,
		f: impl FnOnce(&Ctx) -> I,
	) -> TestExternalities<Runtime, Ctx> {
		let r = self.ext.execute_at_next_block(
			#[track_caller]
			|| {
				for (origin, call, expected_result) in f(&self.context) {
					match expected_result {
						Ok(_) => {
							assert_ok!(call.into().dispatch(origin));
						},
						Err(e) => {
							assert_noop!(call.into().dispatch(origin), e);
						},
					}
				}
			},
		);
		TestExternalities { ext: r.ext, context: self.context }
	}

	#[track_caller]
	pub fn assert_calls_ok<C: Into<Runtime::RuntimeCall>>(
		self,
		validator_ids: &[Runtime::AccountId],
		call_generator: impl Fn(&Runtime::AccountId) -> C,
	) -> Self {
		self.then_apply_extrinsics(
			#[track_caller]
			|_ctx| {
				validator_ids
					.iter()
					.map(|id| (OriginTrait::signed(id.clone()), call_generator(id), Ok(())))
			},
		)
	}

	#[track_caller]
	pub fn assert_calls_noop<C: Into<Runtime::RuntimeCall>, E: Clone + Into<DispatchError>>(
		self,
		validator_ids: &[Runtime::AccountId],
		call_generator: impl Fn(&Runtime::AccountId) -> C,
		err: E,
	) -> Self {
		self.then_apply_extrinsics(
			#[track_caller]
			|_ctx| {
				validator_ids.iter().map(|id| {
					(OriginTrait::signed(id.clone()), call_generator(id), Err(err.clone().into()))
				})
			},
		)
	}
}

#[cfg(test)]
mod test_examples {
	use super::*;
	use frame_support::{derive_impl, traits::OriginTrait};
	use sp_runtime::DispatchError;

	type Block = frame_system::mocking::MockBlock<Test>;

	// Configure a mock runtime to test the pallet.
	frame_support::construct_runtime!(
		pub enum Test
		{
			System: frame_system,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
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
			.then_execute_with_keep_context(|_| {
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
