#!/usr/bin/env -S pnpm tsx

import fs from 'fs';
import toml from '@iarna/toml';
import { compareSemVer } from '../shared/utils';
import { jsonRpc } from '../shared/json_rpc';
import { specVersion } from '../shared/utils/spec_version';
import { globalLogger as logger } from '../shared/utils/logger';

const projectRoot = process.argv[2];
const engineReleaseVersion = process.argv[3];
const network = process.argv[4];

export function tomlVersion(cargoFilePath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    /* eslint-disable @typescript-eslint/no-explicit-any */
    fs.readFile(cargoFilePath, 'utf-8', (err: any, data) => {
      if (err) {
        reject(new Error('Error reading file: ' + err.message));
        return;
      }

      try {
        const cargoToml: any = toml.parse(data);
        resolve(cargoToml.package.version);
      } catch (error: any) {
        reject(new Error('Error parsing TOML: ' + error.message));
      }
    });
  });
}

const versionRegex = /\d+\.\d+\.\d+/;
const releaseVersion = engineReleaseVersion.match(versionRegex)?.[0];
if (!releaseVersion) {
  throw Error('Invalid release version');
}

// Ensure all the versions are the same
const engineTomlVersion = await tomlVersion(`${projectRoot}/engine/Cargo.toml`);
const runtimeTomlVersion = await tomlVersion(`${projectRoot}/state-chain/runtime/Cargo.toml`);
const nodeTomlVersion = await tomlVersion(`${projectRoot}/state-chain/node/Cargo.toml`);
const cliTomlVersion = await tomlVersion(`${projectRoot}/api/bin/chainflip-cli/Cargo.toml`);
const lpApiTomlVersion = await tomlVersion(`${projectRoot}/api/bin/chainflip-lp-api/Cargo.toml`);
const apiLibTomlVersion = await tomlVersion(`${projectRoot}/api/lib/Cargo.toml`);
const runnerTomlVersion = await tomlVersion(`${projectRoot}/engine-runner-bin/Cargo.toml`);
const dylibTomlVersion = await tomlVersion(`${projectRoot}/engine-dylib/Cargo.toml`);
// The engine gets the version from this file
const procMacrosVersion = await tomlVersion(`${projectRoot}/engine-proc-macros/Cargo.toml`);

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
    apiLibTomlVersion === brokerTomlVersion &&
    brokerTomlVersion === runnerTomlVersion &&
    runnerTomlVersion === dylibTomlVersion &&
    dylibTomlVersion === procMacrosVersion
  )
) {
  throw Error('All versions should be the same');
} else if (compareSemVer(engineTomlVersion, releaseVersion) === 'greater') {
  logger.info(
    `Binary versions are correct. Your branch has version ${engineTomlVersion} greater than the current release ${releaseVersion}.`,
  );
} else {
  throw Error(
    `Binary versions are incorrect. The version of your branch (${engineTomlVersion}) should be greater than the current release (${releaseVersion}).)`,
  );
}

let endpoint;
switch (network) {
  case 'mainnet':
  case 'berghain':
    endpoint = 'https://mainnet-archive.chainflip.io:443';
    break;
  case 'perseverance':
    endpoint = 'https://perseverance.chainflip.xyz:443';
    break;
  case 'sisyphos':
    endpoint = 'https://archive.sisyphos.chainflip.io:443';
    break;
  default:
    throw Error('Invalid network');
}

const releaseSpecVersion = Number(
  ((await jsonRpc(logger, 'state_getRuntimeVersion', [], endpoint)) as any).specVersion,
);
logger.info(`Release spec version: ${releaseSpecVersion}`);

const specVersionInSource = specVersion(
  logger,
  `${projectRoot}/state-chain/runtime/src/lib.rs`,
  'read',
);
logger.info(`Spec version in runtime/src/lib.rs: ${specVersionInSource}`);

if (specVersionInSource >= releaseSpecVersion) {
  logger.info(
    `Spec version is correct. Version in TOML is greater than or equal to the release spec version.`,
  );
} else {
  throw Error(
    `Spec version is incorrect. Version in TOML (${specVersionInSource}) should be greater than or equal to the release spec version (${releaseSpecVersion}).`,
  );
}
