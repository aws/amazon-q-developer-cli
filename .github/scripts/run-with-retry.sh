#!/usr/bin/env bash

set -euo pipefail

attempts=1
delay_seconds=15
label="command"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --attempts)
      attempts="$2"
      shift 2
      ;;
    --delay-seconds)
      delay_seconds="$2"
      shift 2
      ;;
    --label)
      label="$2"
      shift 2
      ;;
    --)
      shift
      break
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ $# -eq 0 ]]; then
  echo "no command provided" >&2
  exit 2
fi

if ! [[ "$attempts" =~ ^[0-9]+$ ]] || [[ "$attempts" -lt 1 ]]; then
  echo "attempts must be a positive integer" >&2
  exit 2
fi

if ! [[ "$delay_seconds" =~ ^[0-9]+$ ]] || [[ "$delay_seconds" -lt 0 ]]; then
  echo "delay-seconds must be a non-negative integer" >&2
  exit 2
fi

command=("$@")
attempt=1

while true; do
  echo "[$label] attempt $attempt/$attempts"
  if "${command[@]}"; then
    if [[ "$attempt" -gt 1 ]]; then
      echo "[$label] recovered on attempt $attempt/$attempts"
    fi
    exit 0
  else
    exit_code=$?
  fi

  if [[ "$attempt" -ge "$attempts" ]]; then
    echo "[$label] failed after $attempt/$attempts attempts" >&2
    exit "$exit_code"
  fi

  echo "[$label] retrying in ${delay_seconds}s" >&2
  sleep "$delay_seconds"
  attempt=$((attempt + 1))
done
