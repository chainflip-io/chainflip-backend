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

use chainflip_engine::settings_and_run_main;

use engine_upgrade_utils::{CStrArray, ExitStatus};

#[engine_proc_macros::cfe_entrypoint]
fn cfe_entrypoint(c_args: CStrArray, start_from: u32) -> ExitStatus {
	settings_and_run_main(c_args.to_rust_strings(), start_from)
}
