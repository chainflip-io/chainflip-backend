import { encodeAddress } from '@polkadot/util-crypto';
import { Asset } from '@chainflip-io/cli/.';
import { newSwap } from './new_swap';
import { fund } from './fund';
import { getBalance } from './get_balance';
import { getChainflipApi, observeBalanceIncrease, observeEvent, observeCcmReceived, encodeBtcAddressForContract } from '../shared/utils';
import { CcmDepositMetadata } from "../shared/new_swap";

function extractDestinationAddress(swapInfo: any, destToken: Asset): string | undefined {
    const token = (destToken === 'USDC') ? 'ETH' : destToken;
    return swapInfo[1][token.toLowerCase()];
}

function encodeDestinationAddress(address: string, destToken: Asset): string {

    let destAddress = address;

    if (destAddress && destToken === 'BTC') {
        destAddress = destAddress.replace(/^0x/, '');
        destAddress = Buffer.from(destAddress, 'hex').toString();
    }
    if (destAddress && destToken === 'DOT') {
        destAddress = encodeAddress(destAddress);
    }

    return destAddress;
}

export async function performSwap(sourceToken: Asset, destToken: Asset, ADDRESS: string, swapTag?: string, messageMetadata?: CcmDepositMetadata) {
    const FEE = 100;

    const tag = swapTag ?? '';

    const chainflipApi = await getChainflipApi();

    const addressPromise = observeEvent('swapping:SwapDepositAddressReady', chainflipApi,
        (swapInfo: any) => {
            // Find deposit address for the right swap by looking at destination address:
            const destAddress = extractDestinationAddress(swapInfo, destToken);
            if (!destAddress) return false;

            const destAddressEncoded = encodeDestinationAddress(destAddress, destToken);
            
            const destTokenMatches = swapInfo[4].charAt(0) + swapInfo[4].slice(1).toUpperCase() === destToken;
            const sourceTokenMatches = swapInfo[3].charAt(0) + swapInfo[3].slice(1).toUpperCase() === sourceToken;
            const destAddressMatches = destAddressEncoded.toLowerCase() === ADDRESS.toLowerCase();

            return destAddressMatches && destTokenMatches && sourceTokenMatches;
        });

    await newSwap(sourceToken, destToken, ADDRESS, FEE, messageMetadata);

    console.log(`${tag} The args are:  ${sourceToken} ${destToken} ${ADDRESS} ${FEE} ${messageMetadata ? `someMessage` : ''}`);

    let depositAddressToken = sourceToken;
    if (sourceToken === 'USDC') {
        depositAddressToken = 'ETH';
    }

    const swapInfo = JSON.parse((await addressPromise).toString());
    let swapAddress = swapInfo[0][depositAddressToken.toLowerCase()];
    const destAddress = extractDestinationAddress(swapInfo, destToken);

    console.log(`${tag} Destination address is: ${destAddress}`);

    if (sourceToken === 'BTC') {
        swapAddress = encodeBtcAddressForContract(swapAddress);
    }

    console.log(`${tag} Swap address: ${swapAddress}`);

    const OLD_BALANCE = await getBalance(destToken, ADDRESS);

    console.log(`${tag} Old balance: ${OLD_BALANCE}`);

    const swapExecutedHandle = observeEvent('swapping:SwapExecuted', chainflipApi);

    const ccmEventEmitted = messageMetadata
    ? observeCcmReceived(sourceToken, destToken, ADDRESS, messageMetadata)
    : Promise.resolve();

    await fund(sourceToken, swapAddress.toLowerCase())
    console.log(`${tag} Funded the address`);

    await swapExecutedHandle;
  
    console.log(`${tag} Waiting for balance to update`);

    try {
        const [newBalance,] = await Promise.all([observeBalanceIncrease(destToken, ADDRESS, OLD_BALANCE), ccmEventEmitted]);

        console.log(`${tag} Swap success! New balance: ${newBalance}!`);
    }
    catch (err) {
        throw new Error(`${tag} ${err}`);
    }

}