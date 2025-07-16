#!/usr/bin/env -S pnpm tsx

import { PublicKey } from '@solana/web3.js';
import { decodeSolAddress, runWithTimeoutAndExit } from 'shared/utils';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';

async function forceRecoverSolNonce(nonceAddress: string, nonceValue: string) {
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey(nonceAddress).toBase58()),
      decodeSolAddress(new PublicKey(nonceValue).toBase58()),
    ),
  );
}

async function main() {
  await forceRecoverSolNonce(
    '2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw',
    'ATtt4cicTHjhUoqAR1gazU6JdQGLKSNqn7BSvveWp14m',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    'CKgsJpX1zE4AMByWV1mH1DLHWfi92aBXEPLnTbU7gvcU',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    'GTVTpZBaZiwbpW6eVEEvhXqC7RXkvjruuiKgxvqxiSAg',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    '71sH3oVczEMZhNHkFbDNziNtQLJSmuoUtJZPZKCCiXBA',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    '9ccnmA4SE4TAasCjLwHHnzZQ8YcvtvmtesEsGuBMh4mg',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    '5VHtSfveFNXyX1CbNNjcqPPsax331UN3PJg6qpM3Mph',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    'AbNrs22FNQEL869JfCfaBGYniUhyCq57zkyD8Kdy5Qz9',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    'G2dNcTTqMATa5AEp97Vwy6ESY2ceEgGwbZ5Av9emcuD5',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    'C5XRoKQx9uRnz7XqQutqWmCxPRAfbVUMQWCE8WECbEp8',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    '5jStixd7ve8UMto72nnvyj3S76mV3673BvT1ejK9U1yA',
  );
  await forceRecoverSolNonce(
    'BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES',
    'Ei26neh7hgBaG53pb9BUAJysCbgwWFzepcqbyia9GKTa',
  );
  await forceRecoverSolNonce(
    'Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH',
    'EbWq3dgSjaa8pX3YeXVHopANoHCAsEkXrbALMjndbvr1',
  );
  await forceRecoverSolNonce(
    '3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W',
    '64hFuYc2RjeDLWAatbnbD3XCWgRBgbHLxCjfeNxx1G5e',
  );
  await forceRecoverSolNonce(
    '9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S',
    'GALf62D4Km2XEbHswmuibs9QYRHWpgnDrTT4mUZWHGqn',
  );
  await forceRecoverSolNonce(
    'AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z',
    'EzoN8wxWK9VWSV4dLWcpZCVhvztV5W5GCWKSak4b9C4b',
  );
  await forceRecoverSolNonce(
    'APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS',
    '54s775GHBp5rD4CHCjgtTgR5YbrRHRuXiskd4APXnPuj',
  );
  await forceRecoverSolNonce(
    '4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4',
    '8nyoiZ5zXQPDagxKHozWxk5zsMkMPpS5vU1KHcASc4tH',
  );
  await forceRecoverSolNonce(
    'FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8',
    'G4GMjYZR5rbwATGXt1F5EfwzJJh4xTtUvbBA7nQmStVt',
  );
  await forceRecoverSolNonce(
    'ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP',
    'E9aQyeBWF8pXJrp4aaa97RssiE8SinBP4sofxLuYGbWv',
  );
  await forceRecoverSolNonce(
    'Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K',
    'FYKwioMchMvi8uMW8dkhsivxDhL9NSfeeRQnvtTpb8RT',
  );
  await forceRecoverSolNonce(
    '2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8',
    '3MebcPyKDoWdNjBDR5GiMqoxk6giKghdo7yTVDRDP9dH',
  );
  await forceRecoverSolNonce(
    'HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm',
    'Fymdispqod8j9so1fA5w6QRUXSf7qZLVUtwXRN3inwaD',
  );
  await forceRecoverSolNonce(
    'DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM',
    'ADYS5N8F3UtHqPwkbHMmQviwgj6pXUWY9MeHBFgbtNJT',
  );
  await forceRecoverSolNonce(
    'HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw',
    'CZp2E3hnb9qsgoeE1nDc14wwkig5TXEDhT75BY89grnx',
  );
  await forceRecoverSolNonce(
    'FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr',
    'FXBSFEozmiXCb7BoYMYPmthJTbRE9VULEbhvPfmpzarG',
  );
  await forceRecoverSolNonce(
    'AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm',
    'CiwZV7WLhuDDRPFz372V1HadRJ8yioRk9TkPNftvhUe2',
  );
  await forceRecoverSolNonce(
    '5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi',
    'Cx7GTwXmBSERUDJbUSxTefmqfG36TVHgL1JEMPBPzpyZ',
  );
  await forceRecoverSolNonce(
    'Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk',
    'D1R5p7S3zm23WxbJWUUP5WTrRjSaCs4T7QQL1Em2niTg',
  );
  await forceRecoverSolNonce(
    '4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9',
    'A1Vzqynswr4Hi57T6uuc9jKemhQB1Z2bPKpQTPwpU9rj',
  );
  await forceRecoverSolNonce(
    'BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj',
    '13MfbQex4kW8eChdPhcbnZKDif8ZWpEvpxhUA61rLG6X',
  );
  await forceRecoverSolNonce(
    'EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN',
    'GbSUjgz6xGwpiusgCMRyoGdsp5B1U9txqCYmjKtKPumo',
  );
  await forceRecoverSolNonce(
    'BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm',
    'B2No3zYa3QFMbG1LkfDDqekXqmQWn4C5djvdzurJb6Bx',
  );
  await forceRecoverSolNonce(
    'EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv',
    '546nX7nm3PMdZwq9caYbR4VtMyU8UHHHS9PmzWyBJ1Z1',
  );
  await forceRecoverSolNonce(
    'CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj',
    '6idmaUzkSZ5z8ovoVhzYLKjrhpSVrT7Bm8zs8eHroy4H',
  );
  await forceRecoverSolNonce(
    'zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX',
    'GfqfzAtoWSP1cahryXVKQ7opER7RvM8Q3ELqFpvrnmQN',
  );
  await forceRecoverSolNonce(
    '2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6',
    '2kahYJmdPjy1g6f566ZDMkhAumLDRukjiGPWoq7PMoa7',
  );
  await forceRecoverSolNonce(
    'A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd',
    'FbqNU1MDXbcxb85zPD4CoRJcDx3Q3skkpzv7U9dSZPKp',
  );
  await forceRecoverSolNonce(
    '9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC',
    '9tbBPi74uZ2aWm33bpwvL5ge2bane6MVz8mLuiA79KEK',
  );
  await forceRecoverSolNonce(
    '3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C',
    'tuv4XtrVATYpfzmkS2enymAji48BfH9TXAFehww8Mow',
  );
  await forceRecoverSolNonce(
    '7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu',
    'J3JTYrjMzCU3MnH2jEKrx3SqM7XWWNhLycHivqVjfbGN',
  );
  await forceRecoverSolNonce(
    '4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9',
    '52TTBZfoc4pfbfHxmff5HeJms8P6JDeJseFiZmqpqxJs',
  );
  await forceRecoverSolNonce(
    'ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu',
    '7UVTKi2dvGuRhCgedJ3UqmQ3qAx4JSiwDkPfJaAhZ2mp',
  );
  await forceRecoverSolNonce(
    'Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ',
    'FUyToeoynggaYxfkXnPcqWV2nw85B5TrV7QowEMwkPEc',
  );
  await forceRecoverSolNonce(
    'GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3',
    'GozUYbpAgoFkv5KKMQGe9jAzqU9YPzGbtEniDD3V9xZ8',
  );
  await forceRecoverSolNonce(
    '2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u',
    '3qRw7McbQBKazULB2HSuFSvxBZJERWBQbTvcQuyeCGqp',
  );
  await forceRecoverSolNonce(
    'CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW',
    'BV7c7CUU7VXSBsWFJ71dVBFwGHevZDpb95cb6y4o7isM',
  );
  await forceRecoverSolNonce(
    'Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU',
    'Yqr8Cg9XTkDiqdpTde3sNtdbsSJ3NxmfvHQJhmfmpCe',
  );
  await forceRecoverSolNonce(
    'GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W',
    'CFQLVBG9Kh4uLNtegTuja6smbNCFDhQzh3utaoqmti7z',
  );
  await forceRecoverSolNonce(
    '7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c',
    'Ctb9q1xjptQAQHvf8R9DgYw8NYrCzMwyGNtYKkZwiy8U',
  );
  await forceRecoverSolNonce(
    'EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS',
    '97q7MC5D7spBAaqP5aa4TBA25xdQGYBG7JxPc34auYzK',
  );
}

await runWithTimeoutAndExit(main(), 60);
