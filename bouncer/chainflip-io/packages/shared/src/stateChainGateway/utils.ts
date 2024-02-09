import { StateChainGateway__factory } from '../abis';
import { getStateChainGatewayContractAddress } from '../contracts';
import type { FundingNetworkOptions } from './index';

export const getStateChainGateway = (networkOpts: FundingNetworkOptions) => {
  const stateChainGatewayContractAddress =
    networkOpts.network === 'localnet'
      ? networkOpts.stateChainGatewayContractAddress
      : getStateChainGatewayContractAddress(networkOpts.network);

  return StateChainGateway__factory.connect(
    stateChainGatewayContractAddress,
    networkOpts.signer,
  );
};
