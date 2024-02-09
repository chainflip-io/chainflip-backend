/* eslint-disable @typescript-eslint/no-empty-function */
/* eslint-disable @typescript-eslint/lines-between-class-members */
/* eslint-disable max-classes-per-file */
import { BigNumber, VoidSigner } from 'ethers';
import { Assets, ChainflipNetworks, Chains } from '../../enums';
import executeSwap from '../executeSwap';
import { ExecuteSwapParams } from '../schemas';

const ETH_ADDRESS = '0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2';
const DOT_ADDRESS = '5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX';
const BTC_ADDRESS = 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq';

class MockVault {
  constructor(readonly address: string) {}
  async xSwapNative(): Promise<any> {}
  async xSwapToken(): Promise<any> {}
}

class MockERC20 {
  async approve(): Promise<any> {}
  async allowance(): Promise<any> {
    return BigNumber.from(Number.MAX_SAFE_INTEGER - 1);
  }
}

jest.mock('../../abis/factories/Vault__factory', () => ({
  Vault__factory: class {
    static connect: (address: string) => MockVault = jest.fn(
      (address: string) => new MockVault(address),
    );
  },
}));

jest.mock('../../abis/factories/ERC20__factory', () => ({
  ERC20__factory: class {
    static connect: () => MockERC20 = jest.fn(() => new MockERC20());
  },
}));

