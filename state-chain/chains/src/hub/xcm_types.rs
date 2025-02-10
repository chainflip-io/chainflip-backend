use crate::*;

use codec::{Decode, Encode};
use scale_info::TypeInfo;

#[allow(clippy::large_enum_variant)]
pub mod runtime_types {
	use super::*;
	pub mod asset_hub_polkadot_runtime {

		use super::*;
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub struct Runtime;
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum RuntimeCall {}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum RuntimeError {}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum RuntimeEvent {}
	}
	pub mod assets_common {
		use super::*;
		pub mod runtime_api {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum FungiblesAccessError {
				#[codec(index = 0)]
				AssetIdConversionFailed,
				#[codec(index = 1)]
				AmountToBalanceConversionFailed,
			}
		}
	}
	pub mod bounded_collections {
		use super::*;
		pub mod bounded_vec {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct BoundedVec<_0>(pub sp_std::vec::Vec<_0>);
		}
		pub mod weak_bounded_vec {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct WeakBoundedVec<_0>(pub sp_std::vec::Vec<_0>);
		}
	}
	pub mod frame_metadata_hash_extension {
		use super::*;
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub struct CheckMetadataHash {
			pub mode: runtime_types::frame_metadata_hash_extension::Mode,
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum Mode {
			#[codec(index = 0)]
			Disabled,
			#[codec(index = 1)]
			Enabled,
		}
	}
	pub mod frame_support {
		use super::*;
		pub mod dispatch {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum DispatchClass {
				#[codec(index = 0)]
				Normal,
				#[codec(index = 1)]
				Operational,
				#[codec(index = 2)]
				Mandatory,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Pays {
				#[codec(index = 0)]
				Yes,
				#[codec(index = 1)]
				No,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct PostDispatchInfo {
				pub actual_weight:
					::core::option::Option<runtime_types::sp_weights::weight_v2::Weight>,
				pub pays_fee: runtime_types::frame_support::dispatch::Pays,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum RawOrigin<_0> {
				#[codec(index = 0)]
				Root,
				#[codec(index = 1)]
				Signed(_0),
				#[codec(index = 2)]
				None,
			}
		}
	}
	pub mod frame_system {
		use super::*;
		pub mod extensions {
			use super::*;
			pub mod check_genesis {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct CheckGenesis;
			}
			pub mod check_mortality {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct CheckMortality(pub runtime_types::sp_runtime::generic::era::Era);
			}
			pub mod check_non_zero_sender {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct CheckNonZeroSender;
			}
			pub mod check_nonce {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct CheckNonce(#[codec(compact)] pub ::core::primitive::u32);
			}
			pub mod check_spec_version {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct CheckSpecVersion;
			}
			pub mod check_tx_version {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct CheckTxVersion;
			}
			pub mod check_weight {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct CheckWeight;
			}
		}
	}
	pub mod pallet_asset_conversion_tx_payment {
		use super::*;
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub struct ChargeAssetTxPayment {
			#[codec(compact)]
			pub tip: ::core::primitive::u128,
			pub asset_id:
				::core::option::Option<runtime_types::staging_xcm::v4::location::Location>,
		}
	}
	pub mod pallet_transaction_payment {
		use super::*;
		pub mod types {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct FeeDetails<_0> {
				pub inclusion_fee: ::core::option::Option<
					runtime_types::pallet_transaction_payment::types::InclusionFee<_0>,
				>,
				pub tip: _0,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct InclusionFee<_0> {
				pub base_fee: _0,
				pub len_fee: _0,
				pub adjusted_weight_fee: _0,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct RuntimeDispatchInfo<_0, _1> {
				pub weight: _1,
				pub class: runtime_types::frame_support::dispatch::DispatchClass,
				pub partial_fee: _0,
			}
		}
	}
	pub mod pallet_xcm {
		use super::*;
		pub mod pallet {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Origin {
				#[codec(index = 0)]
				Xcm(runtime_types::staging_xcm::v4::location::Location),
				#[codec(index = 1)]
				Response(runtime_types::staging_xcm::v4::location::Location),
			}
		}
	}
	pub mod polkadot_core_primitives {
		use super::*;
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub struct OutboundHrmpMessage<_0> {
			pub recipient: _0,
			pub data: sp_std::vec::Vec<::core::primitive::u8>,
		}
	}
	pub mod sp_arithmetic {
		use super::*;
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum ArithmeticError {
			#[codec(index = 0)]
			Underflow,
			#[codec(index = 1)]
			Overflow,
			#[codec(index = 2)]
			DivisionByZero,
		}
	}
	pub mod sp_consensus_aura {
		use super::*;
		pub mod ed25519 {
			use super::*;
			pub mod app_ed25519 {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct Public(pub [::core::primitive::u8; 32usize]);
			}
		}
	}
	pub mod sp_core {
		use super::*;
		pub mod crypto {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct KeyTypeId(pub [::core::primitive::u8; 4usize]);
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub struct OpaqueMetadata(pub sp_std::vec::Vec<::core::primitive::u8>);
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum Void {}
	}
	pub mod sp_runtime {
		use super::*;
		pub mod generic {
			use super::*;
			pub mod block {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct Block<_0, _1> {
					pub header: _0,
					pub extrinsics: sp_std::vec::Vec<_1>,
				}
			}
			pub mod digest {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct Digest {
					pub logs:
						sp_std::vec::Vec<runtime_types::sp_runtime::generic::digest::DigestItem>,
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum DigestItem {
					#[codec(index = 6)]
					PreRuntime(
						[::core::primitive::u8; 4usize],
						sp_std::vec::Vec<::core::primitive::u8>,
					),
					#[codec(index = 4)]
					Consensus(
						[::core::primitive::u8; 4usize],
						sp_std::vec::Vec<::core::primitive::u8>,
					),
					#[codec(index = 5)]
					Seal([::core::primitive::u8; 4usize], sp_std::vec::Vec<::core::primitive::u8>),
					#[codec(index = 0)]
					Other(sp_std::vec::Vec<::core::primitive::u8>),
					#[codec(index = 8)]
					RuntimeEnvironmentUpdated,
				}
			}
			pub mod era {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Era {
					#[codec(index = 0)]
					Immortal,
					#[codec(index = 1)]
					Mortal1(::core::primitive::u8),
					#[codec(index = 2)]
					Mortal2(::core::primitive::u8),
					#[codec(index = 3)]
					Mortal3(::core::primitive::u8),
					#[codec(index = 4)]
					Mortal4(::core::primitive::u8),
					#[codec(index = 5)]
					Mortal5(::core::primitive::u8),
					#[codec(index = 6)]
					Mortal6(::core::primitive::u8),
					#[codec(index = 7)]
					Mortal7(::core::primitive::u8),
					#[codec(index = 8)]
					Mortal8(::core::primitive::u8),
					#[codec(index = 9)]
					Mortal9(::core::primitive::u8),
					#[codec(index = 10)]
					Mortal10(::core::primitive::u8),
					#[codec(index = 11)]
					Mortal11(::core::primitive::u8),
					#[codec(index = 12)]
					Mortal12(::core::primitive::u8),
					#[codec(index = 13)]
					Mortal13(::core::primitive::u8),
					#[codec(index = 14)]
					Mortal14(::core::primitive::u8),
					#[codec(index = 15)]
					Mortal15(::core::primitive::u8),
					#[codec(index = 16)]
					Mortal16(::core::primitive::u8),
					#[codec(index = 17)]
					Mortal17(::core::primitive::u8),
					#[codec(index = 18)]
					Mortal18(::core::primitive::u8),
					#[codec(index = 19)]
					Mortal19(::core::primitive::u8),
					#[codec(index = 20)]
					Mortal20(::core::primitive::u8),
					#[codec(index = 21)]
					Mortal21(::core::primitive::u8),
					#[codec(index = 22)]
					Mortal22(::core::primitive::u8),
					#[codec(index = 23)]
					Mortal23(::core::primitive::u8),
					#[codec(index = 24)]
					Mortal24(::core::primitive::u8),
					#[codec(index = 25)]
					Mortal25(::core::primitive::u8),
					#[codec(index = 26)]
					Mortal26(::core::primitive::u8),
					#[codec(index = 27)]
					Mortal27(::core::primitive::u8),
					#[codec(index = 28)]
					Mortal28(::core::primitive::u8),
					#[codec(index = 29)]
					Mortal29(::core::primitive::u8),
					#[codec(index = 30)]
					Mortal30(::core::primitive::u8),
					#[codec(index = 31)]
					Mortal31(::core::primitive::u8),
					#[codec(index = 32)]
					Mortal32(::core::primitive::u8),
					#[codec(index = 33)]
					Mortal33(::core::primitive::u8),
					#[codec(index = 34)]
					Mortal34(::core::primitive::u8),
					#[codec(index = 35)]
					Mortal35(::core::primitive::u8),
					#[codec(index = 36)]
					Mortal36(::core::primitive::u8),
					#[codec(index = 37)]
					Mortal37(::core::primitive::u8),
					#[codec(index = 38)]
					Mortal38(::core::primitive::u8),
					#[codec(index = 39)]
					Mortal39(::core::primitive::u8),
					#[codec(index = 40)]
					Mortal40(::core::primitive::u8),
					#[codec(index = 41)]
					Mortal41(::core::primitive::u8),
					#[codec(index = 42)]
					Mortal42(::core::primitive::u8),
					#[codec(index = 43)]
					Mortal43(::core::primitive::u8),
					#[codec(index = 44)]
					Mortal44(::core::primitive::u8),
					#[codec(index = 45)]
					Mortal45(::core::primitive::u8),
					#[codec(index = 46)]
					Mortal46(::core::primitive::u8),
					#[codec(index = 47)]
					Mortal47(::core::primitive::u8),
					#[codec(index = 48)]
					Mortal48(::core::primitive::u8),
					#[codec(index = 49)]
					Mortal49(::core::primitive::u8),
					#[codec(index = 50)]
					Mortal50(::core::primitive::u8),
					#[codec(index = 51)]
					Mortal51(::core::primitive::u8),
					#[codec(index = 52)]
					Mortal52(::core::primitive::u8),
					#[codec(index = 53)]
					Mortal53(::core::primitive::u8),
					#[codec(index = 54)]
					Mortal54(::core::primitive::u8),
					#[codec(index = 55)]
					Mortal55(::core::primitive::u8),
					#[codec(index = 56)]
					Mortal56(::core::primitive::u8),
					#[codec(index = 57)]
					Mortal57(::core::primitive::u8),
					#[codec(index = 58)]
					Mortal58(::core::primitive::u8),
					#[codec(index = 59)]
					Mortal59(::core::primitive::u8),
					#[codec(index = 60)]
					Mortal60(::core::primitive::u8),
					#[codec(index = 61)]
					Mortal61(::core::primitive::u8),
					#[codec(index = 62)]
					Mortal62(::core::primitive::u8),
					#[codec(index = 63)]
					Mortal63(::core::primitive::u8),
					#[codec(index = 64)]
					Mortal64(::core::primitive::u8),
					#[codec(index = 65)]
					Mortal65(::core::primitive::u8),
					#[codec(index = 66)]
					Mortal66(::core::primitive::u8),
					#[codec(index = 67)]
					Mortal67(::core::primitive::u8),
					#[codec(index = 68)]
					Mortal68(::core::primitive::u8),
					#[codec(index = 69)]
					Mortal69(::core::primitive::u8),
					#[codec(index = 70)]
					Mortal70(::core::primitive::u8),
					#[codec(index = 71)]
					Mortal71(::core::primitive::u8),
					#[codec(index = 72)]
					Mortal72(::core::primitive::u8),
					#[codec(index = 73)]
					Mortal73(::core::primitive::u8),
					#[codec(index = 74)]
					Mortal74(::core::primitive::u8),
					#[codec(index = 75)]
					Mortal75(::core::primitive::u8),
					#[codec(index = 76)]
					Mortal76(::core::primitive::u8),
					#[codec(index = 77)]
					Mortal77(::core::primitive::u8),
					#[codec(index = 78)]
					Mortal78(::core::primitive::u8),
					#[codec(index = 79)]
					Mortal79(::core::primitive::u8),
					#[codec(index = 80)]
					Mortal80(::core::primitive::u8),
					#[codec(index = 81)]
					Mortal81(::core::primitive::u8),
					#[codec(index = 82)]
					Mortal82(::core::primitive::u8),
					#[codec(index = 83)]
					Mortal83(::core::primitive::u8),
					#[codec(index = 84)]
					Mortal84(::core::primitive::u8),
					#[codec(index = 85)]
					Mortal85(::core::primitive::u8),
					#[codec(index = 86)]
					Mortal86(::core::primitive::u8),
					#[codec(index = 87)]
					Mortal87(::core::primitive::u8),
					#[codec(index = 88)]
					Mortal88(::core::primitive::u8),
					#[codec(index = 89)]
					Mortal89(::core::primitive::u8),
					#[codec(index = 90)]
					Mortal90(::core::primitive::u8),
					#[codec(index = 91)]
					Mortal91(::core::primitive::u8),
					#[codec(index = 92)]
					Mortal92(::core::primitive::u8),
					#[codec(index = 93)]
					Mortal93(::core::primitive::u8),
					#[codec(index = 94)]
					Mortal94(::core::primitive::u8),
					#[codec(index = 95)]
					Mortal95(::core::primitive::u8),
					#[codec(index = 96)]
					Mortal96(::core::primitive::u8),
					#[codec(index = 97)]
					Mortal97(::core::primitive::u8),
					#[codec(index = 98)]
					Mortal98(::core::primitive::u8),
					#[codec(index = 99)]
					Mortal99(::core::primitive::u8),
					#[codec(index = 100)]
					Mortal100(::core::primitive::u8),
					#[codec(index = 101)]
					Mortal101(::core::primitive::u8),
					#[codec(index = 102)]
					Mortal102(::core::primitive::u8),
					#[codec(index = 103)]
					Mortal103(::core::primitive::u8),
					#[codec(index = 104)]
					Mortal104(::core::primitive::u8),
					#[codec(index = 105)]
					Mortal105(::core::primitive::u8),
					#[codec(index = 106)]
					Mortal106(::core::primitive::u8),
					#[codec(index = 107)]
					Mortal107(::core::primitive::u8),
					#[codec(index = 108)]
					Mortal108(::core::primitive::u8),
					#[codec(index = 109)]
					Mortal109(::core::primitive::u8),
					#[codec(index = 110)]
					Mortal110(::core::primitive::u8),
					#[codec(index = 111)]
					Mortal111(::core::primitive::u8),
					#[codec(index = 112)]
					Mortal112(::core::primitive::u8),
					#[codec(index = 113)]
					Mortal113(::core::primitive::u8),
					#[codec(index = 114)]
					Mortal114(::core::primitive::u8),
					#[codec(index = 115)]
					Mortal115(::core::primitive::u8),
					#[codec(index = 116)]
					Mortal116(::core::primitive::u8),
					#[codec(index = 117)]
					Mortal117(::core::primitive::u8),
					#[codec(index = 118)]
					Mortal118(::core::primitive::u8),
					#[codec(index = 119)]
					Mortal119(::core::primitive::u8),
					#[codec(index = 120)]
					Mortal120(::core::primitive::u8),
					#[codec(index = 121)]
					Mortal121(::core::primitive::u8),
					#[codec(index = 122)]
					Mortal122(::core::primitive::u8),
					#[codec(index = 123)]
					Mortal123(::core::primitive::u8),
					#[codec(index = 124)]
					Mortal124(::core::primitive::u8),
					#[codec(index = 125)]
					Mortal125(::core::primitive::u8),
					#[codec(index = 126)]
					Mortal126(::core::primitive::u8),
					#[codec(index = 127)]
					Mortal127(::core::primitive::u8),
					#[codec(index = 128)]
					Mortal128(::core::primitive::u8),
					#[codec(index = 129)]
					Mortal129(::core::primitive::u8),
					#[codec(index = 130)]
					Mortal130(::core::primitive::u8),
					#[codec(index = 131)]
					Mortal131(::core::primitive::u8),
					#[codec(index = 132)]
					Mortal132(::core::primitive::u8),
					#[codec(index = 133)]
					Mortal133(::core::primitive::u8),
					#[codec(index = 134)]
					Mortal134(::core::primitive::u8),
					#[codec(index = 135)]
					Mortal135(::core::primitive::u8),
					#[codec(index = 136)]
					Mortal136(::core::primitive::u8),
					#[codec(index = 137)]
					Mortal137(::core::primitive::u8),
					#[codec(index = 138)]
					Mortal138(::core::primitive::u8),
					#[codec(index = 139)]
					Mortal139(::core::primitive::u8),
					#[codec(index = 140)]
					Mortal140(::core::primitive::u8),
					#[codec(index = 141)]
					Mortal141(::core::primitive::u8),
					#[codec(index = 142)]
					Mortal142(::core::primitive::u8),
					#[codec(index = 143)]
					Mortal143(::core::primitive::u8),
					#[codec(index = 144)]
					Mortal144(::core::primitive::u8),
					#[codec(index = 145)]
					Mortal145(::core::primitive::u8),
					#[codec(index = 146)]
					Mortal146(::core::primitive::u8),
					#[codec(index = 147)]
					Mortal147(::core::primitive::u8),
					#[codec(index = 148)]
					Mortal148(::core::primitive::u8),
					#[codec(index = 149)]
					Mortal149(::core::primitive::u8),
					#[codec(index = 150)]
					Mortal150(::core::primitive::u8),
					#[codec(index = 151)]
					Mortal151(::core::primitive::u8),
					#[codec(index = 152)]
					Mortal152(::core::primitive::u8),
					#[codec(index = 153)]
					Mortal153(::core::primitive::u8),
					#[codec(index = 154)]
					Mortal154(::core::primitive::u8),
					#[codec(index = 155)]
					Mortal155(::core::primitive::u8),
					#[codec(index = 156)]
					Mortal156(::core::primitive::u8),
					#[codec(index = 157)]
					Mortal157(::core::primitive::u8),
					#[codec(index = 158)]
					Mortal158(::core::primitive::u8),
					#[codec(index = 159)]
					Mortal159(::core::primitive::u8),
					#[codec(index = 160)]
					Mortal160(::core::primitive::u8),
					#[codec(index = 161)]
					Mortal161(::core::primitive::u8),
					#[codec(index = 162)]
					Mortal162(::core::primitive::u8),
					#[codec(index = 163)]
					Mortal163(::core::primitive::u8),
					#[codec(index = 164)]
					Mortal164(::core::primitive::u8),
					#[codec(index = 165)]
					Mortal165(::core::primitive::u8),
					#[codec(index = 166)]
					Mortal166(::core::primitive::u8),
					#[codec(index = 167)]
					Mortal167(::core::primitive::u8),
					#[codec(index = 168)]
					Mortal168(::core::primitive::u8),
					#[codec(index = 169)]
					Mortal169(::core::primitive::u8),
					#[codec(index = 170)]
					Mortal170(::core::primitive::u8),
					#[codec(index = 171)]
					Mortal171(::core::primitive::u8),
					#[codec(index = 172)]
					Mortal172(::core::primitive::u8),
					#[codec(index = 173)]
					Mortal173(::core::primitive::u8),
					#[codec(index = 174)]
					Mortal174(::core::primitive::u8),
					#[codec(index = 175)]
					Mortal175(::core::primitive::u8),
					#[codec(index = 176)]
					Mortal176(::core::primitive::u8),
					#[codec(index = 177)]
					Mortal177(::core::primitive::u8),
					#[codec(index = 178)]
					Mortal178(::core::primitive::u8),
					#[codec(index = 179)]
					Mortal179(::core::primitive::u8),
					#[codec(index = 180)]
					Mortal180(::core::primitive::u8),
					#[codec(index = 181)]
					Mortal181(::core::primitive::u8),
					#[codec(index = 182)]
					Mortal182(::core::primitive::u8),
					#[codec(index = 183)]
					Mortal183(::core::primitive::u8),
					#[codec(index = 184)]
					Mortal184(::core::primitive::u8),
					#[codec(index = 185)]
					Mortal185(::core::primitive::u8),
					#[codec(index = 186)]
					Mortal186(::core::primitive::u8),
					#[codec(index = 187)]
					Mortal187(::core::primitive::u8),
					#[codec(index = 188)]
					Mortal188(::core::primitive::u8),
					#[codec(index = 189)]
					Mortal189(::core::primitive::u8),
					#[codec(index = 190)]
					Mortal190(::core::primitive::u8),
					#[codec(index = 191)]
					Mortal191(::core::primitive::u8),
					#[codec(index = 192)]
					Mortal192(::core::primitive::u8),
					#[codec(index = 193)]
					Mortal193(::core::primitive::u8),
					#[codec(index = 194)]
					Mortal194(::core::primitive::u8),
					#[codec(index = 195)]
					Mortal195(::core::primitive::u8),
					#[codec(index = 196)]
					Mortal196(::core::primitive::u8),
					#[codec(index = 197)]
					Mortal197(::core::primitive::u8),
					#[codec(index = 198)]
					Mortal198(::core::primitive::u8),
					#[codec(index = 199)]
					Mortal199(::core::primitive::u8),
					#[codec(index = 200)]
					Mortal200(::core::primitive::u8),
					#[codec(index = 201)]
					Mortal201(::core::primitive::u8),
					#[codec(index = 202)]
					Mortal202(::core::primitive::u8),
					#[codec(index = 203)]
					Mortal203(::core::primitive::u8),
					#[codec(index = 204)]
					Mortal204(::core::primitive::u8),
					#[codec(index = 205)]
					Mortal205(::core::primitive::u8),
					#[codec(index = 206)]
					Mortal206(::core::primitive::u8),
					#[codec(index = 207)]
					Mortal207(::core::primitive::u8),
					#[codec(index = 208)]
					Mortal208(::core::primitive::u8),
					#[codec(index = 209)]
					Mortal209(::core::primitive::u8),
					#[codec(index = 210)]
					Mortal210(::core::primitive::u8),
					#[codec(index = 211)]
					Mortal211(::core::primitive::u8),
					#[codec(index = 212)]
					Mortal212(::core::primitive::u8),
					#[codec(index = 213)]
					Mortal213(::core::primitive::u8),
					#[codec(index = 214)]
					Mortal214(::core::primitive::u8),
					#[codec(index = 215)]
					Mortal215(::core::primitive::u8),
					#[codec(index = 216)]
					Mortal216(::core::primitive::u8),
					#[codec(index = 217)]
					Mortal217(::core::primitive::u8),
					#[codec(index = 218)]
					Mortal218(::core::primitive::u8),
					#[codec(index = 219)]
					Mortal219(::core::primitive::u8),
					#[codec(index = 220)]
					Mortal220(::core::primitive::u8),
					#[codec(index = 221)]
					Mortal221(::core::primitive::u8),
					#[codec(index = 222)]
					Mortal222(::core::primitive::u8),
					#[codec(index = 223)]
					Mortal223(::core::primitive::u8),
					#[codec(index = 224)]
					Mortal224(::core::primitive::u8),
					#[codec(index = 225)]
					Mortal225(::core::primitive::u8),
					#[codec(index = 226)]
					Mortal226(::core::primitive::u8),
					#[codec(index = 227)]
					Mortal227(::core::primitive::u8),
					#[codec(index = 228)]
					Mortal228(::core::primitive::u8),
					#[codec(index = 229)]
					Mortal229(::core::primitive::u8),
					#[codec(index = 230)]
					Mortal230(::core::primitive::u8),
					#[codec(index = 231)]
					Mortal231(::core::primitive::u8),
					#[codec(index = 232)]
					Mortal232(::core::primitive::u8),
					#[codec(index = 233)]
					Mortal233(::core::primitive::u8),
					#[codec(index = 234)]
					Mortal234(::core::primitive::u8),
					#[codec(index = 235)]
					Mortal235(::core::primitive::u8),
					#[codec(index = 236)]
					Mortal236(::core::primitive::u8),
					#[codec(index = 237)]
					Mortal237(::core::primitive::u8),
					#[codec(index = 238)]
					Mortal238(::core::primitive::u8),
					#[codec(index = 239)]
					Mortal239(::core::primitive::u8),
					#[codec(index = 240)]
					Mortal240(::core::primitive::u8),
					#[codec(index = 241)]
					Mortal241(::core::primitive::u8),
					#[codec(index = 242)]
					Mortal242(::core::primitive::u8),
					#[codec(index = 243)]
					Mortal243(::core::primitive::u8),
					#[codec(index = 244)]
					Mortal244(::core::primitive::u8),
					#[codec(index = 245)]
					Mortal245(::core::primitive::u8),
					#[codec(index = 246)]
					Mortal246(::core::primitive::u8),
					#[codec(index = 247)]
					Mortal247(::core::primitive::u8),
					#[codec(index = 248)]
					Mortal248(::core::primitive::u8),
					#[codec(index = 249)]
					Mortal249(::core::primitive::u8),
					#[codec(index = 250)]
					Mortal250(::core::primitive::u8),
					#[codec(index = 251)]
					Mortal251(::core::primitive::u8),
					#[codec(index = 252)]
					Mortal252(::core::primitive::u8),
					#[codec(index = 253)]
					Mortal253(::core::primitive::u8),
					#[codec(index = 254)]
					Mortal254(::core::primitive::u8),
					#[codec(index = 255)]
					Mortal255(::core::primitive::u8),
				}
			}
		}
		pub mod transaction_validity {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum InvalidTransaction {
				#[codec(index = 0)]
				Call,
				#[codec(index = 1)]
				Payment,
				#[codec(index = 2)]
				Future,
				#[codec(index = 3)]
				Stale,
				#[codec(index = 4)]
				BadProof,
				#[codec(index = 5)]
				AncientBirthBlock,
				#[codec(index = 6)]
				ExhaustsResources,
				#[codec(index = 7)]
				Custom(::core::primitive::u8),
				#[codec(index = 8)]
				BadMandatory,
				#[codec(index = 9)]
				MandatoryValidation,
				#[codec(index = 10)]
				BadSigner,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum TransactionSource {
				#[codec(index = 0)]
				InBlock,
				#[codec(index = 1)]
				Local,
				#[codec(index = 2)]
				External,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum TransactionValidityError {
				#[codec(index = 0)]
				Invalid(runtime_types::sp_runtime::transaction_validity::InvalidTransaction),
				#[codec(index = 1)]
				Unknown(runtime_types::sp_runtime::transaction_validity::UnknownTransaction),
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum UnknownTransaction {
				#[codec(index = 0)]
				CannotLookup,
				#[codec(index = 1)]
				NoUnsignedValidator,
				#[codec(index = 2)]
				Custom(::core::primitive::u8),
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct ValidTransaction {
				pub priority: ::core::primitive::u64,
				pub requires: sp_std::vec::Vec<sp_std::vec::Vec<::core::primitive::u8>>,
				pub provides: sp_std::vec::Vec<sp_std::vec::Vec<::core::primitive::u8>>,
				pub longevity: ::core::primitive::u64,
				pub propagate: ::core::primitive::bool,
			}
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum DispatchError {
			#[codec(index = 0)]
			Other,
			#[codec(index = 1)]
			CannotLookup,
			#[codec(index = 2)]
			BadOrigin,
			#[codec(index = 3)]
			Module(runtime_types::sp_runtime::ModuleError),
			#[codec(index = 4)]
			ConsumerRemaining,
			#[codec(index = 5)]
			NoProviders,
			#[codec(index = 6)]
			TooManyConsumers,
			#[codec(index = 7)]
			Token(runtime_types::sp_runtime::TokenError),
			#[codec(index = 8)]
			Arithmetic(runtime_types::sp_arithmetic::ArithmeticError),
			#[codec(index = 9)]
			Transactional(runtime_types::sp_runtime::TransactionalError),
			#[codec(index = 10)]
			Exhausted,
			#[codec(index = 11)]
			Corruption,
			#[codec(index = 12)]
			Unavailable,
			#[codec(index = 13)]
			RootNotAllowed,
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub struct DispatchErrorWithPostInfo<_0> {
			pub post_info: _0,
			pub error: runtime_types::sp_runtime::DispatchError,
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum ExtrinsicInclusionMode {
			#[codec(index = 0)]
			AllExtrinsics,
			#[codec(index = 1)]
			OnlyInherents,
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub struct ModuleError {
			pub index: ::core::primitive::u8,
			pub error: [::core::primitive::u8; 4usize],
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum MultiSignature {
			#[codec(index = 0)]
			Ed25519([::core::primitive::u8; 64usize]),
			#[codec(index = 1)]
			Sr25519([::core::primitive::u8; 64usize]),
			#[codec(index = 2)]
			Ecdsa([::core::primitive::u8; 65usize]),
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum TokenError {
			#[codec(index = 0)]
			FundsUnavailable,
			#[codec(index = 1)]
			OnlyProvider,
			#[codec(index = 2)]
			BelowMinimum,
			#[codec(index = 3)]
			CannotCreate,
			#[codec(index = 4)]
			UnknownAsset,
			#[codec(index = 5)]
			Frozen,
			#[codec(index = 6)]
			Unsupported,
			#[codec(index = 7)]
			CannotCreateHold,
			#[codec(index = 8)]
			NotExpendable,
			#[codec(index = 9)]
			Blocked,
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum TransactionalError {
			#[codec(index = 0)]
			LimitReached,
			#[codec(index = 1)]
			NoLayer,
		}
	}
	pub mod sp_weights {
		use super::*;
		pub mod weight_v2 {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct Weight {
				#[codec(compact)]
				pub ref_time: ::core::primitive::u64,
				#[codec(compact)]
				pub proof_size: ::core::primitive::u64,
			}
		}
	}
	pub mod staging_xcm {
		use super::*;
		pub mod v3 {
			use super::*;
			pub mod multilocation {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct MultiLocation {
					pub parents: ::core::primitive::u8,
					pub interior: runtime_types::xcm::v3::junctions::Junctions,
				}
			}
		}
		pub mod v4 {
			use super::*;
			pub mod asset {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct Asset {
					pub id: runtime_types::staging_xcm::v4::asset::AssetId,
					pub fun: runtime_types::staging_xcm::v4::asset::Fungibility,
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum AssetFilter {
					#[codec(index = 0)]
					Definite(runtime_types::staging_xcm::v4::asset::Assets),
					#[codec(index = 1)]
					Wild(runtime_types::staging_xcm::v4::asset::WildAsset),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct AssetId(pub runtime_types::staging_xcm::v4::location::Location);
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum AssetInstance {
					#[codec(index = 0)]
					Undefined,
					#[codec(index = 1)]
					Index(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 2)]
					Array4([::core::primitive::u8; 4usize]),
					#[codec(index = 3)]
					Array8([::core::primitive::u8; 8usize]),
					#[codec(index = 4)]
					Array16([::core::primitive::u8; 16usize]),
					#[codec(index = 5)]
					Array32([::core::primitive::u8; 32usize]),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct Assets(
					pub sp_std::vec::Vec<runtime_types::staging_xcm::v4::asset::Asset>,
				);
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Fungibility {
					#[codec(index = 0)]
					Fungible(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 1)]
					NonFungible(runtime_types::staging_xcm::v4::asset::AssetInstance),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum WildAsset {
					#[codec(index = 0)]
					All,
					#[codec(index = 1)]
					AllOf {
						id: runtime_types::staging_xcm::v4::asset::AssetId,
						fun: runtime_types::staging_xcm::v4::asset::WildFungibility,
					},
					#[codec(index = 2)]
					AllCounted(#[codec(compact)] ::core::primitive::u32),
					#[codec(index = 3)]
					AllOfCounted {
						id: runtime_types::staging_xcm::v4::asset::AssetId,
						fun: runtime_types::staging_xcm::v4::asset::WildFungibility,
						#[codec(compact)]
						count: ::core::primitive::u32,
					},
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum WildFungibility {
					#[codec(index = 0)]
					Fungible,
					#[codec(index = 1)]
					NonFungible,
				}
			}
			pub mod junction {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Junction {
					#[codec(index = 0)]
					Parachain(#[codec(compact)] ::core::primitive::u32),
					#[codec(index = 1)]
					AccountId32 {
						network: ::core::option::Option<
							runtime_types::staging_xcm::v4::junction::NetworkId,
						>,
						id: [::core::primitive::u8; 32usize],
					},
					#[codec(index = 2)]
					AccountIndex64 {
						network: ::core::option::Option<
							runtime_types::staging_xcm::v4::junction::NetworkId,
						>,
						#[codec(compact)]
						index: ::core::primitive::u64,
					},
					#[codec(index = 3)]
					AccountKey20 {
						network: ::core::option::Option<
							runtime_types::staging_xcm::v4::junction::NetworkId,
						>,
						key: [::core::primitive::u8; 20usize],
					},
					#[codec(index = 4)]
					PalletInstance(::core::primitive::u8),
					#[codec(index = 5)]
					GeneralIndex(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 6)]
					GeneralKey {
						length: ::core::primitive::u8,
						data: [::core::primitive::u8; 32usize],
					},
					#[codec(index = 7)]
					OnlyChild,
					#[codec(index = 8)]
					Plurality {
						id: runtime_types::xcm::v3::junction::BodyId,
						part: runtime_types::xcm::v3::junction::BodyPart,
					},
					#[codec(index = 9)]
					GlobalConsensus(runtime_types::staging_xcm::v4::junction::NetworkId),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum NetworkId {
					#[codec(index = 0)]
					ByGenesis([::core::primitive::u8; 32usize]),
					#[codec(index = 1)]
					ByFork {
						block_number: ::core::primitive::u64,
						block_hash: [::core::primitive::u8; 32usize],
					},
					#[codec(index = 2)]
					Polkadot,
					#[codec(index = 3)]
					Kusama,
					#[codec(index = 4)]
					Westend,
					#[codec(index = 5)]
					Rococo,
					#[codec(index = 6)]
					Wococo,
					#[codec(index = 7)]
					Ethereum {
						#[codec(compact)]
						chain_id: ::core::primitive::u64,
					},
					#[codec(index = 8)]
					BitcoinCore,
					#[codec(index = 9)]
					BitcoinCash,
					#[codec(index = 10)]
					PolkadotBulletin,
				}
			}
			pub mod junctions {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Junctions {
					#[codec(index = 0)]
					Here,
					#[codec(index = 1)]
					X1([runtime_types::staging_xcm::v4::junction::Junction; 1usize]),
					#[codec(index = 2)]
					X2([runtime_types::staging_xcm::v4::junction::Junction; 2usize]),
					#[codec(index = 3)]
					X3([runtime_types::staging_xcm::v4::junction::Junction; 3usize]),
					#[codec(index = 4)]
					X4([runtime_types::staging_xcm::v4::junction::Junction; 4usize]),
					#[codec(index = 5)]
					X5([runtime_types::staging_xcm::v4::junction::Junction; 5usize]),
					#[codec(index = 6)]
					X6([runtime_types::staging_xcm::v4::junction::Junction; 6usize]),
					#[codec(index = 7)]
					X7([runtime_types::staging_xcm::v4::junction::Junction; 7usize]),
					#[codec(index = 8)]
					X8([runtime_types::staging_xcm::v4::junction::Junction; 8usize]),
				}
			}
			pub mod location {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct Location {
					pub parents: ::core::primitive::u8,
					pub interior: runtime_types::staging_xcm::v4::junctions::Junctions,
				}
			}
			pub mod traits {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Outcome {
					#[codec(index = 0)]
					Complete { used: runtime_types::sp_weights::weight_v2::Weight },
					#[codec(index = 1)]
					Incomplete {
						used: runtime_types::sp_weights::weight_v2::Weight,
						error: runtime_types::xcm::v3::traits::Error,
					},
					#[codec(index = 2)]
					Error { error: runtime_types::xcm::v3::traits::Error },
				}
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Instruction {
				#[codec(index = 0)]
				WithdrawAsset(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 1)]
				ReserveAssetDeposited(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 2)]
				ReceiveTeleportedAsset(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 3)]
				QueryResponse {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					response: runtime_types::staging_xcm::v4::Response,
					max_weight: runtime_types::sp_weights::weight_v2::Weight,
					querier:
						::core::option::Option<runtime_types::staging_xcm::v4::location::Location>,
				},
				#[codec(index = 4)]
				TransferAsset {
					assets: runtime_types::staging_xcm::v4::asset::Assets,
					beneficiary: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 5)]
				TransferReserveAsset {
					assets: runtime_types::staging_xcm::v4::asset::Assets,
					dest: runtime_types::staging_xcm::v4::location::Location,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 6)]
				Transact {
					origin_kind: runtime_types::xcm::v3::OriginKind,
					require_weight_at_most: runtime_types::sp_weights::weight_v2::Weight,
					call: runtime_types::xcm::double_encoded::DoubleEncoded,
				},
				#[codec(index = 7)]
				HrmpNewChannelOpenRequest {
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					max_message_size: ::core::primitive::u32,
					#[codec(compact)]
					max_capacity: ::core::primitive::u32,
				},
				#[codec(index = 8)]
				HrmpChannelAccepted {
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 9)]
				HrmpChannelClosing {
					#[codec(compact)]
					initiator: ::core::primitive::u32,
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 10)]
				ClearOrigin,
				#[codec(index = 11)]
				DescendOrigin(runtime_types::staging_xcm::v4::junctions::Junctions),
				#[codec(index = 12)]
				ReportError(runtime_types::staging_xcm::v4::QueryResponseInfo),
				#[codec(index = 13)]
				DepositAsset {
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
					beneficiary: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 14)]
				DepositReserveAsset {
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
					dest: runtime_types::staging_xcm::v4::location::Location,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 15)]
				ExchangeAsset {
					give: runtime_types::staging_xcm::v4::asset::AssetFilter,
					want: runtime_types::staging_xcm::v4::asset::Assets,
					maximal: ::core::primitive::bool,
				},
				#[codec(index = 16)]
				InitiateReserveWithdraw {
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
					reserve: runtime_types::staging_xcm::v4::location::Location,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 17)]
				InitiateTeleport {
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
					dest: runtime_types::staging_xcm::v4::location::Location,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 18)]
				ReportHolding {
					response_info: runtime_types::staging_xcm::v4::QueryResponseInfo,
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
				},
				#[codec(index = 19)]
				BuyExecution {
					fees: runtime_types::staging_xcm::v4::asset::Asset,
					weight_limit: runtime_types::xcm::v3::WeightLimit,
				},
				#[codec(index = 20)]
				RefundSurplus,
				#[codec(index = 21)]
				SetErrorHandler(::core::primitive::bool),
				#[codec(index = 22)]
				SetAppendix(::core::primitive::bool),
				#[codec(index = 23)]
				ClearError,
				#[codec(index = 24)]
				ClaimAsset {
					assets: runtime_types::staging_xcm::v4::asset::Assets,
					ticket: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 25)]
				Trap(#[codec(compact)] ::core::primitive::u64),
				#[codec(index = 26)]
				SubscribeVersion {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					max_response_weight: runtime_types::sp_weights::weight_v2::Weight,
				},
				#[codec(index = 27)]
				UnsubscribeVersion,
				#[codec(index = 28)]
				BurnAsset(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 29)]
				ExpectAsset(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 30)]
				ExpectOrigin(
					::core::option::Option<runtime_types::staging_xcm::v4::location::Location>,
				),
				#[codec(index = 31)]
				ExpectError(
					::core::option::Option<(
						::core::primitive::u32,
						runtime_types::xcm::v3::traits::Error,
					)>,
				),
				#[codec(index = 32)]
				ExpectTransactStatus(runtime_types::xcm::v3::MaybeErrorCode),
				#[codec(index = 33)]
				QueryPallet {
					module_name: sp_std::vec::Vec<::core::primitive::u8>,
					response_info: runtime_types::staging_xcm::v4::QueryResponseInfo,
				},
				#[codec(index = 34)]
				ExpectPallet {
					#[codec(compact)]
					index: ::core::primitive::u32,
					name: sp_std::vec::Vec<::core::primitive::u8>,
					module_name: sp_std::vec::Vec<::core::primitive::u8>,
					#[codec(compact)]
					crate_major: ::core::primitive::u32,
					#[codec(compact)]
					min_crate_minor: ::core::primitive::u32,
				},
				#[codec(index = 35)]
				ReportTransactStatus(runtime_types::staging_xcm::v4::QueryResponseInfo),
				#[codec(index = 36)]
				ClearTransactStatus,
				#[codec(index = 37)]
				UniversalOrigin(runtime_types::staging_xcm::v4::junction::Junction),
				#[codec(index = 38)]
				ExportMessage {
					network: runtime_types::staging_xcm::v4::junction::NetworkId,
					destination: runtime_types::staging_xcm::v4::junctions::Junctions,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 39)]
				LockAsset {
					asset: runtime_types::staging_xcm::v4::asset::Asset,
					unlocker: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 40)]
				UnlockAsset {
					asset: runtime_types::staging_xcm::v4::asset::Asset,
					target: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 41)]
				NoteUnlockable {
					asset: runtime_types::staging_xcm::v4::asset::Asset,
					owner: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 42)]
				RequestUnlock {
					asset: runtime_types::staging_xcm::v4::asset::Asset,
					locker: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 43)]
				SetFeesMode { jit_withdraw: ::core::primitive::bool },
				#[codec(index = 44)]
				SetTopic([::core::primitive::u8; 32usize]),
				#[codec(index = 45)]
				ClearTopic,
				#[codec(index = 46)]
				AliasOrigin(runtime_types::staging_xcm::v4::location::Location),
				#[codec(index = 47)]
				UnpaidExecution {
					weight_limit: runtime_types::xcm::v3::WeightLimit,
					check_origin:
						::core::option::Option<runtime_types::staging_xcm::v4::location::Location>,
				},
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Instruction2 {
				#[codec(index = 0)]
				WithdrawAsset(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 1)]
				ReserveAssetDeposited(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 2)]
				ReceiveTeleportedAsset(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 3)]
				QueryResponse {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					response: runtime_types::staging_xcm::v4::Response,
					max_weight: runtime_types::sp_weights::weight_v2::Weight,
					querier:
						::core::option::Option<runtime_types::staging_xcm::v4::location::Location>,
				},
				#[codec(index = 4)]
				TransferAsset {
					assets: runtime_types::staging_xcm::v4::asset::Assets,
					beneficiary: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 5)]
				TransferReserveAsset {
					assets: runtime_types::staging_xcm::v4::asset::Assets,
					dest: runtime_types::staging_xcm::v4::location::Location,
					xcm: runtime_types::staging_xcm::v4::Xcm,
				},
				#[codec(index = 6)]
				Transact {
					origin_kind: runtime_types::xcm::v3::OriginKind,
					require_weight_at_most: runtime_types::sp_weights::weight_v2::Weight,
					call: runtime_types::xcm::double_encoded::DoubleEncoded2,
				},
				#[codec(index = 7)]
				HrmpNewChannelOpenRequest {
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					max_message_size: ::core::primitive::u32,
					#[codec(compact)]
					max_capacity: ::core::primitive::u32,
				},
				#[codec(index = 8)]
				HrmpChannelAccepted {
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 9)]
				HrmpChannelClosing {
					#[codec(compact)]
					initiator: ::core::primitive::u32,
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 10)]
				ClearOrigin,
				#[codec(index = 11)]
				DescendOrigin(runtime_types::staging_xcm::v4::junctions::Junctions),
				#[codec(index = 12)]
				ReportError(runtime_types::staging_xcm::v4::QueryResponseInfo),
				#[codec(index = 13)]
				DepositAsset {
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
					beneficiary: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 14)]
				DepositReserveAsset {
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
					dest: runtime_types::staging_xcm::v4::location::Location,
					xcm: runtime_types::staging_xcm::v4::Xcm,
				},
				#[codec(index = 15)]
				ExchangeAsset {
					give: runtime_types::staging_xcm::v4::asset::AssetFilter,
					want: runtime_types::staging_xcm::v4::asset::Assets,
					maximal: ::core::primitive::bool,
				},
				#[codec(index = 16)]
				InitiateReserveWithdraw {
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
					reserve: runtime_types::staging_xcm::v4::location::Location,
					xcm: runtime_types::staging_xcm::v4::Xcm,
				},
				#[codec(index = 17)]
				InitiateTeleport {
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
					dest: runtime_types::staging_xcm::v4::location::Location,
					xcm: runtime_types::staging_xcm::v4::Xcm,
				},
				#[codec(index = 18)]
				ReportHolding {
					response_info: runtime_types::staging_xcm::v4::QueryResponseInfo,
					assets: runtime_types::staging_xcm::v4::asset::AssetFilter,
				},
				#[codec(index = 19)]
				BuyExecution {
					fees: runtime_types::staging_xcm::v4::asset::Asset,
					weight_limit: runtime_types::xcm::v3::WeightLimit,
				},
				#[codec(index = 20)]
				RefundSurplus,
				#[codec(index = 21)]
				SetErrorHandler(::core::primitive::bool),
				#[codec(index = 22)]
				SetAppendix(::core::primitive::bool),
				#[codec(index = 23)]
				ClearError,
				#[codec(index = 24)]
				ClaimAsset {
					assets: runtime_types::staging_xcm::v4::asset::Assets,
					ticket: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 25)]
				Trap(#[codec(compact)] ::core::primitive::u64),
				#[codec(index = 26)]
				SubscribeVersion {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					max_response_weight: runtime_types::sp_weights::weight_v2::Weight,
				},
				#[codec(index = 27)]
				UnsubscribeVersion,
				#[codec(index = 28)]
				BurnAsset(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 29)]
				ExpectAsset(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 30)]
				ExpectOrigin(
					::core::option::Option<runtime_types::staging_xcm::v4::location::Location>,
				),
				#[codec(index = 31)]
				ExpectError(
					::core::option::Option<(
						::core::primitive::u32,
						runtime_types::xcm::v3::traits::Error,
					)>,
				),
				#[codec(index = 32)]
				ExpectTransactStatus(runtime_types::xcm::v3::MaybeErrorCode),
				#[codec(index = 33)]
				QueryPallet {
					module_name: sp_std::vec::Vec<::core::primitive::u8>,
					response_info: runtime_types::staging_xcm::v4::QueryResponseInfo,
				},
				#[codec(index = 34)]
				ExpectPallet {
					#[codec(compact)]
					index: ::core::primitive::u32,
					name: sp_std::vec::Vec<::core::primitive::u8>,
					module_name: sp_std::vec::Vec<::core::primitive::u8>,
					#[codec(compact)]
					crate_major: ::core::primitive::u32,
					#[codec(compact)]
					min_crate_minor: ::core::primitive::u32,
				},
				#[codec(index = 35)]
				ReportTransactStatus(runtime_types::staging_xcm::v4::QueryResponseInfo),
				#[codec(index = 36)]
				ClearTransactStatus,
				#[codec(index = 37)]
				UniversalOrigin(runtime_types::staging_xcm::v4::junction::Junction),
				#[codec(index = 38)]
				ExportMessage {
					network: runtime_types::staging_xcm::v4::junction::NetworkId,
					destination: runtime_types::staging_xcm::v4::junctions::Junctions,
					xcm: runtime_types::staging_xcm::v4::Xcm,
				},
				#[codec(index = 39)]
				LockAsset {
					asset: runtime_types::staging_xcm::v4::asset::Asset,
					unlocker: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 40)]
				UnlockAsset {
					asset: runtime_types::staging_xcm::v4::asset::Asset,
					target: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 41)]
				NoteUnlockable {
					asset: runtime_types::staging_xcm::v4::asset::Asset,
					owner: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 42)]
				RequestUnlock {
					asset: runtime_types::staging_xcm::v4::asset::Asset,
					locker: runtime_types::staging_xcm::v4::location::Location,
				},
				#[codec(index = 43)]
				SetFeesMode { jit_withdraw: ::core::primitive::bool },
				#[codec(index = 44)]
				SetTopic([::core::primitive::u8; 32usize]),
				#[codec(index = 45)]
				ClearTopic,
				#[codec(index = 46)]
				AliasOrigin(runtime_types::staging_xcm::v4::location::Location),
				#[codec(index = 47)]
				UnpaidExecution {
					weight_limit: runtime_types::xcm::v3::WeightLimit,
					check_origin:
						::core::option::Option<runtime_types::staging_xcm::v4::location::Location>,
				},
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct PalletInfo {
				#[codec(compact)]
				pub index: ::core::primitive::u32,
				pub name: runtime_types::bounded_collections::bounded_vec::BoundedVec<
					::core::primitive::u8,
				>,
				pub module_name: runtime_types::bounded_collections::bounded_vec::BoundedVec<
					::core::primitive::u8,
				>,
				#[codec(compact)]
				pub major: ::core::primitive::u32,
				#[codec(compact)]
				pub minor: ::core::primitive::u32,
				#[codec(compact)]
				pub patch: ::core::primitive::u32,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct QueryResponseInfo {
				pub destination: runtime_types::staging_xcm::v4::location::Location,
				#[codec(compact)]
				pub query_id: ::core::primitive::u64,
				pub max_weight: runtime_types::sp_weights::weight_v2::Weight,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Response {
				#[codec(index = 0)]
				Null,
				#[codec(index = 1)]
				Assets(runtime_types::staging_xcm::v4::asset::Assets),
				#[codec(index = 2)]
				ExecutionResult(
					::core::option::Option<(
						::core::primitive::u32,
						runtime_types::xcm::v3::traits::Error,
					)>,
				),
				#[codec(index = 3)]
				Version(::core::primitive::u32),
				#[codec(index = 4)]
				PalletsInfo(
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						runtime_types::staging_xcm::v4::PalletInfo,
					>,
				),
				#[codec(index = 5)]
				DispatchResult(runtime_types::xcm::v3::MaybeErrorCode),
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct Xcm(pub sp_std::vec::Vec<runtime_types::staging_xcm::v4::Instruction>);
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct Xcm2(pub sp_std::vec::Vec<runtime_types::staging_xcm::v4::Instruction2>);
		}
	}
	pub mod xcm {
		use super::*;
		pub mod double_encoded {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct DoubleEncoded {
				pub encoded: sp_std::vec::Vec<::core::primitive::u8>,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct DoubleEncoded2 {
				pub encoded: sp_std::vec::Vec<::core::primitive::u8>,
			}
		}
		pub mod v2 {
			use super::*;
			pub mod junction {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Junction {
					#[codec(index = 0)]
					Parachain(#[codec(compact)] ::core::primitive::u32),
					#[codec(index = 1)]
					AccountId32 {
						network: runtime_types::xcm::v2::NetworkId,
						id: [::core::primitive::u8; 32usize],
					},
					#[codec(index = 2)]
					AccountIndex64 {
						network: runtime_types::xcm::v2::NetworkId,
						#[codec(compact)]
						index: ::core::primitive::u64,
					},
					#[codec(index = 3)]
					AccountKey20 {
						network: runtime_types::xcm::v2::NetworkId,
						key: [::core::primitive::u8; 20usize],
					},
					#[codec(index = 4)]
					PalletInstance(::core::primitive::u8),
					#[codec(index = 5)]
					GeneralIndex(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 6)]
					GeneralKey(
						runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<
							::core::primitive::u8,
						>,
					),
					#[codec(index = 7)]
					OnlyChild,
					#[codec(index = 8)]
					Plurality {
						id: runtime_types::xcm::v2::BodyId,
						part: runtime_types::xcm::v2::BodyPart,
					},
				}
			}
			pub mod multiasset {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum AssetId {
					#[codec(index = 0)]
					Concrete(runtime_types::xcm::v2::multilocation::MultiLocation),
					#[codec(index = 1)]
					Abstract(sp_std::vec::Vec<::core::primitive::u8>),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum AssetInstance {
					#[codec(index = 0)]
					Undefined,
					#[codec(index = 1)]
					Index(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 2)]
					Array4([::core::primitive::u8; 4usize]),
					#[codec(index = 3)]
					Array8([::core::primitive::u8; 8usize]),
					#[codec(index = 4)]
					Array16([::core::primitive::u8; 16usize]),
					#[codec(index = 5)]
					Array32([::core::primitive::u8; 32usize]),
					#[codec(index = 6)]
					Blob(sp_std::vec::Vec<::core::primitive::u8>),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Fungibility {
					#[codec(index = 0)]
					Fungible(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 1)]
					NonFungible(runtime_types::xcm::v2::multiasset::AssetInstance),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct MultiAsset {
					pub id: runtime_types::xcm::v2::multiasset::AssetId,
					pub fun: runtime_types::xcm::v2::multiasset::Fungibility,
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum MultiAssetFilter {
					#[codec(index = 0)]
					Definite(runtime_types::xcm::v2::multiasset::MultiAssets),
					#[codec(index = 1)]
					Wild(runtime_types::xcm::v2::multiasset::WildMultiAsset),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct MultiAssets(
					pub sp_std::vec::Vec<runtime_types::xcm::v2::multiasset::MultiAsset>,
				);
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum WildFungibility {
					#[codec(index = 0)]
					Fungible,
					#[codec(index = 1)]
					NonFungible,
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum WildMultiAsset {
					#[codec(index = 0)]
					All,
					#[codec(index = 1)]
					AllOf {
						id: runtime_types::xcm::v2::multiasset::AssetId,
						fun: runtime_types::xcm::v2::multiasset::WildFungibility,
					},
				}
			}
			pub mod multilocation {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Junctions {
					#[codec(index = 0)]
					Here,
					#[codec(index = 1)]
					X1(runtime_types::xcm::v2::junction::Junction),
					#[codec(index = 2)]
					X2(
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
					),
					#[codec(index = 3)]
					X3(
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
					),
					#[codec(index = 4)]
					X4(
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
					),
					#[codec(index = 5)]
					X5(
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
					),
					#[codec(index = 6)]
					X6(
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
					),
					#[codec(index = 7)]
					X7(
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
					),
					#[codec(index = 8)]
					X8(
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
						runtime_types::xcm::v2::junction::Junction,
					),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct MultiLocation {
					pub parents: ::core::primitive::u8,
					pub interior: runtime_types::xcm::v2::multilocation::Junctions,
				}
			}
			pub mod traits {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Error {
					#[codec(index = 0)]
					Overflow,
					#[codec(index = 1)]
					Unimplemented,
					#[codec(index = 2)]
					UntrustedReserveLocation,
					#[codec(index = 3)]
					UntrustedTeleportLocation,
					#[codec(index = 4)]
					MultiLocationFull,
					#[codec(index = 5)]
					MultiLocationNotInvertible,
					#[codec(index = 6)]
					BadOrigin,
					#[codec(index = 7)]
					InvalidLocation,
					#[codec(index = 8)]
					AssetNotFound,
					#[codec(index = 9)]
					FailedToTransactAsset,
					#[codec(index = 10)]
					NotWithdrawable,
					#[codec(index = 11)]
					LocationCannotHold,
					#[codec(index = 12)]
					ExceedsMaxMessageSize,
					#[codec(index = 13)]
					DestinationUnsupported,
					#[codec(index = 14)]
					Transport,
					#[codec(index = 15)]
					Unroutable,
					#[codec(index = 16)]
					UnknownClaim,
					#[codec(index = 17)]
					FailedToDecode,
					#[codec(index = 18)]
					MaxWeightInvalid,
					#[codec(index = 19)]
					NotHoldingFees,
					#[codec(index = 20)]
					TooExpensive,
					#[codec(index = 21)]
					Trap(::core::primitive::u64),
					#[codec(index = 22)]
					UnhandledXcmVersion,
					#[codec(index = 23)]
					WeightLimitReached(::core::primitive::u64),
					#[codec(index = 24)]
					Barrier,
					#[codec(index = 25)]
					WeightNotComputable,
				}
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum BodyId {
				#[codec(index = 0)]
				Unit,
				#[codec(index = 1)]
				Named(
					runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<
						::core::primitive::u8,
					>,
				),
				#[codec(index = 2)]
				Index(#[codec(compact)] ::core::primitive::u32),
				#[codec(index = 3)]
				Executive,
				#[codec(index = 4)]
				Technical,
				#[codec(index = 5)]
				Legislative,
				#[codec(index = 6)]
				Judicial,
				#[codec(index = 7)]
				Defense,
				#[codec(index = 8)]
				Administration,
				#[codec(index = 9)]
				Treasury,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum BodyPart {
				#[codec(index = 0)]
				Voice,
				#[codec(index = 1)]
				Members {
					#[codec(compact)]
					count: ::core::primitive::u32,
				},
				#[codec(index = 2)]
				Fraction {
					#[codec(compact)]
					nom: ::core::primitive::u32,
					#[codec(compact)]
					denom: ::core::primitive::u32,
				},
				#[codec(index = 3)]
				AtLeastProportion {
					#[codec(compact)]
					nom: ::core::primitive::u32,
					#[codec(compact)]
					denom: ::core::primitive::u32,
				},
				#[codec(index = 4)]
				MoreThanProportion {
					#[codec(compact)]
					nom: ::core::primitive::u32,
					#[codec(compact)]
					denom: ::core::primitive::u32,
				},
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Instruction {
				#[codec(index = 0)]
				WithdrawAsset(runtime_types::xcm::v2::multiasset::MultiAssets),
				#[codec(index = 1)]
				ReserveAssetDeposited(runtime_types::xcm::v2::multiasset::MultiAssets),
				#[codec(index = 2)]
				ReceiveTeleportedAsset(runtime_types::xcm::v2::multiasset::MultiAssets),
				#[codec(index = 3)]
				QueryResponse {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					response: runtime_types::xcm::v2::Response,
					#[codec(compact)]
					max_weight: ::core::primitive::u64,
				},
				#[codec(index = 4)]
				TransferAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssets,
					beneficiary: runtime_types::xcm::v2::multilocation::MultiLocation,
				},
				#[codec(index = 5)]
				TransferReserveAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssets,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 6)]
				Transact {
					origin_type: runtime_types::xcm::v2::OriginKind,
					#[codec(compact)]
					require_weight_at_most: ::core::primitive::u64,
					call: runtime_types::xcm::double_encoded::DoubleEncoded,
				},
				#[codec(index = 7)]
				HrmpNewChannelOpenRequest {
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					max_message_size: ::core::primitive::u32,
					#[codec(compact)]
					max_capacity: ::core::primitive::u32,
				},
				#[codec(index = 8)]
				HrmpChannelAccepted {
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 9)]
				HrmpChannelClosing {
					#[codec(compact)]
					initiator: ::core::primitive::u32,
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 10)]
				ClearOrigin,
				#[codec(index = 11)]
				DescendOrigin(runtime_types::xcm::v2::multilocation::Junctions),
				#[codec(index = 12)]
				ReportError {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					#[codec(compact)]
					max_response_weight: ::core::primitive::u64,
				},
				#[codec(index = 13)]
				DepositAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					#[codec(compact)]
					max_assets: ::core::primitive::u32,
					beneficiary: runtime_types::xcm::v2::multilocation::MultiLocation,
				},
				#[codec(index = 14)]
				DepositReserveAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					#[codec(compact)]
					max_assets: ::core::primitive::u32,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 15)]
				ExchangeAsset {
					give: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					receive: runtime_types::xcm::v2::multiasset::MultiAssets,
				},
				#[codec(index = 16)]
				InitiateReserveWithdraw {
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					reserve: runtime_types::xcm::v2::multilocation::MultiLocation,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 17)]
				InitiateTeleport {
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 18)]
				QueryHolding {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					#[codec(compact)]
					max_response_weight: ::core::primitive::u64,
				},
				#[codec(index = 19)]
				BuyExecution {
					fees: runtime_types::xcm::v2::multiasset::MultiAsset,
					weight_limit: runtime_types::xcm::v2::WeightLimit,
				},
				#[codec(index = 20)]
				RefundSurplus,
				#[codec(index = 21)]
				SetErrorHandler(::core::primitive::bool),
				#[codec(index = 22)]
				SetAppendix(::core::primitive::bool),
				#[codec(index = 23)]
				ClearError,
				#[codec(index = 24)]
				ClaimAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssets,
					ticket: runtime_types::xcm::v2::multilocation::MultiLocation,
				},
				#[codec(index = 25)]
				Trap(#[codec(compact)] ::core::primitive::u64),
				#[codec(index = 26)]
				SubscribeVersion {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					#[codec(compact)]
					max_response_weight: ::core::primitive::u64,
				},
				#[codec(index = 27)]
				UnsubscribeVersion,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Instruction2 {
				#[codec(index = 0)]
				WithdrawAsset(runtime_types::xcm::v2::multiasset::MultiAssets),
				#[codec(index = 1)]
				ReserveAssetDeposited(runtime_types::xcm::v2::multiasset::MultiAssets),
				#[codec(index = 2)]
				ReceiveTeleportedAsset(runtime_types::xcm::v2::multiasset::MultiAssets),
				#[codec(index = 3)]
				QueryResponse {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					response: runtime_types::xcm::v2::Response,
					#[codec(compact)]
					max_weight: ::core::primitive::u64,
				},
				#[codec(index = 4)]
				TransferAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssets,
					beneficiary: runtime_types::xcm::v2::multilocation::MultiLocation,
				},
				#[codec(index = 5)]
				TransferReserveAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssets,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					xcm: runtime_types::xcm::v2::Xcm,
				},
				#[codec(index = 6)]
				Transact {
					origin_type: runtime_types::xcm::v2::OriginKind,
					#[codec(compact)]
					require_weight_at_most: ::core::primitive::u64,
					call: runtime_types::xcm::double_encoded::DoubleEncoded2,
				},
				#[codec(index = 7)]
				HrmpNewChannelOpenRequest {
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					max_message_size: ::core::primitive::u32,
					#[codec(compact)]
					max_capacity: ::core::primitive::u32,
				},
				#[codec(index = 8)]
				HrmpChannelAccepted {
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 9)]
				HrmpChannelClosing {
					#[codec(compact)]
					initiator: ::core::primitive::u32,
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 10)]
				ClearOrigin,
				#[codec(index = 11)]
				DescendOrigin(runtime_types::xcm::v2::multilocation::Junctions),
				#[codec(index = 12)]
				ReportError {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					#[codec(compact)]
					max_response_weight: ::core::primitive::u64,
				},
				#[codec(index = 13)]
				DepositAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					#[codec(compact)]
					max_assets: ::core::primitive::u32,
					beneficiary: runtime_types::xcm::v2::multilocation::MultiLocation,
				},
				#[codec(index = 14)]
				DepositReserveAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					#[codec(compact)]
					max_assets: ::core::primitive::u32,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					xcm: runtime_types::xcm::v2::Xcm,
				},
				#[codec(index = 15)]
				ExchangeAsset {
					give: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					receive: runtime_types::xcm::v2::multiasset::MultiAssets,
				},
				#[codec(index = 16)]
				InitiateReserveWithdraw {
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					reserve: runtime_types::xcm::v2::multilocation::MultiLocation,
					xcm: runtime_types::xcm::v2::Xcm,
				},
				#[codec(index = 17)]
				InitiateTeleport {
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					xcm: runtime_types::xcm::v2::Xcm,
				},
				#[codec(index = 18)]
				QueryHolding {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					dest: runtime_types::xcm::v2::multilocation::MultiLocation,
					assets: runtime_types::xcm::v2::multiasset::MultiAssetFilter,
					#[codec(compact)]
					max_response_weight: ::core::primitive::u64,
				},
				#[codec(index = 19)]
				BuyExecution {
					fees: runtime_types::xcm::v2::multiasset::MultiAsset,
					weight_limit: runtime_types::xcm::v2::WeightLimit,
				},
				#[codec(index = 20)]
				RefundSurplus,
				#[codec(index = 21)]
				SetErrorHandler(::core::primitive::bool),
				#[codec(index = 22)]
				SetAppendix(::core::primitive::bool),
				#[codec(index = 23)]
				ClearError,
				#[codec(index = 24)]
				ClaimAsset {
					assets: runtime_types::xcm::v2::multiasset::MultiAssets,
					ticket: runtime_types::xcm::v2::multilocation::MultiLocation,
				},
				#[codec(index = 25)]
				Trap(#[codec(compact)] ::core::primitive::u64),
				#[codec(index = 26)]
				SubscribeVersion {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					#[codec(compact)]
					max_response_weight: ::core::primitive::u64,
				},
				#[codec(index = 27)]
				UnsubscribeVersion,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum NetworkId {
				#[codec(index = 0)]
				Any,
				#[codec(index = 1)]
				Named(
					runtime_types::bounded_collections::weak_bounded_vec::WeakBoundedVec<
						::core::primitive::u8,
					>,
				),
				#[codec(index = 2)]
				Polkadot,
				#[codec(index = 3)]
				Kusama,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum OriginKind {
				#[codec(index = 0)]
				Native,
				#[codec(index = 1)]
				SovereignAccount,
				#[codec(index = 2)]
				Superuser,
				#[codec(index = 3)]
				Xcm,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Response {
				#[codec(index = 0)]
				Null,
				#[codec(index = 1)]
				Assets(runtime_types::xcm::v2::multiasset::MultiAssets),
				#[codec(index = 2)]
				ExecutionResult(
					::core::option::Option<(
						::core::primitive::u32,
						runtime_types::xcm::v2::traits::Error,
					)>,
				),
				#[codec(index = 3)]
				Version(::core::primitive::u32),
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum WeightLimit {
				#[codec(index = 0)]
				Unlimited,
				#[codec(index = 1)]
				Limited(#[codec(compact)] ::core::primitive::u64),
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct Xcm(pub sp_std::vec::Vec<runtime_types::xcm::v2::Instruction>);
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct Xcm2(pub sp_std::vec::Vec<runtime_types::xcm::v2::Instruction2>);
		}
		pub mod v3 {
			use super::*;
			pub mod junction {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum BodyId {
					#[codec(index = 0)]
					Unit,
					#[codec(index = 1)]
					Moniker([::core::primitive::u8; 4usize]),
					#[codec(index = 2)]
					Index(#[codec(compact)] ::core::primitive::u32),
					#[codec(index = 3)]
					Executive,
					#[codec(index = 4)]
					Technical,
					#[codec(index = 5)]
					Legislative,
					#[codec(index = 6)]
					Judicial,
					#[codec(index = 7)]
					Defense,
					#[codec(index = 8)]
					Administration,
					#[codec(index = 9)]
					Treasury,
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum BodyPart {
					#[codec(index = 0)]
					Voice,
					#[codec(index = 1)]
					Members {
						#[codec(compact)]
						count: ::core::primitive::u32,
					},
					#[codec(index = 2)]
					Fraction {
						#[codec(compact)]
						nom: ::core::primitive::u32,
						#[codec(compact)]
						denom: ::core::primitive::u32,
					},
					#[codec(index = 3)]
					AtLeastProportion {
						#[codec(compact)]
						nom: ::core::primitive::u32,
						#[codec(compact)]
						denom: ::core::primitive::u32,
					},
					#[codec(index = 4)]
					MoreThanProportion {
						#[codec(compact)]
						nom: ::core::primitive::u32,
						#[codec(compact)]
						denom: ::core::primitive::u32,
					},
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Junction {
					#[codec(index = 0)]
					Parachain(#[codec(compact)] ::core::primitive::u32),
					#[codec(index = 1)]
					AccountId32 {
						network:
							::core::option::Option<runtime_types::xcm::v3::junction::NetworkId>,
						id: [::core::primitive::u8; 32usize],
					},
					#[codec(index = 2)]
					AccountIndex64 {
						network:
							::core::option::Option<runtime_types::xcm::v3::junction::NetworkId>,
						#[codec(compact)]
						index: ::core::primitive::u64,
					},
					#[codec(index = 3)]
					AccountKey20 {
						network:
							::core::option::Option<runtime_types::xcm::v3::junction::NetworkId>,
						key: [::core::primitive::u8; 20usize],
					},
					#[codec(index = 4)]
					PalletInstance(::core::primitive::u8),
					#[codec(index = 5)]
					GeneralIndex(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 6)]
					GeneralKey {
						length: ::core::primitive::u8,
						data: [::core::primitive::u8; 32usize],
					},
					#[codec(index = 7)]
					OnlyChild,
					#[codec(index = 8)]
					Plurality {
						id: runtime_types::xcm::v3::junction::BodyId,
						part: runtime_types::xcm::v3::junction::BodyPart,
					},
					#[codec(index = 9)]
					GlobalConsensus(runtime_types::xcm::v3::junction::NetworkId),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum NetworkId {
					#[codec(index = 0)]
					ByGenesis([::core::primitive::u8; 32usize]),
					#[codec(index = 1)]
					ByFork {
						block_number: ::core::primitive::u64,
						block_hash: [::core::primitive::u8; 32usize],
					},
					#[codec(index = 2)]
					Polkadot,
					#[codec(index = 3)]
					Kusama,
					#[codec(index = 4)]
					Westend,
					#[codec(index = 5)]
					Rococo,
					#[codec(index = 6)]
					Wococo,
					#[codec(index = 7)]
					Ethereum {
						#[codec(compact)]
						chain_id: ::core::primitive::u64,
					},
					#[codec(index = 8)]
					BitcoinCore,
					#[codec(index = 9)]
					BitcoinCash,
					#[codec(index = 10)]
					PolkadotBulletin,
				}
			}
			pub mod junctions {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Junctions {
					#[codec(index = 0)]
					Here,
					#[codec(index = 1)]
					X1(runtime_types::xcm::v3::junction::Junction),
					#[codec(index = 2)]
					X2(
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
					),
					#[codec(index = 3)]
					X3(
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
					),
					#[codec(index = 4)]
					X4(
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
					),
					#[codec(index = 5)]
					X5(
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
					),
					#[codec(index = 6)]
					X6(
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
					),
					#[codec(index = 7)]
					X7(
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
					),
					#[codec(index = 8)]
					X8(
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
						runtime_types::xcm::v3::junction::Junction,
					),
				}
			}
			pub mod multiasset {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum AssetId {
					#[codec(index = 0)]
					Concrete(runtime_types::staging_xcm::v3::multilocation::MultiLocation),
					#[codec(index = 1)]
					Abstract([::core::primitive::u8; 32usize]),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum AssetInstance {
					#[codec(index = 0)]
					Undefined,
					#[codec(index = 1)]
					Index(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 2)]
					Array4([::core::primitive::u8; 4usize]),
					#[codec(index = 3)]
					Array8([::core::primitive::u8; 8usize]),
					#[codec(index = 4)]
					Array16([::core::primitive::u8; 16usize]),
					#[codec(index = 5)]
					Array32([::core::primitive::u8; 32usize]),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Fungibility {
					#[codec(index = 0)]
					Fungible(#[codec(compact)] ::core::primitive::u128),
					#[codec(index = 1)]
					NonFungible(runtime_types::xcm::v3::multiasset::AssetInstance),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct MultiAsset {
					pub id: runtime_types::xcm::v3::multiasset::AssetId,
					pub fun: runtime_types::xcm::v3::multiasset::Fungibility,
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum MultiAssetFilter {
					#[codec(index = 0)]
					Definite(runtime_types::xcm::v3::multiasset::MultiAssets),
					#[codec(index = 1)]
					Wild(runtime_types::xcm::v3::multiasset::WildMultiAsset),
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub struct MultiAssets(
					pub sp_std::vec::Vec<runtime_types::xcm::v3::multiasset::MultiAsset>,
				);
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum WildFungibility {
					#[codec(index = 0)]
					Fungible,
					#[codec(index = 1)]
					NonFungible,
				}
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum WildMultiAsset {
					#[codec(index = 0)]
					All,
					#[codec(index = 1)]
					AllOf {
						id: runtime_types::xcm::v3::multiasset::AssetId,
						fun: runtime_types::xcm::v3::multiasset::WildFungibility,
					},
					#[codec(index = 2)]
					AllCounted(#[codec(compact)] ::core::primitive::u32),
					#[codec(index = 3)]
					AllOfCounted {
						id: runtime_types::xcm::v3::multiasset::AssetId,
						fun: runtime_types::xcm::v3::multiasset::WildFungibility,
						#[codec(compact)]
						count: ::core::primitive::u32,
					},
				}
			}
			pub mod traits {
				use super::*;
				#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
				pub enum Error {
					#[codec(index = 0)]
					Overflow,
					#[codec(index = 1)]
					Unimplemented,
					#[codec(index = 2)]
					UntrustedReserveLocation,
					#[codec(index = 3)]
					UntrustedTeleportLocation,
					#[codec(index = 4)]
					LocationFull,
					#[codec(index = 5)]
					LocationNotInvertible,
					#[codec(index = 6)]
					BadOrigin,
					#[codec(index = 7)]
					InvalidLocation,
					#[codec(index = 8)]
					AssetNotFound,
					#[codec(index = 9)]
					FailedToTransactAsset,
					#[codec(index = 10)]
					NotWithdrawable,
					#[codec(index = 11)]
					LocationCannotHold,
					#[codec(index = 12)]
					ExceedsMaxMessageSize,
					#[codec(index = 13)]
					DestinationUnsupported,
					#[codec(index = 14)]
					Transport,
					#[codec(index = 15)]
					Unroutable,
					#[codec(index = 16)]
					UnknownClaim,
					#[codec(index = 17)]
					FailedToDecode,
					#[codec(index = 18)]
					MaxWeightInvalid,
					#[codec(index = 19)]
					NotHoldingFees,
					#[codec(index = 20)]
					TooExpensive,
					#[codec(index = 21)]
					Trap(::core::primitive::u64),
					#[codec(index = 22)]
					ExpectationFalse,
					#[codec(index = 23)]
					PalletNotFound,
					#[codec(index = 24)]
					NameMismatch,
					#[codec(index = 25)]
					VersionIncompatible,
					#[codec(index = 26)]
					HoldingWouldOverflow,
					#[codec(index = 27)]
					ExportError,
					#[codec(index = 28)]
					ReanchorFailed,
					#[codec(index = 29)]
					NoDeal,
					#[codec(index = 30)]
					FeesNotMet,
					#[codec(index = 31)]
					LockError,
					#[codec(index = 32)]
					NoPermission,
					#[codec(index = 33)]
					Unanchored,
					#[codec(index = 34)]
					NotDepositable,
					#[codec(index = 35)]
					UnhandledXcmVersion,
					#[codec(index = 36)]
					WeightLimitReached(runtime_types::sp_weights::weight_v2::Weight),
					#[codec(index = 37)]
					Barrier,
					#[codec(index = 38)]
					WeightNotComputable,
					#[codec(index = 39)]
					ExceedsStackLimit,
				}
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Instruction {
				#[codec(index = 0)]
				WithdrawAsset(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 1)]
				ReserveAssetDeposited(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 2)]
				ReceiveTeleportedAsset(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 3)]
				QueryResponse {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					response: runtime_types::xcm::v3::Response,
					max_weight: runtime_types::sp_weights::weight_v2::Weight,
					querier: ::core::option::Option<
						runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					>,
				},
				#[codec(index = 4)]
				TransferAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssets,
					beneficiary: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 5)]
				TransferReserveAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssets,
					dest: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 6)]
				Transact {
					origin_kind: runtime_types::xcm::v3::OriginKind,
					require_weight_at_most: runtime_types::sp_weights::weight_v2::Weight,
					call: runtime_types::xcm::double_encoded::DoubleEncoded,
				},
				#[codec(index = 7)]
				HrmpNewChannelOpenRequest {
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					max_message_size: ::core::primitive::u32,
					#[codec(compact)]
					max_capacity: ::core::primitive::u32,
				},
				#[codec(index = 8)]
				HrmpChannelAccepted {
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 9)]
				HrmpChannelClosing {
					#[codec(compact)]
					initiator: ::core::primitive::u32,
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 10)]
				ClearOrigin,
				#[codec(index = 11)]
				DescendOrigin(runtime_types::xcm::v3::junctions::Junctions),
				#[codec(index = 12)]
				ReportError(runtime_types::xcm::v3::QueryResponseInfo),
				#[codec(index = 13)]
				DepositAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					beneficiary: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 14)]
				DepositReserveAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					dest: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 15)]
				ExchangeAsset {
					give: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					want: runtime_types::xcm::v3::multiasset::MultiAssets,
					maximal: ::core::primitive::bool,
				},
				#[codec(index = 16)]
				InitiateReserveWithdraw {
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					reserve: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 17)]
				InitiateTeleport {
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					dest: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 18)]
				ReportHolding {
					response_info: runtime_types::xcm::v3::QueryResponseInfo,
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
				},
				#[codec(index = 19)]
				BuyExecution {
					fees: runtime_types::xcm::v3::multiasset::MultiAsset,
					weight_limit: runtime_types::xcm::v3::WeightLimit,
				},
				#[codec(index = 20)]
				RefundSurplus,
				#[codec(index = 21)]
				SetErrorHandler(::core::primitive::bool),
				#[codec(index = 22)]
				SetAppendix(::core::primitive::bool),
				#[codec(index = 23)]
				ClearError,
				#[codec(index = 24)]
				ClaimAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssets,
					ticket: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 25)]
				Trap(#[codec(compact)] ::core::primitive::u64),
				#[codec(index = 26)]
				SubscribeVersion {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					max_response_weight: runtime_types::sp_weights::weight_v2::Weight,
				},
				#[codec(index = 27)]
				UnsubscribeVersion,
				#[codec(index = 28)]
				BurnAsset(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 29)]
				ExpectAsset(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 30)]
				ExpectOrigin(
					::core::option::Option<
						runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					>,
				),
				#[codec(index = 31)]
				ExpectError(
					::core::option::Option<(
						::core::primitive::u32,
						runtime_types::xcm::v3::traits::Error,
					)>,
				),
				#[codec(index = 32)]
				ExpectTransactStatus(runtime_types::xcm::v3::MaybeErrorCode),
				#[codec(index = 33)]
				QueryPallet {
					module_name: sp_std::vec::Vec<::core::primitive::u8>,
					response_info: runtime_types::xcm::v3::QueryResponseInfo,
				},
				#[codec(index = 34)]
				ExpectPallet {
					#[codec(compact)]
					index: ::core::primitive::u32,
					name: sp_std::vec::Vec<::core::primitive::u8>,
					module_name: sp_std::vec::Vec<::core::primitive::u8>,
					#[codec(compact)]
					crate_major: ::core::primitive::u32,
					#[codec(compact)]
					min_crate_minor: ::core::primitive::u32,
				},
				#[codec(index = 35)]
				ReportTransactStatus(runtime_types::xcm::v3::QueryResponseInfo),
				#[codec(index = 36)]
				ClearTransactStatus,
				#[codec(index = 37)]
				UniversalOrigin(runtime_types::xcm::v3::junction::Junction),
				#[codec(index = 38)]
				ExportMessage {
					network: runtime_types::xcm::v3::junction::NetworkId,
					destination: runtime_types::xcm::v3::junctions::Junctions,
					xcm: ::core::primitive::bool,
				},
				#[codec(index = 39)]
				LockAsset {
					asset: runtime_types::xcm::v3::multiasset::MultiAsset,
					unlocker: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 40)]
				UnlockAsset {
					asset: runtime_types::xcm::v3::multiasset::MultiAsset,
					target: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 41)]
				NoteUnlockable {
					asset: runtime_types::xcm::v3::multiasset::MultiAsset,
					owner: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 42)]
				RequestUnlock {
					asset: runtime_types::xcm::v3::multiasset::MultiAsset,
					locker: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 43)]
				SetFeesMode { jit_withdraw: ::core::primitive::bool },
				#[codec(index = 44)]
				SetTopic([::core::primitive::u8; 32usize]),
				#[codec(index = 45)]
				ClearTopic,
				#[codec(index = 46)]
				AliasOrigin(runtime_types::staging_xcm::v3::multilocation::MultiLocation),
				#[codec(index = 47)]
				UnpaidExecution {
					weight_limit: runtime_types::xcm::v3::WeightLimit,
					check_origin: ::core::option::Option<
						runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					>,
				},
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Instruction2 {
				#[codec(index = 0)]
				WithdrawAsset(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 1)]
				ReserveAssetDeposited(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 2)]
				ReceiveTeleportedAsset(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 3)]
				QueryResponse {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					response: runtime_types::xcm::v3::Response,
					max_weight: runtime_types::sp_weights::weight_v2::Weight,
					querier: ::core::option::Option<
						runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					>,
				},
				#[codec(index = 4)]
				TransferAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssets,
					beneficiary: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 5)]
				TransferReserveAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssets,
					dest: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					xcm: runtime_types::xcm::v3::Xcm,
				},
				#[codec(index = 6)]
				Transact {
					origin_kind: runtime_types::xcm::v3::OriginKind,
					require_weight_at_most: runtime_types::sp_weights::weight_v2::Weight,
					call: runtime_types::xcm::double_encoded::DoubleEncoded2,
				},
				#[codec(index = 7)]
				HrmpNewChannelOpenRequest {
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					max_message_size: ::core::primitive::u32,
					#[codec(compact)]
					max_capacity: ::core::primitive::u32,
				},
				#[codec(index = 8)]
				HrmpChannelAccepted {
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 9)]
				HrmpChannelClosing {
					#[codec(compact)]
					initiator: ::core::primitive::u32,
					#[codec(compact)]
					sender: ::core::primitive::u32,
					#[codec(compact)]
					recipient: ::core::primitive::u32,
				},
				#[codec(index = 10)]
				ClearOrigin,
				#[codec(index = 11)]
				DescendOrigin(runtime_types::xcm::v3::junctions::Junctions),
				#[codec(index = 12)]
				ReportError(runtime_types::xcm::v3::QueryResponseInfo),
				#[codec(index = 13)]
				DepositAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					beneficiary: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 14)]
				DepositReserveAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					dest: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					xcm: runtime_types::xcm::v3::Xcm,
				},
				#[codec(index = 15)]
				ExchangeAsset {
					give: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					want: runtime_types::xcm::v3::multiasset::MultiAssets,
					maximal: ::core::primitive::bool,
				},
				#[codec(index = 16)]
				InitiateReserveWithdraw {
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					reserve: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					xcm: runtime_types::xcm::v3::Xcm,
				},
				#[codec(index = 17)]
				InitiateTeleport {
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
					dest: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					xcm: runtime_types::xcm::v3::Xcm,
				},
				#[codec(index = 18)]
				ReportHolding {
					response_info: runtime_types::xcm::v3::QueryResponseInfo,
					assets: runtime_types::xcm::v3::multiasset::MultiAssetFilter,
				},
				#[codec(index = 19)]
				BuyExecution {
					fees: runtime_types::xcm::v3::multiasset::MultiAsset,
					weight_limit: runtime_types::xcm::v3::WeightLimit,
				},
				#[codec(index = 20)]
				RefundSurplus,
				#[codec(index = 21)]
				SetErrorHandler(::core::primitive::bool),
				#[codec(index = 22)]
				SetAppendix(::core::primitive::bool),
				#[codec(index = 23)]
				ClearError,
				#[codec(index = 24)]
				ClaimAsset {
					assets: runtime_types::xcm::v3::multiasset::MultiAssets,
					ticket: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 25)]
				Trap(#[codec(compact)] ::core::primitive::u64),
				#[codec(index = 26)]
				SubscribeVersion {
					#[codec(compact)]
					query_id: ::core::primitive::u64,
					max_response_weight: runtime_types::sp_weights::weight_v2::Weight,
				},
				#[codec(index = 27)]
				UnsubscribeVersion,
				#[codec(index = 28)]
				BurnAsset(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 29)]
				ExpectAsset(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 30)]
				ExpectOrigin(
					::core::option::Option<
						runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					>,
				),
				#[codec(index = 31)]
				ExpectError(
					::core::option::Option<(
						::core::primitive::u32,
						runtime_types::xcm::v3::traits::Error,
					)>,
				),
				#[codec(index = 32)]
				ExpectTransactStatus(runtime_types::xcm::v3::MaybeErrorCode),
				#[codec(index = 33)]
				QueryPallet {
					module_name: sp_std::vec::Vec<::core::primitive::u8>,
					response_info: runtime_types::xcm::v3::QueryResponseInfo,
				},
				#[codec(index = 34)]
				ExpectPallet {
					#[codec(compact)]
					index: ::core::primitive::u32,
					name: sp_std::vec::Vec<::core::primitive::u8>,
					module_name: sp_std::vec::Vec<::core::primitive::u8>,
					#[codec(compact)]
					crate_major: ::core::primitive::u32,
					#[codec(compact)]
					min_crate_minor: ::core::primitive::u32,
				},
				#[codec(index = 35)]
				ReportTransactStatus(runtime_types::xcm::v3::QueryResponseInfo),
				#[codec(index = 36)]
				ClearTransactStatus,
				#[codec(index = 37)]
				UniversalOrigin(runtime_types::xcm::v3::junction::Junction),
				#[codec(index = 38)]
				ExportMessage {
					network: runtime_types::xcm::v3::junction::NetworkId,
					destination: runtime_types::xcm::v3::junctions::Junctions,
					xcm: runtime_types::xcm::v3::Xcm,
				},
				#[codec(index = 39)]
				LockAsset {
					asset: runtime_types::xcm::v3::multiasset::MultiAsset,
					unlocker: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 40)]
				UnlockAsset {
					asset: runtime_types::xcm::v3::multiasset::MultiAsset,
					target: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 41)]
				NoteUnlockable {
					asset: runtime_types::xcm::v3::multiasset::MultiAsset,
					owner: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 42)]
				RequestUnlock {
					asset: runtime_types::xcm::v3::multiasset::MultiAsset,
					locker: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				},
				#[codec(index = 43)]
				SetFeesMode { jit_withdraw: ::core::primitive::bool },
				#[codec(index = 44)]
				SetTopic([::core::primitive::u8; 32usize]),
				#[codec(index = 45)]
				ClearTopic,
				#[codec(index = 46)]
				AliasOrigin(runtime_types::staging_xcm::v3::multilocation::MultiLocation),
				#[codec(index = 47)]
				UnpaidExecution {
					weight_limit: runtime_types::xcm::v3::WeightLimit,
					check_origin: ::core::option::Option<
						runtime_types::staging_xcm::v3::multilocation::MultiLocation,
					>,
				},
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum MaybeErrorCode {
				#[codec(index = 0)]
				Success,
				#[codec(index = 1)]
				Error(
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
				),
				#[codec(index = 2)]
				TruncatedError(
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						::core::primitive::u8,
					>,
				),
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum OriginKind {
				#[codec(index = 0)]
				Native,
				#[codec(index = 1)]
				SovereignAccount,
				#[codec(index = 2)]
				Superuser,
				#[codec(index = 3)]
				Xcm,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct PalletInfo {
				#[codec(compact)]
				pub index: ::core::primitive::u32,
				pub name: runtime_types::bounded_collections::bounded_vec::BoundedVec<
					::core::primitive::u8,
				>,
				pub module_name: runtime_types::bounded_collections::bounded_vec::BoundedVec<
					::core::primitive::u8,
				>,
				#[codec(compact)]
				pub major: ::core::primitive::u32,
				#[codec(compact)]
				pub minor: ::core::primitive::u32,
				#[codec(compact)]
				pub patch: ::core::primitive::u32,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct QueryResponseInfo {
				pub destination: runtime_types::staging_xcm::v3::multilocation::MultiLocation,
				#[codec(compact)]
				pub query_id: ::core::primitive::u64,
				pub max_weight: runtime_types::sp_weights::weight_v2::Weight,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Response {
				#[codec(index = 0)]
				Null,
				#[codec(index = 1)]
				Assets(runtime_types::xcm::v3::multiasset::MultiAssets),
				#[codec(index = 2)]
				ExecutionResult(
					::core::option::Option<(
						::core::primitive::u32,
						runtime_types::xcm::v3::traits::Error,
					)>,
				),
				#[codec(index = 3)]
				Version(::core::primitive::u32),
				#[codec(index = 4)]
				PalletsInfo(
					runtime_types::bounded_collections::bounded_vec::BoundedVec<
						runtime_types::xcm::v3::PalletInfo,
					>,
				),
				#[codec(index = 5)]
				DispatchResult(runtime_types::xcm::v3::MaybeErrorCode),
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum WeightLimit {
				#[codec(index = 0)]
				Unlimited,
				#[codec(index = 1)]
				Limited(runtime_types::sp_weights::weight_v2::Weight),
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct Xcm(pub sp_std::vec::Vec<runtime_types::xcm::v3::Instruction>);
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct Xcm2(pub sp_std::vec::Vec<runtime_types::xcm::v3::Instruction2>);
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum VersionedAssetId {
			#[codec(index = 3)]
			V3(runtime_types::xcm::v3::multiasset::AssetId),
			#[codec(index = 4)]
			V4(runtime_types::staging_xcm::v4::asset::AssetId),
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum VersionedAssets {
			#[codec(index = 1)]
			V2(runtime_types::xcm::v2::multiasset::MultiAssets),
			#[codec(index = 3)]
			V3(runtime_types::xcm::v3::multiasset::MultiAssets),
			#[codec(index = 4)]
			V4(runtime_types::staging_xcm::v4::asset::Assets),
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum VersionedLocation {
			#[codec(index = 1)]
			V2(runtime_types::xcm::v2::multilocation::MultiLocation),
			#[codec(index = 3)]
			V3(runtime_types::staging_xcm::v3::multilocation::MultiLocation),
			#[codec(index = 4)]
			V4(runtime_types::staging_xcm::v4::location::Location),
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum VersionedXcm {
			#[codec(index = 2)]
			V2(runtime_types::xcm::v2::Xcm),
			#[codec(index = 3)]
			V3(runtime_types::xcm::v3::Xcm),
			#[codec(index = 4)]
			V4(runtime_types::staging_xcm::v4::Xcm),
		}
		#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
		pub enum VersionedXcm2 {
			#[codec(index = 2)]
			V2(runtime_types::xcm::v2::Xcm2),
			#[codec(index = 3)]
			V3(runtime_types::xcm::v3::Xcm2),
			#[codec(index = 4)]
			V4(runtime_types::staging_xcm::v4::Xcm2),
		}
	}
	pub mod xcm_runtime_apis {
		use super::*;
		pub mod conversions {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Error {
				#[codec(index = 0)]
				Unsupported,
				#[codec(index = 1)]
				VersionedConversionFailed,
			}
		}
		pub mod dry_run {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct CallDryRunEffects<_0> {
				pub execution_result: ::core::result::Result<
					runtime_types::frame_support::dispatch::PostDispatchInfo,
					runtime_types::sp_runtime::DispatchErrorWithPostInfo<
						runtime_types::frame_support::dispatch::PostDispatchInfo,
					>,
				>,
				pub emitted_events: sp_std::vec::Vec<_0>,
				pub local_xcm: ::core::option::Option<runtime_types::xcm::VersionedXcm>,
				pub forwarded_xcms: sp_std::vec::Vec<(
					runtime_types::xcm::VersionedLocation,
					sp_std::vec::Vec<runtime_types::xcm::VersionedXcm>,
				)>,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Error {
				#[codec(index = 0)]
				Unimplemented,
				#[codec(index = 1)]
				VersionedConversionFailed,
			}
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub struct XcmDryRunEffects<_0> {
				pub execution_result: runtime_types::staging_xcm::v4::traits::Outcome,
				pub emitted_events: sp_std::vec::Vec<_0>,
				pub forwarded_xcms: sp_std::vec::Vec<(
					runtime_types::xcm::VersionedLocation,
					sp_std::vec::Vec<runtime_types::xcm::VersionedXcm>,
				)>,
			}
		}
		pub mod fees {
			use super::*;
			#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
			pub enum Error {
				#[codec(index = 0)]
				Unimplemented,
				#[codec(index = 1)]
				VersionedConversionFailed,
				#[codec(index = 2)]
				WeightNotComputable,
				#[codec(index = 3)]
				UnhandledXcmVersion,
				#[codec(index = 4)]
				AssetNotFound,
				#[codec(index = 5)]
				Unroutable,
			}
		}
	}
}
