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
    '2qVz58R5aPmF5Q61VaKXnpWQtngdh4Jgbeko32fEcECu',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    '6fxCujPRyyTPzcZWpkDRhvDC4NXf4GB5tCTbQRDnz2iw',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    '2VUnNMpXzohc4284EyuhT7PEuUdqy9E7AfxAP8nrm9cv',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    '7bNgRtjaTgCiXwEagFv2cndco5aTzW1dEXGTZp9EDHgE',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    'DSSpXCVb6LU4a91TAkqyUHGXB1bspfLUxKzb5VhGUvyf',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    'DEwmxJ6xUTXVMnZjvSsRBb9knA1JG3ETT1CQXz5q3yzY',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    'ERJZ2GKvYB2f7MroWy8qEA3xdvMNHg2DUqjBJ6KVzXYE',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    'Ecqa1ZZwjS6Lz74k2kXvVKcrWXJNiVGzLqfe1ftBcCYj',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    'HuksgAnauQ9wTextjMhHVB6oVSCT3GKGb6j1DniSS8eL',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    '2TR5QRLhPzPzB6Gvs4iZsq6Dp8v5w2LamrE9BFrsNkzW',
  );
}

await runWithTimeoutAndExit(main(), 60);
