#!/usr/bin/env bash

trap 'yarn remove-host' SIGINT; { sleep 0.5 && yarn set-host; } & vite --port 3124