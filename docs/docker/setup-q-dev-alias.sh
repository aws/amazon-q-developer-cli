#!/usr/bin/env bash
#
# setup-q-dev-alias.sh
#
# This script creates shell aliases for running Amazon Q CLI in Docker,
# with proper directory mapping for either macOS or Linux hosts.
# It uses Docker volumes for persistent storage instead of direct host mapping.
#
# Usage:
#   source ./setup-q-dev-alias.sh           # To add alias to current session
#   ./setup-q-dev-alias.sh                  # To add alias to shell config only
#   ./setup-q-dev-alias.sh --rebuild        # Force rebuild the Docker image
#   source ./setup-q-dev-alias.sh --rebuild # Add alias and rebuild Docker image
#
# After running, you'll have the 'q-dev' alias available in your shell.

# Detect if the script is being sourced or executed directly
(return 0 2>/dev/null) && sourced=true || sourced=false

# Check for rebuild flag
force_rebuild=false
for arg in "$@"; do
  if [ "$arg" = "--rebuild" ]; then
    force_rebuild=true
  fi
done

# Detect operating system and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

# Map architecture to Docker platform
if [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
  DOCKER_ARCH="arm64"
elif [ "$ARCH" = "x86_64" ]; then
  DOCKER_ARCH="amd64"
else
  echo "⚠️  Unsupported architecture: $ARCH. Defaulting to amd64."
  DOCKER_ARCH="amd64"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Print header
echo "🚀 Setting up Amazon Q Developer Docker aliases"
echo "==============================================="
echo "🖥️  Detected architecture: $ARCH (using Docker platform: linux/$DOCKER_ARCH)"

# Check if Docker is installed
if ! command -v docker &>/dev/null; then
  echo "❌ Error: Docker is not installed or not in PATH"
  echo "Please install Docker first: https://docs.docker.com/get-docker/"
  if [ "$sourced" = true ]; then
    return 1
  else
    exit 1
  fi
fi

# Create directories if they don't exist
echo "📁 Creating required directories..."

# AWS directory structure (common for both OS)
mkdir -p "${HOME}/.aws/amazonq/profiles"

# Create Docker volumes if they don't exist
echo "📦 Setting up Docker volumes for Amazon Q data..."
if ! docker volume inspect amazon-q-data &>/dev/null; then
  docker volume create amazon-q-data
  echo "Created volume: amazon-q-data"
fi

if ! docker volume inspect amazon-q-cache &>/dev/null; then
  docker volume create amazon-q-cache
  echo "Created volume: amazon-q-cache"
fi

# Create a persistent home directory volume if it doesn't exist
if ! docker volume inspect amazon-q-home &>/dev/null; then
  echo "📦 Creating persistent home directory volume..."
  docker volume create amazon-q-home
  echo "Created volume: amazon-q-home"
fi

# Define the alias with Docker volumes including persistent home directory
ALIAS_DEFINITION="alias q-dev='docker run -itq --rm \\
  -v \"\$(pwd):/home/dev/src\" \\
  -v \"\${HOME}/.aws/credentials:/home/dev/.aws/credentials:ro\" \\
  -v \"\${HOME}/.aws/config:/home/dev/.aws/config:ro\" \\
  -v \"\${HOME}/.aws/amazonq:/home/dev/.aws/amazonq:rw\" \\
  -v amazon-q-data:/home/dev/.local/share/amazon-q \\
  -v amazon-q-cache:/home/dev/.cache/amazon-q \\
  -v amazon-q-home:/home/dev \\
  -v \"\${HOME}/.gitconfig:/home/dev/.gitconfig:ro\" \\
  -v \"\${HOME}/.ssh:/home/dev/.ssh:ro\" \\
  -e AWS_PROFILE \\
  -e AWS_REGION \\
  -e TZ \\
  amazon-q-dev'"

# Add the alias to the current shell session if sourced
if [ "$sourced" = true ]; then
  eval "$ALIAS_DEFINITION"
  echo "✅ Alias 'q-dev' added to current shell session"
fi

# Detect shell and add alias to the appropriate config file
SHELL_NAME="$(basename "$SHELL")"
ALIAS_ADDED=false

if [ "$SHELL_NAME" = "bash" ]; then
  if [ -f "$HOME/.bashrc" ]; then
    if ! grep -q "alias q-dev=" "$HOME/.bashrc"; then
      echo -e "\n# Amazon Q Developer Docker alias\n$ALIAS_DEFINITION" >>"$HOME/.bashrc"
      ALIAS_ADDED=true
    fi
  fi
elif [ "$SHELL_NAME" = "zsh" ]; then
  if [ -f "$HOME/.zshrc" ]; then
    if ! grep -q "alias q-dev=" "$HOME/.zshrc"; then
      echo -e "\n# Amazon Q Developer Docker alias\n$ALIAS_DEFINITION" >>"$HOME/.zshrc"
      ALIAS_ADDED=true
    fi
  fi
fi

# Build or rebuild Docker image if needed
build_image() {
  if [ -f "$SCRIPT_DIR/Dockerfile" ]; then
    echo "🔨 Building Docker image from $SCRIPT_DIR/Dockerfile for architecture: $DOCKER_ARCH..."
    docker build --build-arg ARCH=$DOCKER_ARCH -t amazon-q-dev -f "$SCRIPT_DIR/Dockerfile" "$SCRIPT_DIR"
    if [ $? -eq 0 ]; then
      echo "✅ Docker image built successfully"
    else
      echo "❌ Failed to build Docker image"
      return 1
    fi
  else
    echo "❌ Dockerfile not found in $SCRIPT_DIR"
    echo "Please create a Dockerfile first using the template from the documentation."
    return 1
  fi
  return 0
}

# Check if Docker image exists or if rebuild is forced
if [ "$force_rebuild" = true ]; then
  echo "🔄 Forcing rebuild of Docker image..."
  build_image
elif ! docker image inspect amazon-q-dev &>/dev/null; then
  echo "⚠️  The 'amazon-q-dev' Docker image doesn't exist yet."
  echo "Would you like to build it now? (y/n)"
  read -r BUILD_RESPONSE

  if [[ "$BUILD_RESPONSE" =~ ^[Yy]$ ]]; then
    build_image
  else
    echo "📝 You can build the image later with:"
    echo "  docker build --build-arg ARCH=$DOCKER_ARCH -t amazon-q-dev -f $SCRIPT_DIR/Dockerfile $SCRIPT_DIR"
    echo "  or run this script with the --rebuild flag"
  fi
fi

# Print success message
echo "✅ Setup complete!"
if [ "$ALIAS_ADDED" = true ]; then
  echo "The 'q-dev' alias has been added to your shell configuration."
  if [ "$sourced" = false ]; then
    echo "To use it in this terminal session, either:"
    echo "  1. Run 'source ~/.${SHELL_NAME}rc'"
    echo "  2. Start a new terminal session"
  fi
else
  if [ "$sourced" = true ]; then
    echo "The 'q-dev' alias is available for this terminal session."
  else
    echo "The alias was already in your shell configuration."
    echo "To use it in this terminal session, run: source ~/.${SHELL_NAME}rc"
  fi
fi

echo ""
echo "📋 Usage:"
echo "  q-dev                   # Start Amazon Q chat in the current directory"
echo "  q-dev --help            # Show Amazon Q CLI help"
echo "  q-dev --entrypoint bash # Start a bash shell instead of Q chat"
echo ""
echo "📦 Docker Volumes:"
echo "  amazon-q-data          # Persistent storage for Amazon Q data"
echo "  amazon-q-cache         # Persistent storage for Amazon Q cache"
echo "  amazon-q-home          # Persistent home directory for user customizations"
echo ""
echo "🔧 Troubleshooting:"
echo "  If you encounter database errors, you can reset the volumes with:"
echo "  docker volume rm amazon-q-data amazon-q-cache && docker volume create amazon-q-data && docker volume create amazon-q-cache"
echo ""
echo "  If you want to reset your home directory customizations:"
echo "  docker volume rm amazon-q-home && docker volume create amazon-q-home"
echo ""
echo "For more information, see the documentation in:"
echo "  $SCRIPT_DIR/running-in-docker.md"

# Exit with success if executed directly
if [ "$sourced" = false ]; then
  exit 0
fi
