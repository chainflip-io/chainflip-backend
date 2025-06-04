#!/usr/bin/env -S pnpm tsx

import { PublicKey } from '@solana/web3.js';
import { decodeSolAddress, runWithTimeoutAndExit } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

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
    '8jZzQFX8HJhpsdbwipxUxsPNzer8JbwyidYo8Yw46B9R',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    '2naN2rfRjSHVm7J1Ki3giptXvJJXgrhWrejLvkwRouHr',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    'ErD28ThCuMi8VEowaF7MNZ8dUJZQntmi54pgf3Bz5f2x',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    'BsriTrgZVizCa98vhBvWQA7waCAPwSsStVKSSEi6rro7',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    'ztCrQavAALbUT7sFe7tCNn66gPE8echPKFvkhM9rz8W',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    'PMCoRDdHn6qFCof6wyMiCFYDrQ8xRwQ6aYrCTHuqwhj',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    '3VF2i3EUy6vKH8MK4DcWXPj5ZHCKrY4kqoLgJ1pBY4cB',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    '9thoEW7oXNxsDSSj9emUS9pQKdfXbnioxNp9pREdFqMo',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    'DDSzgNxmHBY6kqe7Hzn9UfP8r6LyeEtB44cPLRsQDgvH',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    '6mV4bMu6gh8RU9S1Wv11gLaPnCMk4RFsaisXMHnkc7nb',
  );
  await forceRecoverSolNonce(
    'BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES',
    'A24bFcz14iKtgCwNQqZBEtMPowH5vyFY1r7YRq2p5NiR',
  );
  await forceRecoverSolNonce(
    'Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH',
    'EXzs7fuyvvrGabvD7XUKY6XBFtTZbmec7g4LAKKaiaHJ',
  );
  await forceRecoverSolNonce(
    '3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W',
    'J9NG3Df5kP27QcExFuqLZdFFkW7dz5n42PbXEWYQ1ANH',
  );
  await forceRecoverSolNonce(
    '9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S',
    'BU81wfrcKav5haK5v5Efj8cuTZ1PwgpmaKDbjdQa1Dyx',
  );
  await forceRecoverSolNonce(
    'AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z',
    'EnTWLd8Zyggpm2acQp5XLPFTfDrLTWHSUZAkksvQbvc7',
  );
  await forceRecoverSolNonce(
    'APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS',
    'HvzRYLW2QNvDuUEMWKA6PiLjzqFaygBd85NYG9K5K3jw',
  );
  await forceRecoverSolNonce(
    '4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4',
    '72uTmK3TxW9gBaCcP3QX8mzx7YAw7tD2Zdvigs6bCDZg',
  );
  await forceRecoverSolNonce(
    'FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8',
    '9oCg6x2TBRYozryqpexTtjW1uLAiQX268ge3yAwtGTjq',
  );
  await forceRecoverSolNonce(
    'ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP',
    '5Cd8fyfeUHaK4Bwpe1TZDUYoxMH5WyJQR5JhQwDQApUf',
  );
  await forceRecoverSolNonce(
    'Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K',
    '9utKvF2yumPhcLmiShQXXVVGqZam6Z2qvRSQPVKsxc4n',
  );
  await forceRecoverSolNonce(
    '2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8',
    'GP74bXQ9u8CL4PGvWVKBhBhNka5fpBtWC1oK4pwLykVQ',
  );
  await forceRecoverSolNonce(
    'HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm',
    'Fj2bEjshjzskbcHfCsyUL8degJ2xAMdvxmq8EUuuE6n7',
  );
  await forceRecoverSolNonce(
    'DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM',
    'BHm6tBxwicd8VZGN8jcphv2ehw1Cwt9QtQJ4rSt5Y7KF',
  );
  await forceRecoverSolNonce(
    'HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw',
    'FHgDaa9xRDvZ46WYjzspgLgQveif7WMpbyund5PcjjsL',
  );
  await forceRecoverSolNonce(
    'FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr',
    '79k3TjCRCmEp1LgzQrhUUYZDvQ5z4gSN256CD8PDWZcE',
  );
  await forceRecoverSolNonce(
    'AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm',
    'trFaCAy3atGaG2cu7AEccS86CY4ztxnEuGCgijjJCPZ',
  );
  await forceRecoverSolNonce(
    '5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi',
    '3BZKWDtGxdnyLhJhnR12EayUxEnZ6Cpy4oRjruDKFFYu',
  );
  await forceRecoverSolNonce(
    'Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk',
    '8ZQSdxKkVUrEXYYJagbiP28P1yd9NYKvaPLqpNtSKh4',
  );
  await forceRecoverSolNonce(
    '4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9',
    'HnpjxDcLeYYpRTsdcwAmMUR5nAGcsy9XdEg7XszYx2FD',
  );
  await forceRecoverSolNonce(
    'BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj',
    'CZZPxD7N8Nw7Qyytmk2VQsJAy21mLADVjhJtcEPJtdpB',
  );
  await forceRecoverSolNonce(
    'EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN',
    '9Mvhn3brEfXv4Q3gK8vNteKGNMznQ3KTiB3RrSYj5VR4',
  );
  await forceRecoverSolNonce(
    'BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm',
    'FxyD43vQ12s8j4vtyAzCiMCEHzPDmdYVMdR9iuLckvw6',
  );
  await forceRecoverSolNonce(
    'EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv',
    'DdeHYK8U9adeUgW7B2cgBawpGtp91UEbb8TeA3YAzbFa',
  );
  await forceRecoverSolNonce(
    'CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj',
    '7bh7wQXec1UaQ8ZKqazTFTGtuqsnMLQ3Wh2zoGqanYiJ',
  );
  await forceRecoverSolNonce(
    'zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX',
    '4imnTJ6oabxQu54Q99heSkc89pzwXdyGvog9s8yLEaXz',
  );
  await forceRecoverSolNonce(
    '2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6',
    'ADhP2avpHrBXxbx6XZW7saGFgFBNEMB17nicBW7M1U3L',
  );
  await forceRecoverSolNonce(
    'A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd',
    'e23fwmFwgC1iw8aFt3C8XF7QdXjkuqgGbzfAA5tcP8b',
  );
  await forceRecoverSolNonce(
    '9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC',
    'tsv2JrbcSNYdgw21qqECU63JFaPBFnKHVsnP8tBYQfY',
  );
  await forceRecoverSolNonce(
    '3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C',
    '9yQdGaonrCLPVmL9QC8vZjEayHXjb2B46qkpZ7LSe9pr',
  );
  await forceRecoverSolNonce(
    '7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu',
    '3Tdd8iJkGwspwrhZDvtxk3q1Uy6HSqYrEYzt7k6vhtQq',
  );
  await forceRecoverSolNonce(
    '4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9',
    '9Q1AZYB2hDeqUfiJ7YLHrVD6gq9rRUFKhq584y1zasHb',
  );
  await forceRecoverSolNonce(
    'ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu',
    '7m2wExd7WXwiixkdBv44SdWP4UB5yX9Pw6yqPmqC48Y2',
  );
  await forceRecoverSolNonce(
    'Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ',
    '71FqY55d2uCQznrReH1GWU8KiDcHfu8yNoZ8vR3VgHnq',
  );
  await forceRecoverSolNonce(
    'GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3',
    '54aMDuTuRwLz4vjhDHfhKwKTYRtmBbGEKFMPiVd4eWDN',
  );
  await forceRecoverSolNonce(
    '2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u',
    '3KgGHSuoBWADkr3owkBmiL8kLk8GzittP5fRUYdbQ2hu',
  );
  await forceRecoverSolNonce(
    'CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW',
    'C2TtJAPBrDH9YqvweDfXKFGmZS5Ghh1vQ4upNnJkwYGZ',
  );
  await forceRecoverSolNonce(
    'Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU',
    '3BhCroBwQktvhUwMCjLf4FX4WFPqfkrt9uvCswuwMYhj',
  );
  await forceRecoverSolNonce(
    'GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W',
    '3Fc1pTpsuxBr9FDWnjYgDESFvjdU7ZVnzh2Wp92iF1WA',
  );
  await forceRecoverSolNonce(
    '7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c',
    'DP6YNR1Gtfu1hM9f9D3fH36hH352tQFsKBzVHFVMLHoe',
  );
  await forceRecoverSolNonce(
    'EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS',
    '2Hru4zGbHb3kyBywARbaCqEmCH9em83Pirdq2mGUcGMH',
  );
}

await runWithTimeoutAndExit(main(), 60);
