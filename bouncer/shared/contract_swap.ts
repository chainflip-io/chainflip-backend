import { Asset, executeSwap, ExecuteSwapParams } from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import { randomAsHex } from "@polkadot/util-crypto";
import { chainFromAsset, getAddress, getChainflipApi, observeBalanceIncrease, observeEvent, getEthContractAddress } from '../shared/utils';
import { getNextEthNonce } from '../shared/fund_eth';
import { getBalance } from './get_balance';


export async function executeContractSwap(srcAsset: Asset, destAsset: Asset, destAddress: string) {

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
            amount: srcAsset==='USDC' ? '500000000' : '1000000000000000000',
            destAddress,
            ...srcAsset!=='ETH' ? {srcAsset: srcAsset} : {},
        } as ExecuteSwapParams,
        {
            signer: wallet,
            nonce,
            network: 'localnet',
            vaultContractAddress: getEthContractAddress('VAULT'),
            ...srcAsset!=='ETH' ? {srcTokenContractAddress: getEthContractAddress(srcAsset)} : {},
        },
    );
}

export async function performSwapViaContract(sourceAsset: Asset, destAsset: Asset) {

    const tag = `[contract ${sourceAsset} -> ${destAsset}]`;

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
        log(`Executing (${sourceAsset}) contract swap to(${destAsset}) ${addr}. Current balance: ${oldBalance}`)
        const handle = observeEvent("swapping:SwapExecuted", api);
        await executeContractSwap(sourceAsset, destAsset, addr);
        await handle;
        log(`Successfully observed event: swapping: SwapExecuted`);

        const newBalance = await observeBalanceIncrease(destAsset, addr, oldBalance);
        log(`Swap success! New balance: ${newBalance}`);
    } catch (err) {
        throw new Error(`${tag} ${err}`);
    }

}