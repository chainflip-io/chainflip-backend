/* eslint-disable @typescript-eslint/no-empty-function */
/* eslint-disable @typescript-eslint/lines-between-class-members */
/* eslint-disable max-classes-per-file */
import { BigNumber, VoidSigner } from 'ethers';
import { Assets, ChainflipNetworks, Chains } from '../../enums';
import executeCall from '../executeCall';
import { ExecuteCallParams } from '../schemas';

const ETH_ADDRESS = '0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2';

class MockVault {
  constructor(readonly address: string) {}
  async xCallNative(): Promise<any> {}
  async xCallToken(): Promise<any> {}
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

describe(executeCall, () => {
  it.each([ChainflipNetworks.perseverance, ChainflipNetworks.mainnet] as const)(
    'only works on sisyphos for now',
    async (network) => {
      await expect(
        executeCall({} as any, {
          network,
          signer: new VoidSigner('MY ADDRESS'),
        }),
      ).rejects.toThrowError();
    },
  );

  it.each([
    {
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
      destAsset: Assets.FLIP,
      destChain: Chains.Ethereum,
      destAddress: ETH_ADDRESS,
    },
    {
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
      destAsset: Assets.USDC,
      destChain: Chains.Ethereum,
      destAddress: ETH_ADDRESS,
    },
  ] as Omit<ExecuteCallParams, 'amount' | 'message' | 'gasAmount'>[])(
    'submits a native call (%p)',
    async (params) => {
      const wait = jest
        .fn()
        .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
      const callSpy = jest
        .spyOn(MockVault.prototype, 'xCallNative')
        .mockResolvedValue({ wait });

      expect(
        await executeCall(
          {
            amount: '1',
            message: '0xdeadc0de',
            gasAmount: '101',
            ...params,
          } as ExecuteCallParams,
          {
            network: ChainflipNetworks.sisyphos,
            signer: new VoidSigner('MY ADDRESS'),
          },
        ),
      ).toStrictEqual({ status: 1, transactionHash: 'hello world' });
      expect(wait).toHaveBeenCalledWith(1);
      expect(callSpy.mock.calls).toMatchSnapshot();
    },
  );

  it.each([
    ...(
      [
        { srcAsset: Assets.FLIP, srcChain: Chains.Ethereum },
        { srcAsset: Assets.USDC, srcChain: Chains.Ethereum },
      ] as const
    ).flatMap(
      (src) =>
        [
          {
            destAsset: Assets.ETH,
            destChain: Chains.Ethereum,
            destAddress: ETH_ADDRESS,
            ...src,
          },
        ] as const,
    ),
  ])('submits a token call (%p)', async (params) => {
    const wait = jest
      .fn()
      .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
    const approveSpy = jest
      .spyOn(MockERC20.prototype, 'approve')
      .mockResolvedValue({ wait });
    const callSpy = jest
      .spyOn(MockVault.prototype, 'xCallToken')
      .mockResolvedValue({ wait });
    const allowanceSpy = jest.spyOn(MockERC20.prototype, 'allowance');

    expect(
      await executeCall(
        {
          amount: '1',
          message: '0xdeadc0de',
          gasAmount: '101',
          ...params,
        },
        {
          network: 'sisyphos',
          signer: new VoidSigner('MY ADDRESS'),
        },
      ),
    ).toStrictEqual({ status: 1, transactionHash: 'hello world' });
    expect(wait).toHaveBeenCalledWith(1);
    expect(callSpy.mock.calls).toMatchSnapshot();
    expect(allowanceSpy.mock.calls).toMatchSnapshot();
    expect(approveSpy).not.toHaveBeenCalled();
  });

  it('submits a token call with sufficient approval', async () => {
    const wait = jest
      .fn()
      .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
    const approveSpy = jest
      .spyOn(MockERC20.prototype, 'approve')
      .mockRejectedValue(Error('unmocked call'));
    const swapSpy = jest
      .spyOn(MockVault.prototype, 'xCallToken')
      .mockResolvedValue({ wait });
    const allowanceSpy = jest
      .spyOn(MockERC20.prototype, 'allowance')
      .mockResolvedValueOnce(BigNumber.from(Number.MAX_SAFE_INTEGER - 1));

    expect(
      await executeCall(
        {
          destAsset: Assets.ETH,
          destChain: Chains.Ethereum,
          destAddress: ETH_ADDRESS,
          srcAsset: Assets.FLIP,
          srcChain: Chains.Ethereum,
          amount: '1',
          message: '0xdeadc0de',
          gasAmount: '101',
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
    const callSpy = jest
      .spyOn(MockVault.prototype, 'xCallToken')
      .mockResolvedValue({ wait });
    const allowanceSpy = jest
      .spyOn(MockERC20.prototype, 'allowance')
      .mockResolvedValueOnce(BigNumber.from(Number.MAX_SAFE_INTEGER - 1));

    expect(
      await executeCall(
        {
          srcChain: Chains.Ethereum,
          destAsset: Assets.ETH,
          destChain: Chains.Ethereum,
          destAddress: ETH_ADDRESS,
          srcAsset: Assets.FLIP,
          amount: '1',
          message: '0xdeadc0de',
          gasAmount: '101',
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
    expect(callSpy.mock.calls).toMatchSnapshot();
    expect(allowanceSpy.mock.calls).toMatchSnapshot();
    expect(approveSpy).not.toHaveBeenCalled();
  });

  it.each([1, '1', 1n])('accepts a nonce (%o)', async (nonce) => {
    const wait = jest
      .fn()
      .mockResolvedValue({ status: 1, transactionHash: 'hello world' });
    const callSpy = jest
      .spyOn(MockVault.prototype, 'xCallNative')
      .mockResolvedValue({ wait });

    expect(
      await executeCall(
        {
          srcChain: Chains.Ethereum,
          srcAsset: Assets.ETH,
          amount: '1',
          destAsset: Assets.FLIP,
          destChain: Chains.Ethereum,
          destAddress: ETH_ADDRESS,
          message: '0xdeadc0de',
          gasAmount: '101',
        },
        {
          network: ChainflipNetworks.sisyphos,
          signer: new VoidSigner('MY ADDRESS'),
          nonce,
        },
      ),
    ).toStrictEqual({ status: 1, transactionHash: 'hello world' });
    expect(wait).toHaveBeenCalledWith(1);
    expect(callSpy.mock.calls).toMatchSnapshot();
  });
});
