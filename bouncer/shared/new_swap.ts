import { cryptoWaitReady } from "@polkadot/util-crypto";
import { Keyring } from "@polkadot/api";
import { Mutex } from "async-mutex";
import { Asset } from "@chainflip-io/cli/.";
import { getChainflipApi, encodeDotAddressForContract } from "./utils";

const mutex = new Mutex();

export async function newSwap(sourceToken: Asset, destToken: Asset,
    destAddress: string, fee: any, messageMetadata?: CcmDepositMetadata): Promise<void> {
    await cryptoWaitReady();
    const keyring = new Keyring({ type: 'sr25519' });

    const chainflip = await getChainflipApi();
    const destinationAddress =
        destToken === 'DOT' ? encodeDotAddressForContract(destAddress, keyring) : destAddress;
    const brokerUri = process.env.BROKER_URI ?? '//BROKER_1';
    const broker = keyring.createFromUri(brokerUri);

    await mutex.runExclusive(async () => {
        await chainflip.tx.swapping
            .requestSwapDepositAddress(
                sourceToken,
                destToken,
                { [destToken === 'USDC' ? 'ETH' : destToken]: destinationAddress },
                fee,
                messageMetadata ?? null,
            )
            .signAndSend(broker, { nonce: -1 });
    })

}

export interface CcmDepositMetadata {
    message: string;
    gas_budget: number;
    cf_parameters: string;
    source_address: any;
  }