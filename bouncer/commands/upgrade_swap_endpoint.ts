#!/usr/bin/env -S pnpm tsx

import { Connection, PublicKey } from '@solana/web3.js';
import { getSolConnection, runWithTimeoutAndExit, sleep } from 'shared/utils';
import { upgradeSwapEndpoint } from 'shared/initialize_new_chains';
import { globalLogger } from 'shared/utils/logger';

async function getProgramData(connection: Connection, programDataAccount: PublicKey) {
  const accountInfo = await connection.getAccountInfo(programDataAccount);
  console.log(accountInfo?.data.length);
  if (!accountInfo) {
    throw new Error('Swap Endpoint program should be deployed');
  }
  return accountInfo.data.slice(-50);
}

async function main() {
  const connection = getSolConnection();
  const upgradeSwapEndpointDataAccount = new PublicKey(
    'ErjwBtUxDrpewSnX1JPRh7FeHhNsaaukXKMu7FjsZxHG',
  );
  const initialProgramData = await getProgramData(connection, upgradeSwapEndpointDataAccount);

  await upgradeSwapEndpoint(globalLogger);

  // Wait for the upgrade to be executed so the bouncer after upgrade doesnt fail
  for (let i = 0; i < 40; i++) {
    const programLastBytes = await getProgramData(connection, upgradeSwapEndpointDataAccount);
    const uint8Array = new Uint8Array(
      programLastBytes.buffer,
      programLastBytes.byteOffset,
      programLastBytes.byteLength,
    );
    // Using last bytes to check if the program has been upgraded.
    if (
      uint8Array.some((byte) => byte !== 0) &&
      initialProgramData.toString() !== programLastBytes.toString()
    ) {
      return;
    }
    await sleep(3000);
  }
}

await runWithTimeoutAndExit(main(), 120);
