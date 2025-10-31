// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::*;
use cf_chains::ForeignChainAddress;
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};
use hex_literal::hex;

use pallet_cf_asset_balances::{ExternalOwner, Liabilities};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct RemoveUnusedWallets;

const ADDRESSES_NOT_USED: [[u8; 20]; 118] = [
	hex!("03e5238b9ebaaa526a90f8842a786e63ceb2617d"),
	hex!("09012926102d46c2843e8aaada1a16b2b68c8b6f"),
	hex!("0a291b30b2d3170d75c7fdad4ee10d1cec566ebd"),
	hex!("0c14a3d4113d1aa003c01e7f1c45c445fb52eece"),
	hex!("0ff59e4952654ae9fe24d0da741bec2bb8472ce4"),
	hex!("10f4d150673fb9c447591b3ebc5c6b570ddf5963"),
	hex!("13c4fb213f000251d245505819735b2f70b34acd"),
	hex!("145ffa25dff653e8b8ff708c4f296b55f40e5797"),
	hex!("19519aecf077c67b21a0ba780ec38710d7295d5c"),
	hex!("1967c377640e55d61f401bb761b604440b1e5422"),
	hex!("1b6b066409183d009339f8c7d531662ae6991f3b"),
	hex!("1dc31cd5e75f8a1424474c9f123f494f929ea3e9"),
	hex!("1fcc396c489f44640b3fc88e806d39d97be478ae"),
	hex!("20d6c33746384b763255d0860c585524953ac631"),
	hex!("2134a613fbc05ca708d021b5dbf259f6a7861f98"),
	hex!("24293b944c3128381d6033ab4dd24aaacf0ee4f1"),
	hex!("259e86f90c408b6674340a9d5cdc2fc4f9ff3dae"),
	hex!("26556b1165a0aa51bd42de30a38adb241ffb3f54"),
	hex!("26d80d1ee3360a1ea41e251fbae317146f3ced5a"),
	hex!("276ceeaba29795ff2f3f09f98389cba0aa74927e"),
	hex!("280c19626197516be9e6cdac740abf7d4ed556c8"),
	hex!("2a84b472bb83e3994d87937a806fef8409d876b1"),
	hex!("2c98dcd9b2097b57a2ab5edfd95a40cf9d52d73e"),
	hex!("323d4dc0a137c351217584b8d8b5c26a905c85e5"),
	hex!("32cf00a2dce469baea643162e62d7d753f650906"),
	hex!("33964668bbcc1b8ef847a0f77822349ce5b4e631"),
	hex!("34ed5702367461a1e90ce48267100549d47db8bb"),
	hex!("35ac20795774ed3e1cd94361a7f3af7b28505360"),
	hex!("366b74d4c1104ed7ca35c2b1bca4c1c38d6428cb"),
	hex!("3679043f696721b2a586903bda9bb49c032ab3af"),
	hex!("367c8aadb81b4c30aadca453f7c60731d10e5124"),
	hex!("37396e1ec705cc13e1c92be3741f4f943e1e48d7"),
	hex!("380b25713c21bfada5212a0cc2de3ea1071a9a77"),
	hex!("3ab4fc79afe42edf697ebccf479d47e9f745e4c3"),
	hex!("3cdb212bea0d5854ec0e2558014c3659d3691b90"),
	hex!("3ce8545cdcca123f101f61b654e03edb46ca164b"),
	hex!("3d1135b730e90cb62f9f8802957dc67ef316b217"),
	hex!("3f84764c61089aa1c5a5915f43ca7bbe800dd620"),
	hex!("42849e1b9d3b62cd0a2d11dc42fa785de257cd7d"),
	hex!("42e619122ac3d4bc29089e58321b7ce564400e45"),
	hex!("447a016c2fff5008e9a7daeb2563a2d67da8dac2"),
	hex!("48704213e3bae8f503681b1ae8e70c510c4551dd"),
	hex!("4abfe695128c2b0fc3ad8bc3bc7260d5fbac1bfb"),
	hex!("4b6643ceb8792ee649358dc5e50d5a5e9b450548"),
	hex!("4d318a8b724481b70a2aa5a6153e1b1a3c625374"),
	hex!("5051aa15f3b71929d954916988d525319f5f6d5e"),
	hex!("50a36a81e5d740ae8438cb38cc2558725a3f8f24"),
	hex!("549a11f9d31c23afc0e1588f0620fc958b382736"),
	hex!("5519abec269a63feda7061d9a6cc05e6a62ebd35"),
	hex!("569dc0982ca35d33980fd43c2c3b180bac901f1a"),
	hex!("57d31bdc23cd1716e3620fdbc01b9a1bef493c55"),
	hex!("585f27dd1b6608cdb61ff8b2094dc31b29168f66"),
	hex!("5c6518a441aa2447a871814bb60531e449dfc06d"),
	hex!("5d5e5e5821c3afde8ef407669c797d263a5c6a79"),
	hex!("5e6306cea07775c6455970c4571b8c65f2d1bc71"),
	hex!("60cf87683ea20d56eefeb881721c6645dce6b7bf"),
	hex!("6269f15dad7472a2e8a1f97b42e1fd0db9cd3d40"),
	hex!("691f4fe8dbc3e2b41fb6da9414f18586d4be56bb"),
	hex!("6b01bca62b426565a3c7d100acdd49a5e911e288"),
	hex!("6e95a7d23c438e6d223374b0ab876be1e15671b9"),
	hex!("729ff2e30d1ea35de9f90fc7408bd017c93b2613"),
	hex!("72c470e776c814264ed7d976df9f23112f22f0d9"),
	hex!("741ec03a69bef401716b371a47a58fc471622e5a"),
	hex!("74b5dd3e00b2d7ef9e58b86f6d47051ca0bf6ce6"),
	hex!("764ff5d08cb54434e91173b5201d1717ea3a027c"),
	hex!("798eaa59a7af4b56a47733c3a824e5cbd5a019c3"),
	hex!("79d45d2fcd92f6c36831440143ae73b3f0449bc6"),
	hex!("7e16912d999c872800b15b33a922402738f33f67"),
	hex!("817049b58de1d9c76256fc96aec05c4bcdba19a0"),
	hex!("85671fb129ba91db3969a488ac6a0d27e2324a0f"),
	hex!("871fa74d4a1720ef8854b392bb4216fed34ee38b"),
	hex!("89161b76d4f3f7c9440ce5a806daa12192b0562a"),
	hex!("89771202cc565e32582cc754b4a91ec3d774689f"),
	hex!("8b2af616be0fe5fa1a8edd1e1c3319ad2ec63402"),
	hex!("8c47c00456a4ada51581fb1e9ec70b8c405275ac"),
	hex!("8ca3b96d3601aec3d73ec3eb179319c49453aafc"),
	hex!("8ca97a869a450bd1c822991c9aff866bec0b8ff8"),
	hex!("8ddcc253446dce73324356d4df5816d4cbc5cbc3"),
	hex!("9063e924ea35270eea9531579f43d274c721cdd4"),
	hex!("93e2ced56dea3e0e3836a33d29848dff63d7b021"),
	hex!("93e380f835cb00003b3bc62893bc0c61b25c04b9"),
	hex!("947dbebbe7595dc6e73a22370c0367864763c98e"),
	hex!("967dbbcb8eb7760c2aa9c5b987faebfbc2b2f957"),
	hex!("99ba35a1e9527c70d56bbc2c7650028f1eddb5f8"),
	hex!("9a93b2cfdd227692b2c64c3eeb0c12a5a1433d3e"),
	hex!("9f09b18f6f567313d35adfb8f723d3c3eb766b15"),
	hex!("a02b2e974c781bd69b0e645762cac626887e9eab"),
	hex!("a21afaefe0dc7ab07e522d2c313a4bfd55948643"),
	hex!("a31e9584fb4500c5d17d4d89f977a44bd05cb33b"),
	hex!("a5a875b932830c673e7f20e97443fd7457667ca9"),
	hex!("a68fa6852a0931f554893283adf1de197dd9179c"),
	hex!("aaa608a72b60d170ab2c2e82e7f45f948f889964"),
	hex!("abcb963766d1d1ffb7a2eab4210d72491c50d824"),
	hex!("abd5706c55589018e069c68d621a3caa43770a32"),
	hex!("b00603629b3f3e0881ffd8f6d83c45525408e620"),
	hex!("b2b1c36241a1a68ba9cf70000e402489f5b2278e"),
	hex!("b32c05582619b9e1df2f506fc6f50a33f3dd538a"),
	hex!("b3824b874d8ad2cc707ce5725e521f9dcae36c68"),
	hex!("b7503414d36f568fd92c3fad1ea103a92b6b3c64"),
	hex!("ba3295ce19af0a84d666da885c4d10a584fe9ff1"),
	hex!("bec75b2ce34541009aa1a54578b4dcec32bb8545"),
	hex!("c6dfa06d0ec05a0d6083492255efc81a4ad6b8e8"),
	hex!("d07e1ca2b505f0814ea38c75f103ad48ef41227f"),
	hex!("d3577a74a3ef71c911327ceda15c8ecaaef52d72"),
	hex!("d922618b7549be2d8225dd18a8feb5fc69aaa2cc"),
	hex!("daa5c5a7a31fa67bb7c4f380d29ec32965844a71"),
	hex!("dc0cdb8b584167f4879c07a67b2e2926f121459e"),
	hex!("dd68f9e1778183a56f03c87def509847002f906a"),
	hex!("e08a77664a8985e21f6341346728a8d487260069"),
	hex!("e196914d902aad416e88ee543923ae43e292df9f"),
	hex!("e482771d02ba06948e53a509766666e6dd7e3fab"),
	hex!("e53096d47e282bdc414a61279e80ad0d511696f9"),
	hex!("f1065127fa3f85766ae58cd33813732946a0094d"),
	hex!("f2ad826d282bfe9efb4e8c9792fa838b0bec81e3"),
	hex!("f860705308d79adf3e8116443ad5dd0eccfb3fd6"),
	hex!("f9bd120fec4ea95522e667143eb77189a89c526a"),
	hex!("fc8789df3174ffd9573e5fe6abbd44c44f991512"),
	hex!("fc95e676e208733aeab2ba4b0e9bd8aea1af42d4"),
];

impl OnRuntimeUpgrade for RemoveUnusedWallets {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let no_of_items_pre_upgrade: u64 =
			Liabilities::<Runtime>::get(Asset::Eth).len().try_into().unwrap();
		Ok(no_of_items_pre_upgrade.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let mut eth_wallets = Liabilities::<Runtime>::take(Asset::Eth);
		for wallet in ADDRESSES_NOT_USED {
			eth_wallets.remove(&ExternalOwner::Account(ForeignChainAddress::Eth(wallet.into())));
		}
		Liabilities::<Runtime>::insert(Asset::Eth, eth_wallets);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use core::assert;

		let no_of_items_pre_upgrade: u64 = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		assert!(
			no_of_items_pre_upgrade - 118u64 ==
				<usize as TryInto<u64>>::try_into(Liabilities::<Runtime>::get(Asset::Eth).len())
					.unwrap()
		);

		Ok(())
	}
}
