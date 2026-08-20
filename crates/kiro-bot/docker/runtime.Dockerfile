# kiro-help bot runtime image. Single stage: GitHub Actions has already
# cross-compiled the aarch64 binaries and staged them, the agent definitions,
# the runtime config templates, and the source sha under ./artifacts, so there
# is nothing left to assemble from a carrier image.
#
# Build context is the REPO ROOT, because this needs both ./artifacts (staged
# by the workflow) and this directory's entrypoint.sh.
FROM public.ecr.aws/debian/debian@sha256:1f6767130e3479e42348856acee11bbe78d26cc558b4bf52ac5106f3fcf594ff
ARG SOURCE_SHA
LABEL org.opencontainers.image.revision=$SOURCE_SHA

# Every package here is load-bearing: git for the entrypoint's worktree clone,
# curl for the ECS /healthz container health check, tini as the reaper the
# entrypoint re-execs into when it drops privileges. setpriv comes from
# util-linux in the base image.
#
# The ECS task definition starts this container as root with
# KIRO_BOT_RUNTIME_UID/GID=10001, which is the path that locks down /tmp,
# chowns the mounts, and execs setpriv -> tini. `USER 10001:10001` below is only
# the fallback for a plain `docker run`; on that path there is nothing to drop,
# so kiro-bot is pid 1 itself and reaps nothing. Keep the task definition
# starting as root.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl git tini \
    && groupadd --gid 10001 kiro \
    && useradd --uid 10001 --gid 10001 --create-home --shell /usr/sbin/nologin kiro \
    && install -d -o 10001 -g 10001 \
        /home/kiro/.kiro \
        /home/kiro/.kiro/bots \
        /home/kiro/.kiro/settings \
        /home/kiro/.kiro/sessions \
        /home/kiro/.local/share/kiro-cli \
        /var/lib/kiro-cli \
    && rm -rf /var/lib/apt/lists/*

COPY artifacts/kiro-bot            /usr/local/bin/kiro-bot
COPY artifacts/kiro-cli            /usr/local/bin/kiro-cli
COPY artifacts/kiro-knowledge-mcp  /usr/local/bin/kiro-knowledge-mcp
COPY artifacts/kiro-github-mcp     /usr/local/bin/kiro-github-mcp
COPY artifacts/kiro-mcp            /usr/local/bin/kiro-mcp
# Phase 1c-bundle deprecation alias — removed in Phase 2.
COPY artifacts/kiro-taskei-mcp     /usr/local/bin/kiro-taskei-mcp

# Config templates stay root-owned and un-substituted: entrypoint.sh reads them
# from here, expands ${BOT_MEMBER_ID}/${ALLOWED_CHANNEL_ID}, and writes the
# result into the bot's install dir. Never bake real IDs into the image.
COPY artifacts/kiro-help/          /etc/kiro-help/
# Must stay immediately after the directory COPY above — a directory COPY into
# the same path would clobber it, and entrypoint.sh then dies on a missing
# embedded source sha.
COPY artifacts/source-sha          /etc/kiro-help/source-sha

# chat-cli-v2's disk loader resolves `--agent kiro-help` from here, so this must
# be readable by the runtime uid.
COPY --chown=10001:10001 artifacts/agents/ /home/kiro/.kiro/agents/

COPY crates/kiro-bot/docker/entrypoint.sh /entrypoint.sh

# Downloaded artifacts arrive without the exec bit, and readonlyRootFilesystem
# means this is the only chance to set it.
RUN chmod 0555 \
        /entrypoint.sh \
        /usr/local/bin/kiro-bot \
        /usr/local/bin/kiro-cli \
        /usr/local/bin/kiro-knowledge-mcp \
        /usr/local/bin/kiro-github-mcp \
        /usr/local/bin/kiro-mcp \
        /usr/local/bin/kiro-taskei-mcp

USER 10001:10001
ENV HOME=/home/kiro
ENTRYPOINT ["/entrypoint.sh"]
