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

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBalanceTrace {
	pub operation_identifier: i64,
	pub address: String,
	pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBalance {
	pub timestamp: i64,
	pub block_identifier: BlockIdentifier,
	pub transaction_balance_trace: Vec<TransactionBalanceTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIdentifier {
	pub hash: String,
	pub number: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionBalanceTrace {
	pub transaction_identifier: String,
	pub operation: Vec<BlockBalanceTrace>,
	#[serde(rename = "type")]
	pub type_field: String,
	pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
	pub id: Option<String>,
	pub fee: Option<i64>,
	#[serde(rename = "blockNumber")]
	pub block_number: Option<i64>,
	#[serde(rename = "blockTimeStamp")]
	pub block_time_stamp: Option<i64>,
	#[serde(rename = "contractResult")]
	pub contract_result: Option<Vec<String>>,
	pub contract_address: Option<String>,
	pub receipt: Option<Value>,
	pub log: Option<Vec<Value>>,
	pub result: Option<String>,
	#[serde(rename = "resMessage")]
	pub res_message: Option<String>,
	#[serde(rename = "assetIssueID")]
	pub asset_issue_id: Option<String>,
	pub withdraw_amount: Option<i64>,
	pub unfreeze_amount: Option<i64>,
	pub internal_transactions: Option<Vec<Value>>,
	pub withdraw_expire_amount: Option<i64>,
}
