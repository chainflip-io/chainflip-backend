use pallet_cf_elections::generic_tools::CommonTraits;

use crate::{generic_item, runtime_apis::transparent_rpc_generator::type_variants::{AtRpc, AtRuntime, HasVariant, TypedMigration}};
use crate::runtime_apis::transparent_rpc_generator::VariantName;
use sp_std::vec::Vec;

pub trait PrimitiveTypes: 'static {
    type AssetAmount: CommonTraits;
    type BtcAddress: CommonTraits;
    type AccountId: CommonTraits;
}

impl PrimitiveTypes for AtRuntime {
	type AssetAmount = cf_primitives::AssetAmount;
	type BtcAddress = u8;
	type AccountId = u32;
}


// --------- defining types for the rpc layer ---------

macro_rules! define_rpc_runtime_type {
    (
        struct $name:ident<$T:ident: PrimitiveTypes> {
            $(
                $field:ident: $field_ty:ty,
            )*
        }
    ) => {
        generic_item! {
            mod $name {
                $(
                    $field,
                )*
            }
        }

        impl<$T: PrimitiveTypes> $name::Types for $T {
            $(
                type $field = $field_ty;
            )*
        }

        impl<T: PrimitiveTypes + VariantName> HasVariant<T> for $name::Struct<AtRuntime> {
            type Get = $name::Struct<T>;
        }
    };
}

// --------- the actual types ---------

define_rpc_runtime_type! {
    struct BrokerInfo<T: PrimitiveTypes> {
	    earned_fees: Vec<(cf_primitives::Asset, T::AssetAmount)>,
	    btc_vault_deposit_address: Option<T::BtcAddress>,
	    affiliates: Vec<(sp_runtime::AccountId32, T::AccountId)>,
	    bond: (u8, u8),
	    bound_fee_withdrawal_address: Option<u16>,
    }
}

// generic_item! {
// 	mod BrokerInfo {
// 		earned_fees,
// 		btc_vault_deposit_address,
// 		affiliates,
// 		bond,
// 		bound_fee_withdrawal_address,
// 	}
// }

// impl<T: PrimitiveTypes> BrokerInfo::Types for T {
// 	type earned_fees = Vec<(cf_primitives::Asset, T::AssetAmount)>;
// 	type btc_vault_deposit_address = Option<T::BtcAddress>;
// 	type affiliates = Vec<(sp_runtime::AccountId32, T::AccountId)>;
// 	type bond = (u8, u8);
// 	type bound_fee_withdrawal_address = Option<u16>;
// }

// impl HasVariant<AtRpcLayer> for BrokerInfo::Struct<AtRuntimeLayer> {
//     type Get = BrokerInfo::Struct<AtRpcLayer>;
// }



