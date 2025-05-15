# Use Ubuntu with the appropriate architecture
ARG ARCH=amd64
FROM --platform=linux/${ARCH} ubuntu:latest

# Set environment variables (timezone will be set at runtime)
ENV DEBIAN_FRONTEND=noninteractive TERM="xterm-256color"

# Install common dependencies and mise
RUN apt-get update && apt-get install -y curl wget git jq vim nano unzip zip ssh ca-certificates \
    gnupg lsb-release software-properties-common build-essential pkg-config tzdata sqlite3 && \
    rm -rf /var/lib/apt/lists/*

# Install AWS CLI and Amazon Q CLI with architecture detection
ARG TARGETARCH
RUN ARCH_AWS=$([ "$TARGETARCH" = "amd64" ] && echo "x86_64" || echo "aarch64") && \
    curl -fsSL "https://awscli.amazonaws.com/awscli-exe-linux-$ARCH_AWS.zip" -o awscliv2.zip && \
    curl --proto '=https' --tlsv1.2 -sSf "https://desktop-release.codewhisperer.us-east-1.amazonaws.com/latest/q-$ARCH_AWS-linux.zip" -o q.zip && \
    unzip awscliv2.zip && ./aws/install && rm -rf awscliv2.zip ./aws && \
    unzip q.zip -d /tmp && chmod +x /tmp/q/install.sh && Q_INSTALL_GLOBAL=true /tmp/q/install.sh && \
    rm -rf q.zip /tmp/q
    
# Create non-root user and set up directories
RUN useradd -ms /bin/bash dev && \
    mkdir -p /home/dev/src /home/dev/.aws/amazonq/profiles /home/dev/.ssh && \
    mkdir -p /home/dev/.local/share/amazon-q /home/dev/.cache/amazon-q && \
    chown -R dev:dev /home/dev

# Switch to non-root user
USER dev
WORKDIR /home/dev/src

RUN echo 'export PS1="\[\033[01;32m\]q-dev\[\033[00m\]:\[\033[01;34m\]\w\[\033[00m\]\$ "' >> /home/dev/.bashrc

# Initialize the SQLite database to ensure proper permissions
RUN mkdir -p /home/dev/.local/share/amazon-q && \
    touch /home/dev/.local/share/amazon-q/data.sqlite3 && \
    chmod 644 /home/dev/.local/share/amazon-q/data.sqlite3

# Install mise for managing multiple language runtimes
RUN curl https://mise.run | sh && \
    echo 'eval "$(~/.local/bin/mise activate --shims bash)"' >> ~/.bashrc && \
    eval "$(~/.local/bin/mise activate --shims bash)" && \
    ~/.local/bin/mise use -g python@latest uv@latest node@lts java@latest go@latest

# Default command starts Amazon Q chat
ENTRYPOINT [ "q" ]
CMD ["chat"]
