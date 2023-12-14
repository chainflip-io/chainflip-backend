#!/usr/bin/env -S pnpm tsx

import fs from 'fs';
import toml from '@iarna/toml';
import { compareSemVer } from '../shared/utils';

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
const brokerTomlVersion = await tomlVersion(
  `${projectRoot}/api/bin/chainflip-broker-api/Cargo.toml`,
);

if (
  !(
    engineTomlVersion === runtimeTomlVersion &&
    runtimeTomlVersion === nodeTomlVersion &&
    nodeTomlVersion === cliTomlVersion &&
    cliTomlVersion === lpApiTomlVersion &&
    lpApiTomlVersion === brokerTomlVersion
  )
) {
  throw Error('All versions should be the same');
} else if (compareSemVer(engineTomlVersion, releaseVersion) === 'greater') {
  console.log(`Version is correct. Your branch has a version greater than the current release.`);
} else {
  throw Error(
    `Version is incorrect. The version of your branch (${engineTomlVersion}) should be greater than the current release (${releaseVersion}).)`,
  );
}
