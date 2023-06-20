import { executeSwap, ExecuteSwapParams } from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import { chainFromAsset } from '../shared/utils';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function executeNativeSwap(destAssetSymbol: any, destAddress: string) {

    const wallet = Wallet.fromMnemonic(
        'test test test test test test test test test test test junk',
    ).connect(getDefaultProvider('http://localhost:8545'));

    const destAsset = destAssetSymbol.toUpperCase();
    const destChain = chainFromAsset(destAsset);

    await executeSwap(
        {
            destChain,
            destAsset,
            // It is important that this is large enough to result in
            // an amount larger than existential (e.g. on Polkadot):
            amount: '100000000000000000',
            destAddress,
        } as ExecuteSwapParams,
        {
            signer: wallet,
            network: 'localnet',
            vaultContractAddress: '0xb7a5bd0345ef1cc5e66bf61bdec17d2461fbd968',
        },
    );
}
