#![feature(os_str_display)]
#![feature(trait_alias)]
#![feature(btree_extract_if)]
#![feature(never_type)]
#![feature(iter_intersperse)]

mod diff;
mod typediff;
mod write_migration;

use crate::typediff::compare_metadata;

#[tokio::main]
async fn main() {
	compare_metadata().await
}
