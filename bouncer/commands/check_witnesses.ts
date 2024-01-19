#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes 2 argument.
// 1. The chain to check or HASH if you want to pass an hash to check, it can be: ETH, BTC, DOT or HASH
// 2. the encoded call hash if HASH was used as first argument
// It then prints the number of validator who have correctly witnessed it, and in case this number is less than the number of validators in the authority set it will
// print the validator IDs and vanity of the one not witnessing it.
// In order to obtain the hash you can use PolkaJS to construct the transaction that was supposed to be witnessed. You might need some external data to do so.
// Go to PolkaJS > Developer > Extrinsic
// from there, once you built the extrinsic you need the encoded call hash

// For example: ./commands/check_witnesses.ts ETH
// will wait for the next chainStateUpdate extrinsic for ethereum and after some blocks (2) it will check how many validator witnessed it

import { blake2AsHex } from '@polkadot/util-crypto';
import { runWithTimeout, sleep, getChainflipApi } from '../shared/utils';

const witnessHash = new Set<any>();
function hashCall(extrinsic: SubmittableExtrinsic<'promise', ISubmittableResult>) {
  const blakeHash = blake2AsHex(extrinsic.method.toU8a(), 256);
  witnessHash.add(blakeHash);
}
async function main(): Promise<void> {
  const api = await getChainflipApi();
  // we need the epoch number to query the correct storage item
  const chain: string = process.argv[2];
  if (!chain || !(chain === 'ETH' || chain === 'BTC' || chain === 'DOT')) {
    if (chain === 'HASH') {
      const hash = process.argv[3];
      if (hash) {
        witnessHash.add(hash);
      } else {
        console.log('Invalid Args, provide an hash');
        process.exit(-1);
      }
    } else {
      console.log('Invalid Args, provide a chain');
      process.exit(-1);
    }
  }

  let currentBlockNumber = 0;
  while (witnessHash.size === 0) {
    const signedBlock = await api.rpc.chain.getBlock();
    if (signedBlock.block) {
      currentBlockNumber = Number(signedBlock.block.header.number);
      console.log(currentBlockNumber);
    }

    signedBlock.block.extrinsics.forEach((ex: any, _index: any) => {
      if (ex.toHuman().method.method === 'witnessAtEpoch') {
        const callData = ex.toHuman().method.args.call;
        if (callData && callData.section === 'ethereumChainTracking' && chain === 'ETH') {
          const finalData = callData.args;
          // set priorityFee to 0, it is not kept into account for the chaintracking
          finalData.new_chain_state.trackedData.priorityFee = '0';
          const blockHeight = finalData.new_chain_state.blockHeight.replace(/,/g, '');
          const baseFee = finalData.new_chain_state.trackedData.baseFee.replace(/,/g, '');
          // parse the data and removed useless comas
          finalData.new_chain_state.trackedData.baseFee = baseFee;
          finalData.new_chain_state.blockHeight = blockHeight;
          // create the extrinsic we need to witness (ETH chain tracking in this case)
          const extrinsic = api.tx.ethereumChainTracking.updateChainState(
            finalData.new_chain_state,
          );
          // obtain the hash of the extrinsic call
          hashCall(extrinsic);
        }

        if (callData && callData.section === 'polkadotChainTracking' && chain === 'DOT') {
          const finalData = callData.args;
          // set medianTip to 0, it is not kept into account for the chaintracking
          finalData.new_chain_state.trackedData.medianTip = '0';
          // parse the data and removed useless comas
          const blockHeight = finalData.new_chain_state.blockHeight.replace(/,/g, '');
          const runtimeVersion =
            finalData.new_chain_state.trackedData.runtimeVersion.specVersion.replace(/,/g, '');
          finalData.new_chain_state.trackedData.runtimeVersion.specVersion = runtimeVersion;
          finalData.new_chain_state.blockHeight = blockHeight;
          // create the extrinsic we need to witness (DOT chain tracking in this case)
          const extrinsic = api.tx.polkadotChainTracking.updateChainState(
            finalData.new_chain_state,
          );
          // obtain the hash of the extrinsic call
          hashCall(extrinsic);
        }

        if (callData && callData.section === 'bitcoinChainTracking' && chain === 'BTC') {
          const finalData = callData.args;

          // parse the data and removed useless comas
          const blockHeight = finalData.new_chain_state.blockHeight.replace(/,/g, '');

          finalData.new_chain_state.blockHeight = blockHeight;
          // These are the default values we use on the state chain for the btc chain tracking
          finalData.new_chain_state.trackedData.btcFeeInfo = {
            feePerInputUtxo: 7500,
            feePerOutputUtxo: 4300,
            minFeeRequiredPerTx: 1200,
          };
          // create the extrinsic we need to witness (DOT chain tracking in this case)
          const extrinsic = api.tx.bitcoinChainTracking.updateChainState(finalData.new_chain_state);
          // obtain the hash of the extrinsic call
          hashCall(extrinsic);
        }
      }
    });
    await sleep(6000);
  }

  const unsubscribe = await api.rpc.chain.subscribeNewHeads(async (header) => {
    // waiting at least 2 blocks to be sure that we give all validator enough time to witness something
    if (Number(header.number) - currentBlockNumber > 2) {
      unsubscribe();

      for (const elem of witnessHash) {
        const result = await api.rpc('cf_witness_count', elem);
        if (result) {
          console.log(`Number of nodes who failed to witness: ${result.failing_count}`);
          console.log(`List of validators: ${result.validators}`);
        } else {
          console.log('The provided hash is not a valid callhash');
        }
      }
      process.exit(0);
    }
  });
}

runWithTimeout(main(), 3600000).catch(() => {
  console.log('Failed to check amount of witnesses for ' + process.argv[2]);
  process.exit(-1);
});
