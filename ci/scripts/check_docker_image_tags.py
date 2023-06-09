"""Helper script to check if the docker images used locally match the ones used in GitHub Actions"""
import sys
import yaml

EXIT_CODE = 0

services = {
    "geth": "services.geth.image",
    "polkadot": "services.polkadot.image",
    "bitcoin": "services.bitcoin.image"
}

with (
    open("./localnet/docker-compose.yml", 'r', encoding="utf-8") as docker_compose_file,
    open(".github/workflows/_40_post_check.yml", 'r', encoding="utf-8") as github_actions_file
):
    docker_compose = yaml.safe_load(docker_compose_file)
    github_actions = yaml.safe_load(github_actions_file)
    for service, path in services.items():
        path_parts = path.split('.')
        docker_image = docker_compose
        for part in path_parts:
            docker_image = docker_image.get(part, None)
        github_image = github_actions['jobs']['bouncer']['services'].get(
            service, {}).get('image', None)
        if docker_image != github_image:
            error_message = f"""🚨 \033[1;31m{service} docker image mismatch!\033[0m\n\033[1;33mLocal:\033[0m {docker_image}\n\033[1;33mGitHub:\033[0m {github_image}"""
            print(error_message)
            EXIT_CODE = 1
        else:
            print(
                f"🎉 \033[1;32m{service}: Docker image tags match!\033[0m")
    sys.exit(EXIT_CODE)
