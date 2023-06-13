import { executeSwap, ChainId } from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import { decodeAddress } from '@polkadot/util-crypto';
import { u8aToHex } from '@polkadot/util';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function executeNativeSwap(destTokenSymbol: any, destAddress: string) {

    const wallet = Wallet.fromMnemonic(
        'test test test test test test test test test test test junk',
    ).connect(getDefaultProvider('http://localhost:8545'));

    await executeSwap(
        {
            destChainId: ChainId.Polkadot,
            destTokenSymbol,
            // It is important that this is large enough to result in
            // an amount larger than existential (on Polkadot):
            amount: '100000000000000000',
            destAddress: u8aToHex(decodeAddress(destAddress)),
        },
        {
            signer: wallet,
            cfNetwork: 'localnet',
            vaultContractAddress: '0xb7a5bd0345ef1cc5e66bf61bdec17d2461fbd968',
        },
    );
}