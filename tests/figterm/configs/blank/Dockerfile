FROM dotenv-base

RUN ~/install-fig

ENTRYPOINT ["npm", "run", "test", "--"]
CMD ["tests/bash", "tests/zsh"]
