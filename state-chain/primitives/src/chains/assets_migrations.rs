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

use cf_utilities::migrations::{basics::HasVersion, v20100, v20200, HasChangelog};

use super::assets::*;

// -------------- HasChangelog ---------------- //

impl<T: HasChangelog> HasChangelog for hub::AssetMap<T> {
	type if_unspecified = hub::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for sol::AssetMap<T>
where
	<T as HasVersion<v20100>>::HistoricalType: Default,
{
	type if_unspecified = sol::_AssetMap::see_field_changelogs;
	type in_20100 =
		sol::_AssetMap::see_field_changelogs_and_also<sol::_AssetMap::field::usdt::Added>;
}

impl<T: HasChangelog> HasChangelog for arb::AssetMap<T>
where
	<T as HasVersion<v20100>>::HistoricalType: Default,
{
	type if_unspecified = arb::_AssetMap::see_field_changelogs;
	type in_20100 =
		arb::_AssetMap::see_field_changelogs_and_also<arb::_AssetMap::field::usdt::Added>;
}

impl<T: HasChangelog> HasChangelog for btc::AssetMap<T> {
	type if_unspecified = btc::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for dot::AssetMap<T> {
	type if_unspecified = dot::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for tron::AssetMap<T> {
	type if_unspecified = tron::_AssetMap::see_field_changelogs;
}

impl<T: HasChangelog> HasChangelog for eth::AssetMap<T>
where
	<T as HasVersion<v20100>>::HistoricalType: Default,
{
	type if_unspecified = eth::_AssetMap::see_field_changelogs;
	type in_20100 =
		eth::_AssetMap::see_field_changelogs_and_also<eth::_AssetMap::field::wbtc::Added>;
}

impl<T: HasChangelog + Default> HasChangelog for any::AssetMap<T>
where
	<T as HasVersion<v20100>>::HistoricalType: Default,
	<T as HasVersion<v20200>>::HistoricalType: Default,
{
	type if_unspecified = any::_AssetMap::see_field_changelogs;
	type in_20200 =
		any::_AssetMap::see_field_changelogs_and_also<any::_AssetMap::field::tron::Added>;
}
