#!/bin/bash
# Start knight-rider for TUI verification
set -e

cd "$(dirname "$0")/../packages/tui"

# Kill stale instances
for pid in $(lsof -ti:3001 2>/dev/null); do kill "$pid" 2>/dev/null; done
sleep 1

# Start knight-rider in background
nohup bun run knight-rider > /tmp/knight-rider.log 2>&1 &
echo $! > /tmp/knight-rider.pid
echo "Knight-rider PID: $(cat /tmp/knight-rider.pid)"

# Wait for boot
echo "Waiting for knight-rider to boot..."
for i in $(seq 1 40); do
  if curl -s http://localhost:3001/api/status 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); exit(0 if d.get('ready') else 1)" 2>/dev/null; then
    echo "Knight-rider ready!"
    curl -s http://localhost:3001/api/status | python3 -m json.tool
    exit 0
  fi
  sleep 2
done

echo "TIMEOUT - knight-rider did not become ready"
echo "Check /tmp/knight-rider.log for errors"
exit 1
