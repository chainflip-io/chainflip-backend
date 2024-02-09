/* eslint-disable @typescript-eslint/no-empty-function */
/* eslint-disable @typescript-eslint/lines-between-class-members */
/* eslint-disable max-classes-per-file */
import { VoidSigner, ethers } from 'ethers';
import { checkAllowance } from '../../contracts';
import {
  executeRedemption,
  fundStateChainAccount,
  getMinimumFunding,
  getRedemptionDelay,
} from '../index';

class MockGateway {
  constructor(readonly address: string) {}
  async fundStateChainAccount(): Promise<any> {}
  async executeRedemption(): Promise<any> {}
  async getMinimumFunding(): Promise<any> {}
  async REDEMPTION_DELAY(): Promise<any> {}
}

jest.mock('../../abis/factories/StateChainGateway__factory', () => ({
  StateChainGateway__factory: class {
    static connect: (address: string) => MockGateway = jest.fn(
      (address: string) => new MockGateway(address),
    );
  },
}));

jest.mock('../../contracts', () => ({
  ...jest.requireActual('../../contracts'),
  checkAllowance: jest.fn(),
}));

const signerOptions = {
  network: 'sisyphos',
  signer: new VoidSigner('0x0'),
} as const;

describe(fundStateChainAccount, () => {
  it('approves the gateway and funds the account', async () => {
    const checkSpy = jest
      .mocked(checkAllowance)
      .mockResolvedValue({ isAllowable: true });
    const waitMock = jest.fn().mockResolvedValue({ status: 1 });
    const fundSpy = jest
      .spyOn(MockGateway.prototype, 'fundStateChainAccount')
      .mockResolvedValue({ wait: waitMock });

    await fundStateChainAccount('0x1234', '1000', signerOptions);

    expect(checkSpy).toHaveBeenCalled();
    expect(waitMock).toHaveBeenCalledWith(1);
    expect(fundSpy).toHaveBeenCalledWith('0x1234', '1000', {
      nonce: undefined,
    });
  });
});

describe(executeRedemption, () => {
  it('executes the redemption', async () => {
    const waitMock = jest.fn().mockResolvedValue({ status: 1 });
    const executeSpy = jest
      .spyOn(MockGateway.prototype, 'executeRedemption')
      .mockResolvedValue({ wait: waitMock });
    await executeRedemption('0x1234', signerOptions);
    expect(executeSpy).toHaveBeenCalledWith('0x1234', { nonce: undefined });
  });
});

describe(getMinimumFunding, () => {
  it('retrieves minimum funding amount', async () => {
    jest
      .spyOn(MockGateway.prototype, 'getMinimumFunding')
      .mockResolvedValue(ethers.BigNumber.from('1234'));
    expect(await getMinimumFunding(signerOptions)).toEqual(
      ethers.BigNumber.from('1234'),
    );
  });
});

describe(getRedemptionDelay, () => {
  it('retrieves the redemption delay', async () => {
    jest
      .spyOn(MockGateway.prototype, 'REDEMPTION_DELAY')
      .mockResolvedValue(1234);
    expect(await getRedemptionDelay(signerOptions)).toEqual(1234);
  });
});
