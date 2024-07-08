#!/bin/bash
set -e
set -o pipefail

ACTION="go"

while [[ $# -gt 0 ]]; do
  case $1 in
    -i|--index)
      INDEX=$2
      shift
      shift
      ;;
    *)
      echo "Invalid option: $1"
      exit 1
      ;;
  esac
done

node build.js && node inspect ./dist/index.js "$ACTION"
