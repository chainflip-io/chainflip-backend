import { VoidSigner, ethers } from 'ethers';
import { ADDRESSES } from '../../consts';
import { getStateChainGateway } from '../utils';

describe(getStateChainGateway, () => {
  it.each(['sisyphos'] as const)(
    'returns the correct gateway for %s',
    (network) => {
      expect(
        getStateChainGateway({
          network,
          signer: new VoidSigner('0x0'),
        }),
      ).toMatchObject({
        address: ADDRESSES[network].STATE_CHAIN_MANAGER_CONTRACT_ADDRESS,
      });
    },
  );

  it('uses the address for localnets', () => {
    const address = '0x1234';
    expect(
      getStateChainGateway({
        network: 'localnet',
        signer: new VoidSigner('0x0').connect(
          ethers.providers.getDefaultProvider('goerli'),
        ),
        stateChainGatewayContractAddress: address,
      }),
    ).toMatchObject({
      address,
    });
  });
});
