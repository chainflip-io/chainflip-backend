/* eslint-disable max-classes-per-file */
import { VoidSigner, ethers } from 'ethers';
import {
  fundStateChainAccount,
  executeRedemption,
  getMinimumFunding,
  getRedemptionDelay,
  approveStateChainGateway,
} from '@/shared/stateChainGateway';
import { FundingSDK } from '../index';

jest.mock('@/shared/stateChainGateway');

class MockERC20 {
  async balanceOf(): Promise<ethers.BigNumber> {
    throw new Error('unmocked call');
  }
}

jest.mock('@/shared/abis/factories/ERC20__factory', () => ({
  ERC20__factory: class {
    static connect: () => MockERC20 = jest.fn(() => new MockERC20());
  },
}));

describe(FundingSDK, () => {
  const sdk = new FundingSDK({
    network: 'sisyphos',
    signer: new VoidSigner('0xcafebabe').connect(
      ethers.providers.getDefaultProvider('goerli'),
    ),
  });

  it('uses sisyphos as the default network', () => {
    expect(
      // @ts-expect-error it's private
      new FundingSDK({ signer: null as any }).options.network,
    ).toEqual('perseverance');
  });

  describe(FundingSDK.prototype.fundStateChainAccount, () => {
    it('approves the gateway and funds the account', async () => {
      jest.mocked(fundStateChainAccount).mockResolvedValueOnce({
        transactionHash: '0xabcdef',
      } as any);

      await sdk.fundStateChainAccount('0x1234', '1000');

      expect(fundStateChainAccount).toHaveBeenCalledWith(
        '0x1234',
        '1000',
        // @ts-expect-error it's private
        sdk.options,
      );
    });
  });

  describe(FundingSDK.prototype.executeRedemption, () => {
    it('approves the gateway and funds the account', async () => {
      jest.mocked(executeRedemption).mockResolvedValue({
        transactionHash: '0xabcdef',
      } as any);

      const txHash = await sdk.executeRedemption('0x1234');
      expect(txHash).toEqual('0xabcdef');

      expect(executeRedemption).toHaveBeenCalledWith(
        '0x1234',
        // @ts-expect-error it's private
        sdk.options,
      );
    });
  });

  describe(FundingSDK.prototype.getMinimumFunding, () => {
    it('approves the gateway and funds the account', async () => {
      jest
        .mocked(getMinimumFunding)
        .mockResolvedValue(ethers.BigNumber.from(1000));
      const funding = await sdk.getMinimumFunding();

      expect(getMinimumFunding).toHaveBeenCalledWith(
        // @ts-expect-error it's private
        sdk.options,
      );

      expect(funding).toEqual(1000n);
    });
  });

  describe(FundingSDK.prototype.getRedemptionDelay, () => {
    it('approves the gateway and funds the account', async () => {
      jest.mocked(getRedemptionDelay).mockResolvedValue(1000);
      const delay = await sdk.getRedemptionDelay();

      expect(getRedemptionDelay).toHaveBeenCalledWith(
        // @ts-expect-error it's private
        sdk.options,
      );

      expect(delay).toEqual(1000);
    });
  });

  describe(FundingSDK.prototype.getFlipBalance, () => {
    it('gets the FLIP balance of an address', async () => {
      const spy = jest
        .spyOn(MockERC20.prototype, 'balanceOf')
        .mockResolvedValueOnce(ethers.BigNumber.from(1000));
      const balance = await sdk.getFlipBalance();
      expect(balance).toBe(1000n);
      expect(spy.mock.calls).toMatchInlineSnapshot(`
        [
          [
            "0xcafebabe",
          ],
        ]
      `);
    });
  });

  describe(FundingSDK.prototype.approveStateChainGateway, () => {
    it('requests approval and returns the tx hash', async () => {
      jest.mocked(approveStateChainGateway).mockResolvedValueOnce({
        transactionHash: '0xabcdef',
      } as any);
      const txHash = await sdk.approveStateChainGateway(1, 2);
      expect(txHash).toBe('0xabcdef');
    });
  });
});
