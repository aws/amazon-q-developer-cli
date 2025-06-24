FROM dotenv-base

# https://github.com/sindresorhus/pure
# Use manual install.
RUN mkdir -p "$HOME/.zsh"
RUN git clone https://github.com/sindresorhus/pure.git "$HOME/.zsh/pure"
COPY zshrc ~/.zshrc
RUN ~/install-fig

ENTRYPOINT ["npm", "run", "test", "--"]
CMD ["tests/zsh"]
