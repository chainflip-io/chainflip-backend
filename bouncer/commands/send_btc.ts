import { sendBtc } from '../shared/send_btc';

const bitcoin_address = process.argv[2];
const btc_amount = parseFloat(process.argv[3]);

async function sendBitcoin() {
  try {
    await sendBtc(bitcoin_address, btc_amount);
    process.exit(0);
  } catch (error) {
    console.log(`ERROR: ${error}`);
    process.exit(-1);
  }
}

sendBitcoin();
