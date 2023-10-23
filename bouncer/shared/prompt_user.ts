import { createInterface } from 'readline/promises';

export async function promptUser(prompt: string) {
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  try {
    const completePrompt = prompt + '\nEnter to continue. Ctrl + C to exit.\n';
    const ans = await rl.question(completePrompt);
    rl.close();
    return ans;
  } finally {
    rl.close();
  }
}
