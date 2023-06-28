import { Asset, executeSwap, ExecuteSwapParams } from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import { chainFromAsset } from '../shared/utils';
import { getNextEthNonce } from '../shared/fund_eth';


// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function executeNativeSwap(destAsset: Asset, destAddress: string) {

    const wallet = Wallet.fromMnemonic(
        process.env.ETH_USDC_WHALE_MNEMONIC ??
        'test test test test test test test test test test test junk',
    ).connect(getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

    const destChain = chainFromAsset(destAsset);

    const nonce = await getNextEthNonce();

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
            nonce,
            network: 'localnet',
            vaultContractAddress: '0xb7a5bd0345ef1cc5e66bf61bdec17d2461fbd968',
        },
    );
}
