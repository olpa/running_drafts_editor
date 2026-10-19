#!/bin/sh

set -eu

deployment_root=/rnotes/RUNNING_DRAFTS_FRONTEND
source_root=/mnt/HC_Volume_103849597/home/olpa/running_drafts_editor/web
compose_file="$source_root/compose.deployment.yaml"

select_target() {
  target=${1:?usage: $0 dev|prod}
  case "$target" in
    dev)
      compose_project=running-drafts-frontend-dev
      ;;
    prod)
      compose_project=running-drafts-frontend-prod
      ;;
    *)
      echo "error: target must be dev or prod" >&2
      exit 2
      ;;
  esac
  env_file="$deployment_root/.env.$target"

  if [ ! -f "$compose_file" ]; then
    echo "error: deployment Compose file not found: $compose_file" >&2
    exit 1
  fi
  if [ ! -f "$env_file" ]; then
    echo "error: target environment file not found: $env_file" >&2
    exit 1
  fi
  if [ "${IMAGE_TAG+x}" = x ]; then
    echo "error: IMAGE_TAG must be selected only in $env_file" >&2
    exit 1
  fi

  image_tag=$(sed -n 's/^IMAGE_TAG=//p' "$env_file")
  case "$image_tag" in
    ''|*[!A-Za-z0-9._-]*)
      echo "error: IMAGE_TAG in $env_file is missing or invalid" >&2
      exit 1
      ;;
  esac
  if [ "$(grep -c '^IMAGE_TAG=' "$env_file")" -ne 1 ]; then
    echo "error: $env_file must contain exactly one IMAGE_TAG" >&2
    exit 1
  fi
}

compose() {
  docker compose \
    --project-name "$compose_project" \
    --project-directory "$deployment_root" \
    --env-file "$env_file" \
    --file "$compose_file" \
    "$@"
}
