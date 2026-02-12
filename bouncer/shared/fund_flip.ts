import type { HexString } from '@polkadot/util/types';
import { fundStateChainAccount } from '@chainflip/cli';
import { Wallet, ethers } from 'ethers';
import { getNextEvmNonce } from 'shared/send_evm';
import {
  getContractAddress,
  hexPubkeyToFlipAddress,
  decodeFlipAddressForContract,
  getEvmEndpoint,
  assetDecimals,
  amountToFineAmount,
  getEvmWhaleKeypair,
} from 'shared/utils';
import { approveErc20 } from 'shared/approve_erc20';
import { ChainflipIO } from 'shared/utils/chainflip_io';
import { fundingFunded } from 'generated/events/funding/funded';

export async function fundFlip<A = []>(cf: ChainflipIO<A>, scAddress: string, flipAmount: string) {
  // Doing effectively infinite approvals to prevent race conditions between tests
  await approveErc20(
    cf.logger,
    'Flip',
    getContractAddress('Ethereum', 'GATEWAY'),
    '100000000000000000000000000',
  );

  const flipperinoAmount = amountToFineAmount(flipAmount, assetDecimals('Flip'));

  const flipContractAddress = getContractAddress('Ethereum', 'Flip');

  const gatewayContractAddress = getContractAddress('Ethereum', 'GATEWAY');

  const { privkey: whalePrivKey } = getEvmWhaleKeypair('Ethereum');
  cf.debug('Approving ' + flipAmount + ' Flip to State Chain Gateway');

  const wallet = new Wallet(whalePrivKey, ethers.getDefaultProvider(getEvmEndpoint('Ethereum')));

  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    stateChainGatewayContractAddress: gatewayContractAddress,
    flipContractAddress,
  } as const;
  const txOptions = {
    nonce: await getNextEvmNonce(cf.logger, 'Ethereum'),
  } as const;

  cf.debug('Funding ' + flipAmount + ' Flip to ' + scAddress);
  let pubkey = decodeFlipAddressForContract(scAddress);

  if (pubkey.substr(0, 2) !== '0x') {
    pubkey = '0x' + pubkey;
  }
  const receipt2 = await fundStateChainAccount(
    pubkey as HexString,
    BigInt(flipperinoAmount),
    networkOptions,
    txOptions,
  );

  cf.debug(
    'Transaction complete, tx_hash: ' +
      receipt2.hash +
      ' blockNumber: ' +
      receipt2.blockNumber +
      ' blockHash: ' +
      receipt2.blockHash,
  );

  await cf.stepUntilEvent(
    'Funding.Funded',
    fundingFunded.refine((event) => event.accountId === hexPubkeyToFlipAddress(pubkey)),
  );
}
