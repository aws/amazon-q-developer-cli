FROM dotenv-base

RUN git clone --recursive https://github.com/sorin-ionescu/prezto.git "$HOME/.zprezto"
RUN ~/install-fig

ENTRYPOINT ["npm", "run", "test", "--"]
CMD ["tests/zsh"]
