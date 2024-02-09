import { ChainflipNetwork, Chains, isTestnet } from '@/shared/enums';
import { ChainData } from './types';

export const ethereum: (network: ChainflipNetwork) => ChainData = (
  network,
) => ({
  id: Chains.Ethereum,
  name: 'Ethereum',
  isMainnet: !isTestnet(network),
});

export const polkadot: (network: ChainflipNetwork) => ChainData = (
  network,
) => ({
  id: Chains.Polkadot,
  name: 'Polkadot',
  isMainnet: !isTestnet(network),
});

export const bitcoin: (network: ChainflipNetwork) => ChainData = (network) => ({
  id: Chains.Bitcoin,
  name: 'Bitcoin',
  isMainnet: !isTestnet(network),
});
