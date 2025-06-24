FROM dotenv-base

COPY bashrc ~/.bashrc
COPY zshrc ~/.zshrc

RUN sh -c "$(curl -fsSL https://starship.rs/install.sh)" -- --yes

RUN ~/install-fig

ENTRYPOINT ["npm", "run", "test", "--"]
CMD ["tests/bash", "tests/zsh"]
