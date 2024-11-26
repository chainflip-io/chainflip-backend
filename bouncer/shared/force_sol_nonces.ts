#!/usr/bin/env -S pnpm tsx

import { PublicKey } from '@solana/web3.js';
import {
  decodeSolAddress,
  runWithTimeoutAndExit,
} from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function main() {
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw').toBase58()),
      decodeSolAddress(new PublicKey('8T217weMrePR8VqqiY1J6VQKn5GfDXDwTPuYekPffNTo').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo').toBase58()),
      decodeSolAddress(new PublicKey('Hvg5WgDgdhcex1TsJW8PiPqcxUizLitoEmXcCShmXVWJ').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p').toBase58()),
      decodeSolAddress(new PublicKey('8vM4M9MWoYZE7YDGhpUhoetabY8dwaz4AcDR9hbCHd7u').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2').toBase58()),
      decodeSolAddress(new PublicKey('3fWpjCEzbHNU8qQD8YqoE5PfFahHNp4nwVVgJzxwTZya').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM').toBase58()),
      decodeSolAddress(new PublicKey('96CDysvpx87Cd4TnxsMajFA9cKwFid1tMUFWnQWnifpJ').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn').toBase58()),
      decodeSolAddress(new PublicKey('BjY7LRovNVwGEh5BGcfK4bZcjVan4YzuyFqcBwraG9Bj').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa').toBase58()),
      decodeSolAddress(new PublicKey('Hb35byiENfrMFwznb5TUxdAWN52dV81tWYWT3N99VRWr').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna').toBase58()),
      decodeSolAddress(new PublicKey('4TbbvCow8yHxnzdMT22gUt3JvHwAF8dbscBCLRezmpCY').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55').toBase58()),
      decodeSolAddress(new PublicKey('FooctpZoHqoSjDE983JTJRyovN5Py6PZiiubd53gLFMv').toBase58()),
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey('AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv').toBase58()),
      decodeSolAddress(new PublicKey('HCNp5KwKadPNiPs3nY1DtVcDfFkuUE2uUBsBueZbnkWc').toBase58()),
    ),
  );
}

await runWithTimeoutAndExit(main(), 60);
