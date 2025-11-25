use crate::{
	build_eip712_data::{build_eip712_typed_data, to_ethers_typed_data},
	eip712::Eip712,
	*,
};

use core::marker::PhantomData;

use ethers_core::types::{transaction::eip712::Eip712 as EthersEip712, H256, U128, U256};
use scale_info::prelude::string::String;

mod test_types {

	use super::*;
	#[derive(TypeInfo, Encode)]
	pub enum TestEnum<T, S> {
		A(T),
		B(S),
		C(Vec<u128>),
		D { dd: u16 },
	}
	#[derive(TypeInfo, Encode)]
	pub struct TestEmpty;
	#[derive(TypeInfo, Encode)]
	pub struct TestEmptyNested(pub TestEmpty);

	#[derive(TypeInfo, Encode)]
	pub struct TestCompact<T, S> {
		#[codec(compact)]
		pub c: u128,
		pub a: T,
		pub b: S,
	}

	#[derive(TypeInfo, Encode)]
	pub struct TestEmptyGeneric<T>(pub PhantomData<T>);

	#[derive(TypeInfo, Encode)]
	pub enum TestEmptyEnum {
		Aaaaa,
		Bbbbb,
	}

	#[derive(TypeInfo, Encode)]
	pub struct TestWrappedEmptyEnum(pub TestEmptyEnum);

	#[derive(TypeInfo, Encode)]
	pub struct Abs(pub (u8, u128));
	#[derive(Encode, TypeInfo)]
	pub struct Mail {
		pub from: String,
		pub to: String,
		pub message: String,
	}

	#[derive(Encode, TypeInfo)]
	pub struct TestVec<T>(pub Vec<T>);
	#[derive(Encode, TypeInfo)]
	pub struct TestArray(pub [u16; 10]);

	#[derive(Encode, TypeInfo)]
	pub struct TetsPrimitiveTypeU256(pub U256);
	#[derive(Encode, TypeInfo)]
	pub struct TetsPrimitiveTypeU128(pub U128);
	#[derive(Encode, TypeInfo)]
	pub struct TetsPrimitiveTypeH256(pub H256);
}

fn eip712_hash_matches_ethers<T: TypeInfo + Encode + 'static>(test_value: T) {
	let typed_data =
		build_eip712_typed_data(test_value, "Chainflip-Mainnet".to_string(), 1).unwrap();
	let ethers_typed_data = to_ethers_typed_data(typed_data.clone()).unwrap();
	assert_eq!(
		ethers_typed_data.encode_eip712().unwrap(),
		keccak256(typed_data.encode_eip712().unwrap())
	);
}

#[test]
fn test_eip_encoding_matches_ethers() {
	eip712_hash_matches_ethers(test_types::TestEnum::<u32, u8>::A(5));
	eip712_hash_matches_ethers(test_types::TestEnum::<u32, u8>::B(6));
	eip712_hash_matches_ethers(test_types::TestEnum::<u32, u8>::C(vec![7]));
	eip712_hash_matches_ethers(test_types::TestEnum::<u32, u8>::D { dd: 8 });
	eip712_hash_matches_ethers(test_types::TestWrappedEmptyEnum(test_types::TestEmptyEnum::Aaaaa));
	eip712_hash_matches_ethers(test_types::TestWrappedEmptyEnum(test_types::TestEmptyEnum::Bbbbb));
	eip712_hash_matches_ethers(test_types::TestCompact { a: 5u8, b: 6u32, c: 7 });
	eip712_hash_matches_ethers(test_types::TestEmpty);
	eip712_hash_matches_ethers(test_types::TestEmptyNested(test_types::TestEmpty));
	eip712_hash_matches_ethers(test_types::Abs((8, 9)));
	eip712_hash_matches_ethers(
		test_types::TestEmptyGeneric::<test_types::Mail>(Default::default()),
	);
	eip712_hash_matches_ethers(test_types::TestVec::<u8>(vec![]));
	eip712_hash_matches_ethers(test_types::TestVec::<u128>(vec![]));
	eip712_hash_matches_ethers(test_types::TestVec(vec![5u128, 6u128]));
	eip712_hash_matches_ethers(test_types::TestVec(vec![5u8, 6u8]));

	eip712_hash_matches_ethers(test_types::TestArray([5u16; 10]));
	eip712_hash_matches_ethers(test_types::TetsPrimitiveTypeU256(U256([1, 2, 3, 4])));
	eip712_hash_matches_ethers(test_types::TetsPrimitiveTypeU128(U128([1, 2])));
	eip712_hash_matches_ethers(test_types::TetsPrimitiveTypeH256(H256([5u8; 32])));
	eip712_hash_matches_ethers((12u128, test_types::Abs((8, 10))));
}
