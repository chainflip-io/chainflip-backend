import json
import sys
import os
import stat
import subprocess

BOOTNODES_BASE_PATH = "state-chain/node/bootnodes/"
CHAINSPECS_BASE_PATH = "state-chain/node/chainspecs/"

def set_executable_permission(file_path):
    """Set executable permission for a file."""
    os.chmod(file_path, stat.S_IXUSR)

def get_file_content(file_path):
    """Read and return file content."""
    with open(file_path, "r") as file_data:
        return file_data.read().splitlines()

def update_chainspec_with_bootnodes(chainspec_path, bootnodes):
    """Load chainspec, update with bootnodes, and write back."""
    with open(chainspec_path, "r") as chainspec:
        chainspec_data = json.load(chainspec)

    chainspec_data["bootNodes"] = bootnodes

    with open(chainspec_path, "w") as chainspec:
        json.dump(chainspec_data, chainspec, indent=4)

def main():
    if len(sys.argv) != 3:
        print("Usage: <script> <network> <binary>")
        sys.exit(1)

    network = sys.argv[1]
    binary = sys.argv[2]

    # Ensure binary is executable
    set_executable_permission(binary)

    bootnodes_filename = os.path.join(BOOTNODES_BASE_PATH, f"{network}.txt")
    bootnodes = get_file_content(bootnodes_filename)

    chainspec_filename = os.path.join(CHAINSPECS_BASE_PATH, f"{network}.chainspec.json")

    if network == "test":
        chainspec_name = network
    else:
        chainspec_name = network + "-new"

    generate_chainspec_command = [binary, "build-spec", "--chain", chainspec_name, "--disable-default-bootnode"]

    with open(chainspec_filename, "w") as chainspec:
        subprocess.call(generate_chainspec_command, stdout=chainspec)

    update_chainspec_with_bootnodes(chainspec_filename, bootnodes)

    chainspec_filename_raw = os.path.join(CHAINSPECS_BASE_PATH, f"{network}.chainspec.raw.json")
    generate_chainspec_raw_command = [binary, "build-spec", "--chain", chainspec_filename, "--raw", "--disable-default-bootnode"]

    with open(chainspec_filename_raw, "w") as chainspec_raw:
        subprocess.call(generate_chainspec_raw_command, stdout=chainspec_raw)

if __name__ == "__main__":
    main()
