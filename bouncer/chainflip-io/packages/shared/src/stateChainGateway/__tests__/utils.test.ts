import { VoidSigner, getDefaultProvider } from 'ethers';
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
        target: ADDRESSES[network].STATE_CHAIN_GATEWAY_ADDRESS,
      });
    },
  );

  it('uses the address for localnets', () => {
    const address = '0x1234';
    expect(
      getStateChainGateway({
        network: 'localnet',
        signer: new VoidSigner('0x0').connect(getDefaultProvider('goerli')),
        stateChainGatewayContractAddress: address,
        flipContractAddress: '0x0000',
      }),
    ).toMatchObject({
      target: address,
    });
  });
});
