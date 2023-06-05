import { executeSwap, ChainId } from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';

const wallet = Wallet.fromMnemonic(
  'test test test test test test test test test test test junk',
).connect(getDefaultProvider('http://localhost:8545'));

// execute native swap
await executeSwap(
  {
    destChainId: ChainId.Ethereum,
    destTokenSymbol: 'FLIP',
    amount: '1', // 1 wei,
    destAddress: '0xcafebabe',
  },
  {
    signer: wallet,
    cfNetwork: 'localnet',
    vaultContractAddress: '0x1234567890123456789012345678901234567890',
  },
);

// execute token swap
await executeSwap(
  {
    destChainId: ChainId.Ethereum,
    destTokenSymbol: 'FLIP',
    amount: '1', // 1 wei,
    destAddress: '0xcafebabe',
    srcTokenSymbol: 'USDC',
  },
  {
    signer: wallet,
    cfNetwork: 'localnet',
    vaultContractAddress: '0x1234567890123456789012345678901234567890',
    srcTokenContractAddress: '0x1234567890123456789012345678901234567890',
  },
);
