#!/bin/sh

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
# shellcheck source=deployment/common.sh
. "$script_dir/common.sh"
select_target "${1:?usage: $0 dev|prod [compose-up-options...]}"
shift

compose config --quiet
if ! docker image inspect "running-drafts-frontend:$image_tag" >/dev/null 2>&1; then
  echo "error: image running-drafts-frontend:$image_tag is not present" >&2
  exit 1
fi
compose up --detach --no-build --force-recreate "$@" web
