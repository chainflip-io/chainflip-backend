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

// For example: ./commands/check_witnesses.ts ETH
// will wait for the next chainStateUpdate extrinsic for ethereum and after some blocks (2) it will check how many validator witnessed it

import { runWithTimeout, sleep, getChainflipApi } from '../shared/utils';
import { blake2AsHex } from '@polkadot/util-crypto';

const witnessHash = new Set<any>();

async function main(): Promise<void> {
    const api = await getChainflipApi();
    // we need the epoch number to query the correct storage item
    const chain: string = process.argv[2];
    if(!chain || !(chain === "ETH" || chain === "BTC" || chain === "DOT")) {
        if (chain === "HASH") {
            const hash = process.argv[3];
            if(hash){
                witnessHash.add(hash)
            } else {
                console.log("Invalid Args, provide an hash");
                process.exit(-1);
            }
        } else {
            console.log("Invalid Args, provide a chain");
            process.exit(-1);
        }
    }

    let currentBlockNumber: number = 0;
    const validators = (await api.query.validator.currentAuthorities()).toHuman();
    while(witnessHash.size == 0){
        const signedBlock = await api.rpc.chain.getBlock();
        if(signedBlock.block){
            currentBlockNumber = Number(signedBlock.block.header.number)
            console.log(currentBlockNumber)
        }
        
        signedBlock.block.extrinsics.forEach((ex: any, index: any) => {
            if (ex.toHuman().method.method === 'witnessAtEpoch') {
                const callData = ex.toHuman().method.args.call;
                if (callData && callData.section === 'ethereumChainTracking' && chain === "ETH") {
                    const finalData = callData.args;
                    // set priorityFee to 0, it is not kept into account for the chaintracking
                    finalData.new_chain_state.trackedData.priorityFee = '0';
                    const blockHeight = finalData.new_chain_state.blockHeight.replace(/,/g, '');
                    const baseFee = finalData.new_chain_state.trackedData.baseFee.replace(
                        /,/g,
                        '',
                    );
                    // parse the data and removed useless comas
                    finalData.new_chain_state.trackedData.baseFee = baseFee;
                    finalData.new_chain_state.blockHeight = blockHeight;
                    // create the extrinsic we need to witness (ETH chain tracking in this case)
                    const extrinsic = api.tx.ethereumChainTracking.updateChainState(
                        finalData.new_chain_state,
                    );
                    // obtain the hash of the extrinsic call
                    const blakeHash = blake2AsHex(extrinsic.method.toU8a(), 256);
                    witnessHash.add(blakeHash);
                }

                if (callData && callData.section === 'polkadotChainTracking' && chain === "DOT") {
                    const finalData = callData.args;
                    // set medianTip to 0, it is not kept into account for the chaintracking
                    finalData.new_chain_state.trackedData.medianTip = '0';
                    // parse the data and removed useless comas
                    const blockHeight = finalData.new_chain_state.blockHeight.replace(/,/g, '');
                    const runtimeVersion =
                        finalData.new_chain_state.trackedData.runtimeVersion.specVersion.replace(
                            /,/g,
                            '',
                        );
                    finalData.new_chain_state.trackedData.runtimeVersion.specVersion =
                        runtimeVersion;
                    finalData.new_chain_state.blockHeight = blockHeight;
                    // create the extrinsic we need to witness (DOT chain tracking in this case)
                    const extrinsic = api.tx.polkadotChainTracking.updateChainState(
                        finalData.new_chain_state,
                    );
                    // obtain the hash of the extrinsic call
                    const blakeHash = blake2AsHex(extrinsic.method.toU8a(), 256);
                    witnessHash.add(blakeHash);
                }

                if (callData && callData.section === 'bitcoinChainTracking' && chain === "BTC") {
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
                    const extrinsic = api.tx.bitcoinChainTracking.updateChainState(
                        finalData.new_chain_state,
                    );
                    // obtain the hash of the extrinsic call
                    const blakeHash = blake2AsHex(extrinsic.method.toU8a(), 256);
                    witnessHash.add(blakeHash);
                }
            }
        });
        await sleep(6000);
    }

    const epoch = Number(await api.query.validator.currentEpoch());
    const vanityNames = (await api.query.validator.vanityNames()).toHuman();
    const unsubscribe = await api.rpc.chain.subscribeNewHeads(async (header) => {

        //waiting at least 2 blocks to be sure that we give all validator enough time to witness something
        if (Number(header.number) - currentBlockNumber > 2) {
            unsubscribe();

            for (const elem of witnessHash) {
                let failingValidators = [];
                let votes;
                try{
                    votes = (
                        await api.query.witnesser.votes(epoch, elem)
                    ).toHuman();
                }catch(e) {
                    console.log("Failed to query the storage item with the provided hash!")
                }
                if (votes) {
                    let binary = hex2bin(votes.toString());
                    const witnessNumber = binary.match(/1/g)?.length;
                    // hashes are stored as 152 bits, the last 2 bits are always 0
                    while((binary.match(/0/g)?.length || 0)  >  2) {
                        let index = binary.indexOf("0");
                        binary = binary.substring(index + 1);
                        failingValidators.push(validators[index]);
                    }
                    console.log(`${witnessNumber}/${validators?.length} witnessed ${elem} hash!\nThe extrinsic was in block ${currentBlockNumber}`);
                    failingValidators.forEach(element => {
                        if(vanityNames[element] && vanityNames[element].substr(0,2) === "0x") {
                            let vanity = vanityNames[element].substr(2);
                            let bytes=[];
                            for(let i=0; i<vanity.length; i+=2 ){
                                bytes.push(parseInt(vanity.substr(i, 2), 16));
                            }
                            console.log(element+ " - " + String.fromCharCode(...bytes))
                        }else {
                            console.log(element + " - " + vanityNames[element]);
                        }
                    });
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


function hex2bin(hex: string) {
    hex = hex.replace('0x', '').toLowerCase();
    var out = '';
    for (var c of hex) {
        switch (c) {
            case '0':
                out += '0000';
                break;
            case '1':
                out += '0001';
                break;
            case '2':
                out += '0010';
                break;
            case '3':
                out += '0011';
                break;
            case '4':
                out += '0100';
                break;
            case '5':
                out += '0101';
                break;
            case '6':
                out += '0110';
                break;
            case '7':
                out += '0111';
                break;
            case '8':
                out += '1000';
                break;
            case '9':
                out += '1001';
                break;
            case 'a':
                out += '1010';
                break;
            case 'b':
                out += '1011';
                break;
            case 'c':
                out += '1100';
                break;
            case 'd':
                out += '1101';
                break;
            case 'e':
                out += '1110';
                break;
            case 'f':
                out += '1111';
                break;
            default:
                return '';
        }
    }

    return out;
}