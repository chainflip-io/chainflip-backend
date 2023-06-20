import { encodeAddress } from '@polkadot/util-crypto';
import { newSwap } from './new_swap';
import { fund } from './fund';
import { getBalance } from './get_balance';
import { Token, chainflipApi as getChainflipApi, observeBalanceIncrease, observeEvent, observeEventWithNameAndQuery } from '../shared/utils';

function extractDestinationAddress(swapInfo: any, destToken: Token) {

    let destAddress = (() => {
        let token = destToken;
        if (token === 'USDC') {
            token = 'ETH';
        }
        return swapInfo[1][token.toLowerCase()];
    })();

    if (destToken === 'BTC') {
        destAddress = destAddress.replace(/^0x/, '');
        destAddress = Buffer.from(destAddress, 'hex').toString();
    }
    if (destToken === 'DOT') {
        destAddress = encodeAddress(destAddress);
    }

    return destAddress;
}

export async function performSwap(sourceToken: Token, destToken: Token, ADDRESS: string) {
    const FEE = 100;

    const tag = `[${sourceToken}->${destToken}]`;

    const chainflipApi = await getChainflipApi();

    const addressPromise = observeEventWithNameAndQuery('swapping:SwapDepositAddressReady',
        (event) => {
            const swapInfo = JSON.parse(event.data.toString());
            const destAddress = extractDestinationAddress(swapInfo, destToken);

            return destAddress.toLowerCase() === ADDRESS.toLowerCase()
        },
        chainflipApi);


    await newSwap(sourceToken, destToken, ADDRESS, FEE);

    console.log(`${tag} The args are:  ${sourceToken} ${destToken} ${ADDRESS} ${FEE}`);

    let depositAddressToken = sourceToken;
    if (sourceToken === 'USDC') {
        depositAddressToken = 'ETH';
    }

    const swapInfo = JSON.parse((await addressPromise).toString());
    let swapAddress = swapInfo[0][depositAddressToken.toLowerCase()];
    const destAddress = swapInfo[1][destToken.toLowerCase()];

    console.log(`${tag} Destination address is: ${destAddress}`);
    console.log(`${tag} The swap address is: ${swapAddress}`);

    if (sourceToken === 'BTC') {
        console.log("Doing BTC address conversion");
        swapAddress = swapAddress.replace(/^0x/, '');
        swapAddress = Buffer.from(swapAddress, 'hex').toString();
    }

    console.log(`${tag} Swap address: ${swapAddress}`);

    const OLD_BALANCE = await getBalance(destToken, ADDRESS);

    console.log(`${tag} Old balance: ${OLD_BALANCE}`);

    const swapExecutedHandle = observeEvent('swapping:SwapExecuted', chainflipApi);

    await fund(sourceToken, swapAddress.toLowerCase())
    console.log(`${tag} Funded the address`);

    await swapExecutedHandle;

    console.log(`${tag} Waiting for balance to update`);

    const newBalance = await observeBalanceIncrease(destToken, ADDRESS, OLD_BALANCE);
    console.log(`${tag} Swap success! New balance: ${newBalance}!`);
}