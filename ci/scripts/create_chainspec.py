import json
import sys
import os
import stat
import subprocess

bootnodes_base_path = "state-chain/node/bootnodes/"
chainspecs_base_path = "state-chain/node/chainspecs/"

# Set the first argument to variable network
network = sys.argv[1]

# Set the path of a binary file to variable binary
binary = sys.argv[2]

os.chmod(binary, stat.S_IXUSR)

bootnodes_filename = bootnodes_base_path + network + ".txt"

# Save contents of bootnodes file to variable
with open(bootnodes_filename, "r") as bootnodes_data:
    bootnodes = bootnodes_data.read().splitlines()

chainspec_filename = chainspecs_base_path + network + ".chainspec.json"

generate_chainspec_subprocess=[binary, "build-spec", "--chain", network, "--disable-default-bootnode"]
with open(chainspec_filename, "w") as chainspec:
    subprocess.call(generate_chainspec_subprocess, stdout=chainspec)

# Load the chainspec file
with open(chainspec_filename, "r") as chainspec:
    chainspec_data = json.load(chainspec)

chainspec_data["bootNodes"] = bootnodes
with open(chainspec_filename, "w") as chainspec:
    json.dump(chainspec_data, chainspec, indent=4)

chainspec_filename_raw = chainspecs_base_path + network + ".chainspec.raw.json"

generate_chainspec_raw_subprocess=[binary, "build-spec", "--chain", chainspec_filename, "--raw", "--disable-default-bootnode"]
with open(chainspec_filename_raw, "w") as chainspec_raw:
    subprocess.call(generate_chainspec_raw_subprocess, stdout=chainspec_raw)
