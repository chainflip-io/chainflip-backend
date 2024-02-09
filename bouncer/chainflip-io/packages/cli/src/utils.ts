import { createInterface } from 'node:readline/promises';
import { ChainflipNetworks } from '@/shared/enums';
import { chainflipNetwork } from '@/shared/parsers';
import { ChainflipNetwork } from './enums';

export const askForPrivateKey = async () => {
  const rl = createInterface({ input: process.stdin, output: process.stdout });

  try {
    return await rl.question("Please enter your wallet's private key: ");
  } finally {
    rl.close();
  }
};

type GetEthNetworkOptions =
  | { chainflipNetwork: 'localnet'; ethNetwork?: string }
  | { chainflipNetwork: ChainflipNetwork };

export function getEthNetwork(opts: GetEthNetworkOptions) {
  if (opts.chainflipNetwork === 'localnet') return opts.ethNetwork;
  if (opts.chainflipNetwork === ChainflipNetworks.mainnet) return 'mainnet';
  return 'goerli';
}

export const cliNetworks = [
  ...Object.values(chainflipNetwork.enum),
  'localnet',
] as const;
