import { Keyring } from "@polkadot/api";
import { u8aToHex } from "@polkadot/util";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { Mutex } from "async-mutex";
import { Token, getChainflipApi } from "./utils";

const mutex = new Mutex();

export async function newSwap(sourceToken: Token, destToken: Token,
    destAddress: string, fee: any): Promise<void> {
    await cryptoWaitReady();
    const keyring = new Keyring({ type: 'sr25519' });

    const chainflip = await getChainflipApi();
    const destinationAddress =
        destToken === 'DOT' ? u8aToHex(keyring.decodeAddress(destAddress)) : destAddress;

    const brokerUri = process.env.BROKER_URI ?? '//BROKER_1';
    const broker = keyring.createFromUri(brokerUri);

    await mutex.runExclusive(async () => {
        await chainflip.tx.swapping
            .requestSwapDepositAddress(
                sourceToken,
                destToken,
                { [destToken === 'USDC' ? 'ETH' : destToken]: destinationAddress },
                fee,
                null,
            )
            .signAndSend(broker, { nonce: -1 });
    })

}