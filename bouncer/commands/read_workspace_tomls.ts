#!/usr/bin/env -S pnpm tsx

import fs from 'fs';
import toml from '@iarna/toml';
import { compareSemVer } from '../shared/utils';
import { jsonRpc } from '../shared/json_rpc';
import { specVersion } from '../shared/utils/spec_version';

const projectRoot = process.argv[2];
const engineReleaseVersion = process.argv[3];

export function tomlVersion(cargoFilePath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    fs.readFile(cargoFilePath, 'utf-8', (err, data) => {
      if (err) {
        reject(new Error('Error reading file: ' + err.message));
        return;
      }

      try {
        const cargoToml = toml.parse(data);
        resolve(cargoToml.package.version);
      } catch (error) {
        reject(new Error('Error parsing TOML: ' + error.message));
      }
    });
  });
}

const versionRegex = /\d+\.\d+\.\d+/;
const releaseVersion = engineReleaseVersion.match(versionRegex)[0];

// Ensure all the versions are the same
const engineTomlVersion = await tomlVersion(`${projectRoot}/engine/Cargo.toml`);
const runtimeTomlVersion = await tomlVersion(`${projectRoot}/state-chain/runtime/Cargo.toml`);
const nodeTomlVersion = await tomlVersion(`${projectRoot}/state-chain/node/Cargo.toml`);
const cliTomlVersion = await tomlVersion(`${projectRoot}/api/bin/chainflip-cli/Cargo.toml`);
const lpApiTomlVersion = await tomlVersion(`${projectRoot}/api/bin/chainflip-lp-api/Cargo.toml`);
const apiLibTomlVersion = await tomlVersion(`${projectRoot}/api/lib/Cargo.toml`);
const brokerTomlVersion = await tomlVersion(
  `${projectRoot}/api/bin/chainflip-broker-api/Cargo.toml`,
);

if (
  !(
    engineTomlVersion === runtimeTomlVersion &&
    runtimeTomlVersion === nodeTomlVersion &&
    nodeTomlVersion === cliTomlVersion &&
    cliTomlVersion === lpApiTomlVersion &&
    lpApiTomlVersion === brokerTomlVersion &&
    apiLibTomlVersion === brokerTomlVersion
  )
) {
  throw Error('All versions should be the same');
} else if (compareSemVer(engineTomlVersion, releaseVersion) === 'greater') {
  console.log(
    `Binary versions are correct. Your branch has a version greater than the current release.`,
  );
} else {
  throw Error(
    `Binary versions are incorrect. The version of your branch (${engineTomlVersion}) should be greater than the current release (${releaseVersion}).)`,
  );
}

const releaseSpecVersion = Number(
  (await jsonRpc('state_getRuntimeVersion', [], 'https://perseverance.chainflip.xyz:443'))
    .specVersion,
);
console.log(`Release spec version: ${releaseSpecVersion}`);

const specVersionInToml = specVersion(`${projectRoot}/state-chain/runtime/src/lib.rs`, 'read');
console.log(`Spec version in TOML: ${specVersionInToml}`);

if (specVersionInToml >= releaseSpecVersion) {
  console.log(
    `Spec version is correct. Version in TOML is greater than or equal to the release spec version.`,
  );
} else {
  throw Error(
    `Spec version is incorrect. Version in TOML (${specVersionInToml}) should be greater than or equal to the release spec version (${releaseSpecVersion}).`,
  );
}
