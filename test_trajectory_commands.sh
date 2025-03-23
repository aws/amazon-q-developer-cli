#!/bin/bash
# Test script for trajectory commands

# Define colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Testing trajectory commands...${NC}"

# Create a test directory
TEST_DIR=~/workspace/q-agent-trajectory/test_run
mkdir -p $TEST_DIR

# Build the project
echo -e "${YELLOW}Building project...${NC}"
cargo build -p q_cli

# Run Amazon Q with trajectory recording enabled
echo -e "${YELLOW}Starting Amazon Q with trajectory recording...${NC}"
echo -e "${GREEN}Command: ./target/debug/q_cli chat --trajectory --trajectory-dir $TEST_DIR --auto-visualize${NC}"
echo -e "${YELLOW}When Amazon Q starts, run the following commands:${NC}"
echo -e "${GREEN}1. /trajectory status${NC}"
echo -e "${GREEN}2. /trajectory help${NC}"
echo -e "${GREEN}3. /trajectory checkpoint create initial_state${NC}"
echo -e "${GREEN}4. /trajectory checkpoint list${NC}"
echo -e "${GREEN}5. /trajectory visualize${NC}"
echo -e "${GREEN}6. /trajectory disable${NC}"
echo -e "${GREEN}7. /trajectory status${NC}"
echo -e "${GREEN}8. /trajectory enable${NC}"
echo -e "${GREEN}9. /trajectory checkpoint restore <ID>${NC} (Use the ID from step 4)"
echo -e "${GREEN}10. /quit${NC}"

# Start Amazon Q
./target/debug/q_cli chat --trajectory --trajectory-dir $TEST_DIR --auto-visualize
