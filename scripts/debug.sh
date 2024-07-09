#!/bin/bash
set -e
set -o pipefail

if [ -z "$1" ]; then
  echo "Please provide URL"
  exit 1
fi

URL=$1

node build.js && node ./dist/index.js "$URL"
