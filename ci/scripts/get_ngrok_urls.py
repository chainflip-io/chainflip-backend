"""
Helper Script to parse ngrok logs
and get the tunnel url for the chainflip node and polkadot node
"""
import os

# Get the path to the GitHub output file from the environment variables
github_output_path = os.getenv('GITHUB_OUTPUT')

chainflip_node_tunnel_logs = open('/tmp/ngrok-chainflip-node.log', encoding='utf-8').read()
polkadot_tunnel_logs = open('/tmp/ngrok-polkadot.log', encoding='utf-8').read()

chainflip_node_tunnel_url = [line for line in chainflip_node_tunnel_logs.split(
    '\n') if 'started tunnel' in line][-1].split('url=')[-1].strip().split('https://')[-1]
polkadot_node_tunnel_url = [line for line in polkadot_tunnel_logs.split(
    '\n') if 'started tunnel' in line][-1].split('url=')[-1].strip().split('https://')[-1]

polkadot_js_chainflip_node = f"https://polkadot.js.org/apps/?rpc=wss%3A%2F%2F{chainflip_node_tunnel_url}#/explorer"
polkadot_js_polkadot_node = f"https://polkadot.js.org/apps/?rpc=wss%3A%2F%2F{polkadot_node_tunnel_url}#/explorer"

print(
    f"💚 \033[1;33mPolkadotJS Chainflip Node URL:\033[0m {polkadot_js_chainflip_node}")
print(
    f"🧡 \033[1;33mPolkadotJS Polkadot Node URL:\033[0m {polkadot_js_polkadot_node}")

with open(github_output_path, 'a', encoding="utf-8") as f:
    f.write(
        f'ngrok_chainflip_node="{polkadot_js_chainflip_node}\n')
    f.write(
        f'ngrok_chainflip_node="{polkadot_js_polkadot_node}"\n')
