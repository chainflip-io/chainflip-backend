import { Asset, executeSwap, ExecuteSwapParams, approveVault} from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import { randomAsHex } from "@polkadot/util-crypto";
import { chainFromAsset, getAddress, getChainflipApi, observeBalanceIncrease, observeEvent, getEthContractAddress } from '../shared/utils';
import { getNextEthNonce } from './send_eth';
import { getBalance } from './get_balance';


export async function executeContractSwap(srcAsset: Asset, destAsset: Asset, destAddress: string): Promise<any> {

    const wallet = Wallet.fromMnemonic(
        process.env.ETH_USDC_WHALE_MNEMONIC ??
        'test test test test test test test test test test test junk',
    ).connect(getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

    const destChain = chainFromAsset(destAsset);

    const nonce = await getNextEthNonce();

    const receipt = await executeSwap(
        {
            destChain,
            destAsset,
            // It is important that this is large enough to result in
            // an amount larger than existential (e.g. on Polkadot):
            amount: srcAsset==='USDC' ? '500000000' : '1000000000000000000',
            destAddress,
            ...srcAsset!=='ETH' ? {srcAsset} : {},
        } as ExecuteSwapParams,
        {
            signer: wallet,
            nonce,
            network: 'localnet',
            vaultContractAddress: getEthContractAddress('VAULT'),
            ...srcAsset!=='ETH' ? {srcTokenContractAddress: getEthContractAddress(srcAsset)} : {},
        },
    );
    return receipt;
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
        log(`Old balance: ${oldBalance}`);
        log(`Executing (${sourceAsset}) contract swap to(${destAsset}) ${addr}. Current balance: ${oldBalance}`);
        // To uniquely identify the contractSwap, we need to use the TX hash. This is only known
        // after sending the transaction, so we send it first and observe the events afterwards.
        // There are still multiple blocks of safety margin inbetween before the event is emitted
        const receipt = await executeContractSwap(sourceAsset, destAsset, addr);
        await observeEvent("swapping:SwapScheduled", api, (event) => {
            if('vault' in event[5]){
                return event[5].vault.txHash == receipt.transactionHash;
            }
            // Otherwise it was a swap scheduled by requesting a deposit address
            return false;
        });
        log(`Successfully observed event: swapping: SwapScheduled`);

        const newBalance = await observeBalanceIncrease(destAsset, addr, oldBalance);
        log(`Swap success! New balance: ${newBalance}`);
    } catch (err) {
        throw new Error(`${tag} ${err}`);
    }
}

export async function approveTokenVault(srcAsset: 'FLIP' | 'USDC', amount: string) {
    const wallet = Wallet.fromMnemonic(
        process.env.ETH_USDC_WHALE_MNEMONIC ??
        'test test test test test test test test test test test junk',
    ).connect(getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

    const nonce = await getNextEthNonce();
    return approveVault(
        {
            amount,
            srcAsset,
        },
        {
            signer: wallet,
            nonce,
            network: 'localnet',
            vaultContractAddress: getEthContractAddress('VAULT'),
            srcTokenContractAddress: getEthContractAddress(srcAsset)
        },
    )
}

