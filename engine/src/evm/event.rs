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

use anyhow::{anyhow, Result};
use derive_where::derive_where;
use ethers::abi::RawLog;

use std::{fmt::Debug, sync::Arc};
// use web3::types::{Log, H256, U256};
use ethers::types::Log;
use sp_core::{H160, H256, U256};

/// Type for storing common (i.e. tx_hash) and specific event information
#[derive(Debug, PartialEq, Eq)]
pub struct Event<EventParameters: Debug> {
	/// The transaction hash of the transaction that emitted this event
	pub tx_hash: H256,
	/// The index number of this particular log, in the list of logs emitted by the tx_hash
	pub log_index: U256,
	/// The event specific parameters
	pub event_parameters: EventParameters,
}

impl<EventParameters: Debug> std::fmt::Display for Event<EventParameters> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "EventParameters: {:?}; tx_hash: {:#x}", self.event_parameters, self.tx_hash)
	}
}

impl<EventParameters: Debug + ethers::contract::EthLogDecode> Event<EventParameters> {
	pub fn new_from_unparsed_logs(log: Log) -> Result<Self> {
		Ok(Self {
			tx_hash: log
				.transaction_hash
				.ok_or_else(|| anyhow!("Could not get transaction hash from ETH log"))?,
			log_index: log
				.log_index
				.ok_or_else(|| anyhow!("Could not get log index from ETH log"))?,
			event_parameters: EventParameters::decode_log(&RawLog {
				topics: log.topics.into_iter().collect(),
				data: log.data.to_vec(),
			})?,
		})
	}
}

pub trait EvmEventType<Data: std::fmt::Debug>: Sync + Send {
	fn parse_log(&self, log: Log) -> Result<Event<Data>>;
}

#[derive_where(Default; )]
pub struct EvmEventTypeCarrier<Event, TargetData> {
	_phantom: std::marker::PhantomData<(Event, TargetData)>,
}

pub fn evm_event_type<
	ParseData: ethers::contract::EthLogDecode + std::fmt::Debug + Into<TargetData> + 'static,
	TargetData: std::fmt::Debug + Sync + Send + 'static,
>() -> Arc<dyn EvmEventType<TargetData>> {
	let event_carrier: EvmEventTypeCarrier<ParseData, TargetData> = Default::default();
	Arc::new(event_carrier)
}

impl<
		ParseData: ethers::contract::EthLogDecode + std::fmt::Debug + Into<TargetData>,
		TargetData: std::fmt::Debug + Sync + Send,
	> EvmEventType<TargetData> for EvmEventTypeCarrier<ParseData, TargetData>
{
	fn parse_log(&self, log: Log) -> Result<Event<TargetData>> {
		let Event { tx_hash, log_index, event_parameters } =
			Event::<ParseData>::new_from_unparsed_logs(log)?;
		Ok(Event { tx_hash, log_index, event_parameters: event_parameters.into() })
	}
}

#[derive_where(Clone;)]
pub struct EvmEventSource<EventData> {
	pub contract_address: H160,
	pub event_type: Arc<dyn EvmEventType<EventData>>,
}

impl<TargetData: std::fmt::Debug + Sync + Send + 'static> EvmEventSource<TargetData> {
	pub fn new<
		ParseData: ethers::contract::EthLogDecode + std::fmt::Debug + Into<TargetData> + 'static,
	>(
		contract_address: H160,
	) -> Self {
		EvmEventSource { contract_address, event_type: evm_event_type::<ParseData, TargetData>() }
	}
}
