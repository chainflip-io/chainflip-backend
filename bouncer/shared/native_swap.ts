import { Asset, executeSwap, ExecuteSwapParams } from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import { randomAsHex } from "@polkadot/util-crypto";
import { chainFromAsset, getAddress, getChainflipApi, observeBalanceIncrease, observeEvent } from '../shared/utils';
import { getNextEthNonce } from '../shared/fund_eth';
import { getBalance } from './get_balance';


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

export async function performNativeSwap(destAsset: Asset) {

    const tag = `[contract ETH -> ${destAsset}]`;

    const log = (msg: string) => {
        console.log(`${tag} ${msg}`);
    };

    try {
        const api = await getChainflipApi();
        const seed = randomAsHex(32);
        const addr = await getAddress(destAsset, seed);
        log(`Destination address: ${addr}`);

        const oldBalance = await getBalance(destAsset, addr);
        log(`Old balance: ${addr}`);
        // Note that we start observing events before executing
        // the swap to avoid race conditions:
        log(`Executing native contract swap to(${destAsset}) ${addr}. Current balance: ${oldBalance}`)
        const handle = observeEvent("swapping:SwapExecuted", api);
        await executeNativeSwap(destAsset, addr);
        await handle;
        log(`Successfully observed event: swapping: SwapExecuted`);

        const newBalance = await observeBalanceIncrease(destAsset, addr, oldBalance);
        log(`Swap success! New balance: ${newBalance}`);
    } catch (err) {
        throw new Error(`${tag} ${err}`);
    }

}