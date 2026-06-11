use cf_utilities::migrations::{basics::HasVersion, v0201, v0202, Migrations};

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
	type MigrationTo0201 = sol::_AssetMap::MigrateFields<sol::_AssetMap::field::usdt::Added>;
}

impl<T: Migrations> Migrations for arb::AssetMap<T>
where
	<T as HasVersion<v0201>>::HistoricalType: Default,
{
	type DefaultMigration = arb::_AssetMap::MigrateFields;
	type MigrationTo0201 = arb::_AssetMap::MigrateFields<arb::_AssetMap::field::usdt::Added>;
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
	type MigrationTo0201 = eth::_AssetMap::MigrateFields<eth::_AssetMap::field::wbtc::Added>;
}

impl<T: Migrations + Default> Migrations for any::AssetMap<T>
where
	<T as HasVersion<v0201>>::HistoricalType: Default,
	<T as HasVersion<v0202>>::HistoricalType: Default,
{
	type DefaultMigration = any::_AssetMap::MigrateFields;
	type MigrationTo0202 = any::_AssetMap::MigrateFields<any::_AssetMap::field::tron::Added>;
}