describe(executeSwap, () => {
  it.each([ChainflipNetworks.perseverance, ChainflipNetworks.mainnet] as const)(
    'only works on sisyphos for now',
    async (network) => {
      await expect(
        executeSwap({} as any, {
          network,
          signer: new VoidSigner('MY ADDRESS'),
        }),
      ).rejects.toThrowError();
    },
  );

  it.each([
    {
      destAsset: Assets.BTC,
      destChain: Chains.Bitcoin,
      destAddress: BTC_ADDRESS,
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
    },
    {
      destAsset: 'BTC',
      destChain: 'Bitcoin',
      destAddress: BTC_ADDRESS,
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
    },
    {
      destAsset: Assets.FLIP,
      destChain: Chains.Ethereum,
      destAddress: ETH_ADDRESS,
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
    },
    {
      destAsset: Assets.USDC,
      destChain: Chains.Ethereum,
      destAddress: ETH_ADDRESS,
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
    },
    {
      destAsset: Assets.DOT,
      destChain: Chains.Polkadot,
      destAddress: DOT_ADDRESS,
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
    },
  ] as Omit<ExecuteSwapParams, 'amount'>[])(
    'submits a native swap (%p)',
    async (params) => {
      const wait = jest
        .fn()
        .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
      const swapSpy = jest
        .spyOn(MockVault.prototype, 'xSwapNative')
        .mockResolvedValue({ wait });

      expect(
        await executeSwap({ amount: '1', ...params } as ExecuteSwapParams, {
          network: ChainflipNetworks.sisyphos,
          signer: new VoidSigner('MY ADDRESS'),
        }),
      ).toStrictEqual({ status: 1, transactionHash: 'hello world' });
      expect(wait).toHaveBeenCalledWith(1);
      expect(swapSpy.mock.calls).toMatchSnapshot();
    },
  );

  it.each([
    ...[
      { srcAsset: Assets.FLIP, srcChain: Chains.Ethereum },
      { srcAsset: Assets.USDC, srcChain: Chains.Ethereum },
    ].flatMap((src) => [
      {
        destAsset: Assets.BTC,
        destChain: Chains.Bitcoin,
        destAddress: BTC_ADDRESS,
        ...src,
      },
      {
        destAsset: 'BTC',
        destChain: 'Bitcoin',
        destAddress: BTC_ADDRESS,
        ...src,
      },
      {
        destAsset: Assets.ETH,
        destChain: Chains.Ethereum,
        destAddress: ETH_ADDRESS,
        ...src,
      },
      {
        destAsset: Assets.DOT,
        destChain: Chains.Polkadot,
        destAddress: DOT_ADDRESS,
        ...src,
      },
    ]),
  ] as Omit<ExecuteSwapParams, 'amount'>[])(
    'submits a token swap (%p)',
    async (params) => {
      const wait = jest
        .fn()
        .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
      const approveSpy = jest
        .spyOn(MockERC20.prototype, 'approve')
        .mockResolvedValue({ wait });
      const swapSpy = jest
        .spyOn(MockVault.prototype, 'xSwapToken')
        .mockResolvedValue({ wait });
      const allowanceSpy = jest.spyOn(MockERC20.prototype, 'allowance');

      expect(
        await executeSwap({ amount: '1', ...params } as ExecuteSwapParams, {
          network: 'sisyphos',
          signer: new VoidSigner('MY ADDRESS'),
        }),
      ).toStrictEqual({ status: 1, transactionHash: 'hello world' });
      expect(wait).toHaveBeenCalledWith(1);
      expect(swapSpy.mock.calls).toMatchSnapshot();
      expect(allowanceSpy.mock.calls).toMatchSnapshot();
      expect(approveSpy).not.toHaveBeenCalled();
    },
  );

  it('submits a token swap with sufficient approval', async () => {
    const wait = jest
      .fn()
      .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
    const approveSpy = jest
      .spyOn(MockERC20.prototype, 'approve')
      .mockRejectedValue(Error('unmocked call'));
    const swapSpy = jest
      .spyOn(MockVault.prototype, 'xSwapToken')
      .mockResolvedValue({ wait });
    const allowanceSpy = jest
      .spyOn(MockERC20.prototype, 'allowance')
      .mockResolvedValueOnce(BigNumber.from(Number.MAX_SAFE_INTEGER - 1));

    expect(
      await executeSwap(
        {
          destAsset: Assets.BTC,
          destChain: Chains.Bitcoin,
          destAddress: BTC_ADDRESS,
          srcAsset: Assets.FLIP,
          srcChain: Chains.Ethereum,
          amount: '1',
        },
        { network: 'sisyphos', signer: new VoidSigner('MY ADDRESS') },
      ),
    ).toStrictEqual({ status: 1, transactionHash: 'hello world' });
    expect(wait).toHaveBeenCalledWith(1);
    expect(swapSpy.mock.calls).toMatchSnapshot();
    expect(allowanceSpy.mock.calls).toMatchSnapshot();
    expect(approveSpy).not.toHaveBeenCalled();
  });

  it('can be invoked with localnet options', async () => {
    const wait = jest
      .fn()
      .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
    const approveSpy = jest
      .spyOn(MockERC20.prototype, 'approve')
      .mockResolvedValue({ wait });
    const swapSpy = jest
      .spyOn(MockVault.prototype, 'xSwapToken')
      .mockResolvedValue({ wait });
    const allowanceSpy = jest
      .spyOn(MockERC20.prototype, 'allowance')
      .mockResolvedValueOnce(BigNumber.from(Number.MAX_SAFE_INTEGER - 1));

    expect(
      await executeSwap(
        {
          destAsset: Assets.BTC,
          destChain: Chains.Bitcoin,
          destAddress: BTC_ADDRESS,
          srcAsset: Assets.FLIP,
          amount: '1',
          srcChain: Chains.Ethereum,
        },
        {
          network: 'localnet',
          signer: new VoidSigner('MY ADDRESS'),
          vaultContractAddress: '0x123',
          srcTokenContractAddress: '0x456',
        },
      ),
    ).toStrictEqual({ status: 1, transactionHash: 'hello world' });
    expect(wait).toHaveBeenCalledWith(1);
    expect(swapSpy.mock.calls).toMatchSnapshot();
    expect(allowanceSpy.mock.calls).toMatchSnapshot();
    expect(approveSpy).not.toHaveBeenCalled();
  });

  it.each([1, '1', 1n])('accepts a nonce (%o)', async (nonce) => {
    const wait = jest
      .fn()
      .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
    const swapSpy = jest
      .spyOn(MockVault.prototype, 'xSwapNative')
      .mockResolvedValue({ wait });

    expect(
      await executeSwap(
        {
          amount: '1',
          destAsset: Assets.BTC,
          destChain: Chains.Bitcoin,
          destAddress: BTC_ADDRESS,
          srcAsset: Assets.ETH,
          srcChain: Chains.Ethereum,
        },
        {
          network: ChainflipNetworks.sisyphos,
          signer: new VoidSigner('MY ADDRESS'),
          nonce,
        },
      ),
    ).toStrictEqual({ status: 1, transactionHash: 'hello world' });
    expect(wait).toHaveBeenCalledWith(1);
    expect(swapSpy.mock.calls).toMatchSnapshot();
  });
});
