/* eslint-disable max-classes-per-file */
import { VoidSigner, getDefaultProvider } from 'ethers';
import {
  fundStateChainAccount,
  executeRedemption,
  getMinimumFunding,
  getRedemptionDelay,
  getPendingRedemption,
  approveStateChainGateway,
} from '@/shared/stateChainGateway';
import { FundingSDK } from '../index';

jest.mock('@/shared/stateChainGateway');

class MockERC20 {
  async balanceOf(): Promise<bigint> {
    throw new Error('unmocked call');
  }
}

jest.mock('@/shared/abis/factories/ERC20__factory', () => ({
  ERC20__factory: class {
    static connect() {
      return new MockERC20();
    }
  },
}));

describe(FundingSDK, () => {
  const sdk = new FundingSDK({
    network: 'sisyphos',
    signer: new VoidSigner('0xcafebabe').connect(getDefaultProvider('goerli')),
  });

  it('uses perseverance as the default network', () => {
    expect(
      // @ts-expect-error it's private
      new FundingSDK({ signer: null as any }).options.network,
    ).toEqual('perseverance');
  });

  it('support mainnet', () => {
    expect(
      // @ts-expect-error it's private
      new FundingSDK({ signer: null as any, network: 'mainnet' }).options
        .network,
    ).toEqual('mainnet');
  });

  describe(FundingSDK.prototype.fundStateChainAccount, () => {
    it('approves the gateway and funds the account', async () => {
      jest.mocked(fundStateChainAccount).mockResolvedValueOnce({
        hash: '0xabcdef',
      } as any);

      await sdk.fundStateChainAccount('0x1234', 1000n);

      expect(fundStateChainAccount).toHaveBeenCalledWith(
        '0x1234',
        1000n,
        // @ts-expect-error it's private
        sdk.options,
        {},
      );
    });
  });

  describe(FundingSDK.prototype.executeRedemption, () => {
    it('executes the redemption', async () => {
      jest.mocked(executeRedemption).mockResolvedValue({
        hash: '0xabcdef',
      } as any);

      const txHash = await sdk.executeRedemption('0x1234');
      expect(txHash).toEqual('0xabcdef');

      expect(executeRedemption).toHaveBeenCalledWith(
        '0x1234',
        // @ts-expect-error it's private
        sdk.options,
        {},
      );
    });
  });

  describe(FundingSDK.prototype.getMinimumFunding, () => {
    it('returns the minimum funding', async () => {
      jest.mocked(getMinimumFunding).mockResolvedValue(1000n);
      const funding = await sdk.getMinimumFunding();

      expect(getMinimumFunding).toHaveBeenCalledWith(
        // @ts-expect-error it's private
        sdk.options,
      );

      expect(funding).toEqual(1000n);
    });
  });

  describe(FundingSDK.prototype.getRedemptionDelay, () => {
    it('returns the redemption delay', async () => {
      jest.mocked(getRedemptionDelay).mockResolvedValue(1000n);
      const delay = await sdk.getRedemptionDelay();

      expect(getRedemptionDelay).toHaveBeenCalledWith(
        // @ts-expect-error it's private
        sdk.options,
      );

      expect(delay).toEqual(1000n);
    });
  });

  describe(FundingSDK.prototype.getFlipBalance, () => {
    it('gets the FLIP balance of an address', async () => {
      const spy = jest
        .spyOn(MockERC20.prototype, 'balanceOf')
        .mockResolvedValueOnce(1000n);
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

  describe(FundingSDK.prototype.getPendingRedemption, () => {
    it('returns the pending redemption for an account', async () => {
      const redemption = {
        amount: 101n,
        redeemAddress: '0xcoffeebabe',
        startTime: 1695126000n,
        expiryTime: 1695129600n,
      };
      jest.mocked(getPendingRedemption).mockResolvedValue(redemption);
      const result = await sdk.getPendingRedemption('0xcoffeebabe');

      expect(getPendingRedemption).toHaveBeenCalledWith(
        '0xcoffeebabe',
        // @ts-expect-error it's private
        sdk.options,
      );

      expect(result).toEqual(redemption);
    });
  });

  describe(FundingSDK.prototype.approveStateChainGateway, () => {
    it('requests approval and returns the tx hash', async () => {
      jest.mocked(approveStateChainGateway).mockResolvedValueOnce({
        hash: '0xabcdef',
      } as any);
      const txHash = await sdk.approveStateChainGateway(1n, {});
      expect(txHash).toBe('0xabcdef');
    });
  });
});
