#!/usr/bin/env -S pnpm tsx

import { PublicKey } from '@solana/web3.js';
import { decodeSolAddress, runWithTimeoutAndExit } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function forceRecoverSolNonce(nonceAddress: string, nonceValue: string) {
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey(nonceAddress).toBase58()),
      decodeSolAddress(new PublicKey(nonceValue).toBase58()),
    ),
  );
}

async function main() {
  await forceRecoverSolNonce(
    '2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw',
    '8T217weMrePR8VqqiY1J6VQKn5GfDXDwTPuYekPffNTo',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    'Hvg5WgDgdhcex1TsJW8PiPqcxUizLitoEmXcCShmXVWJ',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    '8vM4M9MWoYZE7YDGhpUhoetabY8dwaz4AcDR9hbCHd7u',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    '3fWpjCEzbHNU8qQD8YqoE5PfFahHNp4nwVVgJzxwTZya',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    '96CDysvpx87Cd4TnxsMajFA9cKwFid1tMUFWnQWnifpJ',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    'BjY7LRovNVwGEh5BGcfK4bZcjVan4YzuyFqcBwraG9Bj',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    'Hb35byiENfrMFwznb5TUxdAWN52dV81tWYWT3N99VRWr',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    '4TbbvCow8yHxnzdMT22gUt3JvHwAF8dbscBCLRezmpCY',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    'FooctpZoHqoSjDE983JTJRyovN5Py6PZiiubd53gLFMv',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    'HCNp5KwKadPNiPs3nY1DtVcDfFkuUE2uUBsBueZbnkWc',
  );
}

await runWithTimeoutAndExit(main(), 60);
