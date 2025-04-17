# Running Amazon Q CLI in Docker

This guide explains how to run Amazon Q CLI Chat in a Docker container for development and testing.

## Quick Setup

The easiest way to get started is to use our setup script:

```bash
# Download the setup script
curl -O https://raw.githubusercontent.com/aws/amazon-q-developer-cli/main/docs/docker/setup-q-dev-alias.sh

# Make it executable
chmod +x setup-q-dev-alias.sh

# Source it to add the alias to your current session
source ./setup-q-dev-alias.sh
```

This will:
1. Create necessary Docker volumes
2. Build the Docker image (if it doesn't exist)
3. Add a `q-dev` alias to your shell

## Using Amazon Q in Docker

After setting up, you can use Amazon Q CLI in Docker:

```bash
# Navigate to your project directory
cd ~/my-project

# Start Amazon Q chat
q-dev

# Or run a specific command
q-dev --help
```

## Common Commands

| Command | Description |
|---------|-------------|
| `q-dev` | Start Amazon Q chat in the current directory |
| `q-dev --help` | Show Amazon Q CLI help |
| `q-dev --entrypoint bash` | Start a bash shell instead of Q chat |

## Development Workflow

### Understanding Container Context

When using Amazon Q in Docker, it's important to understand what context is available:

- **Only the current directory** is mounted in the container
- Amazon Q can only see and access files in your current directory (and subdirectories)
- System-wide tools and dependencies inside the container may differ from your host machine
- The container runs Ubuntu Linux, regardless of your host OS (macOS, Windows, etc.)

### Best Practices for Development

1. **Always navigate to your project root** before running `q-dev`
   ```bash
   cd ~/path/to/project-root
   q-dev
   ```

2. **Add context from your project files**
   ```
   /context add README.md
   /context add src/
   ```

3. **For larger codebases**, be selective about which files you add to context
   ```
   /context add src/main.py
   /context add src/important_module/
   ```

4. **Use the container's bash shell** to explore or modify the environment
   ```bash
   q-dev --entrypoint bash
   ```

5. **Install additional dependencies** in the container if needed
   ```bash
   q-dev --entrypoint bash
   pip install some-package
   ```
   Note: These changes will persist between container sessions thanks to the persistent home volume

### Working with Different Languages

The container comes with several language runtimes pre-installed:

- **Python**: Use with `python` or `uv`
- **Node.js**: Use with `node` or `npm`
- **Java**: Available with `java` and `javac`
- **Go**: Available with `go`
- **AWS CLI**: Available with `aws`
- **Git**: Available for version control

Example workflows:

**Python project:**
```bash
# Start a bash shell in the container
q-dev --entrypoint bash

# Create and run a Python script
echo 'print("Hello from container")' > test.py
python test.py
```

**Node.js project:**
```bash
# Initialize a Node.js project
q-dev --entrypoint bash
npm init -y
npm install express
echo 'console.log("Hello from Node.js");' > index.js
node index.js
```

**AWS CLI usage:**
```bash
# AWS CLI commands use your host credentials
q-dev --entrypoint bash
aws s3 ls
```

## Common Development Tasks

### Git Operations

Git is pre-installed and configured to use your host's Git credentials:

```bash
# Inside the container
git status
git add .
git commit -m "Update code"
```

### Running Tests

Run tests for different languages:

```bash
# Python tests
pytest tests/

# JavaScript tests
npm test

# Go tests
go test ./...
```

### Building Projects

Build your projects inside the container:

```bash
# Node.js
npm run build

# Python
python setup.py build

# Go
go build
```

### Using Amazon Q for Code Assistance

Amazon Q is particularly helpful for coding tasks:

```
# Ask for code examples
How do I implement a REST API in Express.js?

# Get help with errors
I'm getting this error: TypeError: Cannot read property 'map' of undefined

# Request code reviews
Can you review this function and suggest improvements?
```

## Adding Context

Once inside the Q chat session, you can add context from your project:

```
# Add a single file
/context add README.md

# Add a directory and all its contents
/context add src/

# Add multiple specific files
/context add package.json tsconfig.json

# Clear all context
/context clear
```

Remember that Amazon Q can only access files in the current directory that's mounted in the container. Files outside this directory are not accessible to Amazon Q.

## Persistent Environment

The Docker container is configured with three types of persistence:

1. **Project Files**: Your current directory is mounted at `/home/dev/src` in the container
2. **Amazon Q Data**: Stored in Docker volumes (`amazon-q-data` and `amazon-q-cache`)
3. **Home Directory**: All user customizations are stored in the `amazon-q-home` volume

This means:

- **Installed packages** (via pip, npm, etc.) will persist between sessions
- **Shell history** and customizations will be saved
- **Configuration files** in your home directory will be preserved
- **Global tools** you install will be available in future sessions

For example, you can customize your environment once and have it available every time:

```bash
# First session
q-dev --entrypoint bash
pip install pandas matplotlib
npm install -g typescript
echo 'alias ll="ls -la"' >> ~/.bashrc
exit

# Later session - all customizations are still available
q-dev --entrypoint bash
python -c "import pandas; print(pandas.__version__)"
tsc --version
ll
```

## Limitations

When using Amazon Q in Docker, be aware of these limitations:

1. **File Access**: Only files in the current directory (and subdirectories) are accessible
2. **System Context**: The container has its own Linux environment, separate from your host OS
3. **Performance**: Running in Docker may be slightly slower than running natively
4. **GUI Applications**: The container doesn't support GUI applications
5. **Host Tools**: Tools installed on your host machine aren't available unless they're also installed in the container
6. **Network Services**: Services running on your host (like local databases) need to be accessed via special Docker networking

### Accessing Host Services

If you need to access services running on your host machine (like a local database or web server):

```bash
# On macOS/Linux, use host.docker.internal to reference the host
q-dev --entrypoint bash
curl http://host.docker.internal:3000

# For databases, use host.docker.internal instead of localhost
mysql -h host.docker.internal -u user -p
```

## Troubleshooting

If you encounter issues:

1. **Rebuild the Docker image**:
   ```bash
   source ./setup-q-dev-alias.sh --rebuild
   ```

2. **Reset data volumes** if you have database errors:
   ```bash
   docker volume rm amazon-q-data amazon-q-cache
   docker volume create amazon-q-data
   docker volume create amazon-q-cache
   ```

3. **Reset home directory** if your environment becomes corrupted:
   ```bash
   docker volume rm amazon-q-home
   docker volume create amazon-q-home
   ```

4. **Check Docker logs**:
   ```bash
   docker logs $(docker ps -q --filter ancestor=amazon-q-dev)
   ```

5. **Verify file access** by checking what's mounted:
   ```bash
   q-dev --entrypoint bash
   ls -la /home/dev/src
   ```

## Advanced Configuration

For advanced configuration options and customization details, see [customizing-docker-setup.md](customizing-docker-setup.md).
