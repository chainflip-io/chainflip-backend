import type { HexString } from '@polkadot/util/types';
import { fundStateChainAccount } from 'shared/utils/chainflip_cli';
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
  sleep,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { approveErc20 } from 'shared/approve_erc20';
import { ChainflipIO } from 'shared/utils/chainflip_io';
import { fundingFundedEvent } from 'generated/events/funding/funded';

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
    nonce: await getNextEvmNonce(cf.logger, 'Ethereum', { privkey: whalePrivKey }),
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

  const scAccount = hexPubkeyToFlipAddress(pubkey);
  await cf.stepUntilEvent(fundingFundedEvent.refine((event) => event.accountId === scAccount));

  // The Funded event is observed via the indexer (a best-block view), but dedot validates every
  // extrinsic against the FINALIZED block before broadcasting. A follow-up extrinsic from this
  // freshly-funded account would otherwise be rejected with "Invalid - Payment" until the credit
  // is finalized.
  await using chainflip = await getChainflipApi();
  for (let attempt = 0; attempt < 30; attempt++) {
    const finalized = await chainflip.at((await chainflip.block.finalized()).hash);
    if ((await finalized.query.flip.account(scAccount)).balance > 0n) {
      return;
    }
    await sleep(1000);
  }
  throw new Error(`Funding of ${scAccount} confirmed via event but not finalized in time`);
}
