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

use std::marker::PhantomData;

use cf_chains::ChainCrypto;
use cf_primitives::AccountId;
use multisig::p2p::{OutgoingMultisigStageMessages, VersionedCeremonyMessage};
use sp_core::ed25519;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::core::{ed25519_secret_key_to_x25519_secret_key, X25519KeyPair};

pub mod core;
pub mod muxer;

type EdPublicKey = ed25519::Public;
type XPublicKey = x25519_dalek::PublicKey;

pub fn pk_to_string(pk: &XPublicKey) -> String {
	hex::encode(pk.as_bytes())
}

pub struct P2PKey {
	pub signing_key: ed25519_dalek::SigningKey,
	pub encryption_key: X25519KeyPair,
}

impl P2PKey {
	pub fn new(ed25519_secret_key: &ed25519_dalek::SecretKey) -> Self {
		let x_secret_key = ed25519_secret_key_to_x25519_secret_key(ed25519_secret_key);
		P2PKey {
			signing_key: ed25519_dalek::SigningKey::from_bytes(ed25519_secret_key),
			encryption_key: X25519KeyPair {
				public_key: (&x_secret_key).into(),
				secret_key: x_secret_key,
			},
		}
	}
}

pub struct MultisigMessageSender<C: ChainCrypto>(
	pub UnboundedSender<OutgoingMultisigStageMessages>,
	PhantomData<C>,
);

impl<C: ChainCrypto> MultisigMessageSender<C> {
	pub fn new(sender: UnboundedSender<OutgoingMultisigStageMessages>) -> Self {
		MultisigMessageSender(sender, PhantomData)
	}
}
pub struct MultisigMessageReceiver<C: ChainCrypto>(
	pub UnboundedReceiver<(AccountId, VersionedCeremonyMessage)>,
	PhantomData<C>,
);

impl<C: ChainCrypto> MultisigMessageReceiver<C> {
	pub fn new(receiver: UnboundedReceiver<(AccountId, VersionedCeremonyMessage)>) -> Self {
		MultisigMessageReceiver(receiver, PhantomData)
	}
}
