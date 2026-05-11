use cf_utilities::migrations::{
	basics::{HasVersion, NewFieldWithDefault},
	registry::{FeatureNewAssetsIn0201, FeatureTronIn0202},
	v0201, v0202, Migrations,
};

use super::assets::*;

// -------------- migrations ---------------- //

impl<T: Migrations> Migrations for hub::AssetMap<T> {
	type DefaultMigration = hub::_AssetMap::MigrateFields;
}

impl<T: Migrations> Migrations for sol::AssetMap<T>
where
	<T as HasVersion<v0201>>::HistoricalType: Default,
{
	type DefaultMigration = sol::_AssetMap::MigrateFields;
	type MigrationTo0201 = sol::_AssetMap::MigrateFields<FeatureNewAssetsIn0201>;
}

impl<T: Migrations> Migrations for arb::AssetMap<T>
where
	<T as HasVersion<v0201>>::HistoricalType: Default,
{
	type DefaultMigration = arb::_AssetMap::MigrateFields;
	type MigrationTo0201 = arb::_AssetMap::MigrateFields<FeatureNewAssetsIn0201>;
}

impl<T: Migrations> Migrations for btc::AssetMap<T> {
	type DefaultMigration = btc::_AssetMap::MigrateFields;
}

impl<T: Migrations> Migrations for dot::AssetMap<T> {
	type DefaultMigration = dot::_AssetMap::MigrateFields;
}

impl<T: Migrations> Migrations for tron::AssetMap<T> {
	type DefaultMigration = tron::_AssetMap::MigrateFields;
}

impl<T: Migrations> Migrations for eth::AssetMap<T>
where
	<T as HasVersion<v0201>>::HistoricalType: Default,
{
	type DefaultMigration = eth::_AssetMap::MigrateFields;
	type MigrationTo0201 = eth::_AssetMap::MigrateFields<FeatureNewAssetsIn0201>;
}

impl<T: Migrations + Default> Migrations for any::AssetMap<T>
where
	<T as HasVersion<v0201>>::HistoricalType: Default,
	<T as HasVersion<v0202>>::HistoricalType: Default,
{
	type DefaultMigration = any::_AssetMap::MigrateFields;
	type MigrationTo0202 = any::_AssetMap::MigrateFields<FeatureTronIn0202>;
}

// -------------- custom migration details ---------------- //

// to 0201

impl<TargetFieldsTypes: arb::_AssetMap::HistoricalTypesAt<v0201, usdt: Default>>
	arb::_AssetMap::CustomMigration<TargetFieldsTypes, v0201> for FeatureNewAssetsIn0201
{
	type usdt = NewFieldWithDefault;
}

impl<TargetFieldTypes: eth::_AssetMap::HistoricalTypesAt<v0201, wbtc: Default>>
	eth::_AssetMap::CustomMigration<TargetFieldTypes, v0201> for FeatureNewAssetsIn0201
{
	type wbtc = NewFieldWithDefault;
}

impl<TargetFieldsTypes: sol::_AssetMap::HistoricalTypesAt<v0201, usdt: Default>>
	sol::_AssetMap::CustomMigration<TargetFieldsTypes, v0201> for FeatureNewAssetsIn0201
{
	type usdt = NewFieldWithDefault;
}

// to 0202

impl<TargetFieldsTypes: any::_AssetMap::HistoricalTypesAt<v0202, tron: Default>>
	any::_AssetMap::CustomMigration<TargetFieldsTypes, v0202> for FeatureTronIn0202
{
	type tron = NewFieldWithDefault;
}
