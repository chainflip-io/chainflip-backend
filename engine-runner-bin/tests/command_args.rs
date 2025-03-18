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

use assert_cmd::Command;
use engine_upgrade_utils::NEW_VERSION;

fn assert_command_arg_for_latest_version(arg: &str) {
	Command::cargo_bin("engine-runner")
		.unwrap()
		.arg(arg)
		.assert()
		.success()
		.stdout(predicates::str::contains(format!("chainflip-engine {NEW_VERSION}")));
}

#[test]
fn version_should_return_for_latest_version() {
	assert_command_arg_for_latest_version("--version");
	assert_command_arg_for_latest_version("-V");
}

#[test]
fn help_should_return_for_latest_version() {
	assert_command_arg_for_latest_version("--help");
	assert_command_arg_for_latest_version("-h");
}
