#!/usr/bin/env sh

docker_process_init_files() {
	echo
	local f
	for f; do
		case "$f" in
			*.sh)
				# https://github.com/docker-library/postgres/issues/450#issuecomment-393167936
				# https://github.com/docker-library/postgres/pull/452
				if [ -x "$f" ]; then
					echo "$0: running $f"
					"$f"
				else
					echo "$0: sourcing $f"
					. "$f"
				fi
				;;
			*)        echo "$0: ignoring $f" ;;
		esac
		echo
	done
}
[ -d "/docker-entrypoint-init.d" ] && docker_process_init_files /docker-entrypoint-init.d/*

echo "Installing dependencies before command" >> /dev/stdout
poetry install

# Resume to "command"
echo "Executing command" >> /dev/stdout
echo "$@" >> /dev/stdout
exec "$@"