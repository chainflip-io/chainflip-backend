import { Signer } from 'ethers';
import { ChainflipNetwork } from '../enums';

export { default as executeSwap } from './executeSwap';
export type { ExecuteSwapParams } from './schemas';
export * from './approval';

export type SwapNetworkOptions =
  | { network: ChainflipNetwork; signer: Signer }
  | {
      network: 'localnet';
      signer: Signer;
      vaultContractAddress: string;
      srcTokenContractAddress: string;
    };
