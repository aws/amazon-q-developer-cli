FROM rust AS chef 
RUN cargo install cargo-chef 
WORKDIR /app/

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cat recipe.json
# Build cached dependencies
RUN cargo chef cook --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --package q_cli --package figterm

# We do not need the Rust toolchain to run the binary!
FROM node AS runtime
WORKDIR /usr/home/app/
COPY --from=builder /app/target/release/app /usr/local/bin

# Install dependencies
RUN sed -i 's#archive.ubuntu.com/ubuntu#mirror.pit.teraswitch.com/ubuntu#g' /etc/apt/sources.list
RUN sed -i 's#security.ubuntu.com/ubuntu#mirror.pit.teraswitch.com/ubuntu#g' /etc/apt/sources.list
# RUN apt install -y python3 python3-pip && ln -sf python3 /usr/bin/python
# RUN pip3 install --no-cache --upgrade pip setuptools
RUN apt update
RUN apt install -y \
  # sudo make g++ libtool libtool-bin libc-dev perl \
  bash zsh fish \
  uuid-runtime netcat-openbsd \
  protobuf-compiler \
  clang cmake curl git jq \
  musl-dev \
  vim
  # llvm clang
RUN apt upgrade -y

ENV HOME=/usr/home

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain stable --profile minimal -y
ENV PATH="${HOME}/.cargo/bin:${PATH}"
RUN rustup target add x86_64-unknown-linux-musl
RUN npm install --global ts-proto

# ENV TMPDIR=/tmp/

WORKDIR /usr/home/app/

RUN mkdir -p /Applications/Amazon\ Q.app/

# Set up test user
RUN adduser --disabled-password --home /usr/home test 
RUN adduser test sudo
# Prevent password prompts for sudo
RUN echo '%sudo ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers
RUN chown -R test:test /usr/home/
USER test

COPY --chown=test:test ./tests/figterm/package.json .
COPY --chown=test:test ./q_cli/ ./q_cli/
COPY --chown=test:test ./figterm/ ./figterm/
COPY --chown=test:test ./lib/ ./lib/
COPY --chown=test:test ./build-scripts/ ./build-scripts/
RUN ls -la

ENV BASH_ENV=/tmp/.env
ENV CARGO_INCREMENTAL=0
ENV CARGO_PROFILE_RELEASE_LTO=thin
ENV RUSTFLAGS="-C force-frame-pointers=yes -C link-arg=-Wl,--compress-debug-sections=zlib"

RUN cd q_cli && cargo build --release
RUN cd figterm && cargo build --release

RUN FIG_CLI=q_cli/target/x86_64-unknown-linux-musl/release/q_cli \
  FIGTERM=figterm/target/x86_64-unknown-linux-musl/release/figterm \
  IS_MINIMAL=1 \
  build-scripts/deb.sh

# Generate local.ts
# Set up testing infrastructure
COPY --chown=test:test ./package.json .
RUN npm install

# Provide easy script for child images to call to add fig to dotfiles.
RUN echo 'install-fig-from-deb' > ~/install-fig
RUN chmod +x ~/install-fig

# Indicate src/ will be mounted to preserve things like local.ts that are generated in the container.
VOLUME /usr/home/app/src/

CMD ["bash"]
