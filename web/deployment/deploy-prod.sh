#!/bin/sh
exec "$(dirname -- "$0")/deploy.sh" prod "$@"
