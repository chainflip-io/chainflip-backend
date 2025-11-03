use crate::{eip712::Eip712, *};

use core::marker::PhantomData;

use ethers_core::types::transaction::eip712::{
	Eip712 as EthersEip712, Eip712DomainType as EthersType, TypedData as EthersTypedData,
};
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

	#[derive(TypeInfo, serde::Serialize, Encode)]
	pub struct Test1<T, S> {
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
	#[derive(serde::Serialize, Encode, TypeInfo)]
	pub struct Mail {
		pub from: String,
		pub to: String,
		pub message: String,
	}
}

fn eip712_hash_matches_ethers<T: TypeInfo + Encode + 'static>(test_value: T) {
	let domain = crate::eip712::EIP712Domain {
		name: Some("Chainflip-Mainnet".to_string()),
		version: Some("1".to_string()),
		chain_id: None,
		verifying_contract: None,
		salt: None,
	};

	let domain_ethers = ethers_core::types::transaction::eip712::EIP712Domain {
		name: domain.name.clone(),
		version: domain.version.clone(),
		chain_id: domain.chain_id,
		verifying_contract: domain.verifying_contract,
		salt: domain.salt,
	};

	let typed_data: TypedData = crate::encode_eip712_using_type_info(test_value, domain).unwrap();

	let message_scale_value: scale_value::Value = typed_data.message.clone().into();

	let mut types = typed_data.types.clone();
	types.insert(
		"EIP712Domain".to_string(),
		vec![
			Eip712DomainType { name: "name".to_string(), r#type: "string".to_string() },
			Eip712DomainType { name: "version".to_string(), r#type: "string".to_string() },
		],
	);

	let ethers_types: BTreeMap<String, Vec<EthersType>> = types
		.iter()
		.map(|(s, tys)| {
			(
				s.clone(),
				tys.iter()
					.map(|t| EthersType { name: t.name.clone(), r#type: t.r#type.clone() })
					.collect(),
			)
		})
		.collect();

	let ethers_typed_data = EthersTypedData {
		domain: domain_ethers,
		types: ethers_types,
		primary_type: typed_data.primary_type.clone(),
		message: serde_json::to_value(message_scale_value)
			.unwrap()
			.as_object()
			.ok_or(Eip712Error::Message(
				"the primary type is not a JSON object but one of the primitive types".to_string(),
			))
			.unwrap()
			.clone()
			.into_iter()
			.collect(),
	};

	assert_eq!(
		ethers_typed_data.encode_eip712().unwrap(),
		keccak256(typed_data.encode_eip712().unwrap())
	)
}

#[test]
fn test_eip_encoding_matches_ethers() {
	eip712_hash_matches_ethers(test_types::TestEnum::<u32, u8>::A(5));
	eip712_hash_matches_ethers(test_types::TestEnum::<u32, u8>::B(6));
	eip712_hash_matches_ethers(test_types::TestEnum::<u32, u8>::C(vec![7]));
	eip712_hash_matches_ethers(test_types::TestEnum::<u32, u8>::D { dd: 8 });
	eip712_hash_matches_ethers(test_types::TestWrappedEmptyEnum(test_types::TestEmptyEnum::Aaaaa));
	eip712_hash_matches_ethers(test_types::TestWrappedEmptyEnum(test_types::TestEmptyEnum::Bbbbb));
	eip712_hash_matches_ethers(test_types::Test1 { a: 5u8, b: 6u32, c: 7 });
	eip712_hash_matches_ethers(test_types::TestEmpty);
	eip712_hash_matches_ethers(test_types::TestEmptyNested(test_types::TestEmpty));
	eip712_hash_matches_ethers(test_types::Abs((8, 9)));
	eip712_hash_matches_ethers(
		test_types::TestEmptyGeneric::<test_types::Mail>(Default::default()),
	);
}
