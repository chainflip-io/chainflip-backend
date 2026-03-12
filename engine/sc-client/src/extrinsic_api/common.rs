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

use sp_runtime::transaction_validity::InvalidTransaction;
use tokio::sync::{mpsc, oneshot};

pub(super) async fn send_request<Request, F: FnOnce(oneshot::Sender<Result>) -> Request, Result>(
	request_sender: &mpsc::Sender<Request>,
	into_request: F,
) -> oneshot::Receiver<Result> {
	let (result_sender, result_receiver) = oneshot::channel();
	// Must drop this _result before await'ing on result_receiver, as in error case it contains the
	// result_sender
	let _result = request_sender.send(into_request(result_sender)).await;
	result_receiver
}

pub(super) fn invalid_err_obj(
	invalid_reason: InvalidTransaction,
) -> jsonrpsee::types::ErrorObjectOwned {
	jsonrpsee::types::ErrorObject::owned(
		1010,
		"Invalid Transaction",
		Some(<&'static str>::from(invalid_reason)),
	)
}

// error codes declared https://github.com/paritytech/polkadot-sdk/blob/master/substrate/client/rpc-api/src/author/error.rs
// not exported
/// The transaction is temporarily banned.
pub const POOL_TEMPORARILY_BANNED: i32 = 1012;
/// The transaction is already in the pool
pub const POOL_ALREADY_IMPORTED: i32 = 1013;
/// Transaction has too low priority to replace existing one in the pool.
pub const POOL_TOO_LOW_PRIORITY: i32 = 1014;
