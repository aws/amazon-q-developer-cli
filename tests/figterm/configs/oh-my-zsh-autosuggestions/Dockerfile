FROM dotenv-base

RUN sh -c "$(wget https://raw.github.com/robbyrussell/oh-my-zsh/master/tools/install.sh -O -)"
COPY zshrc ~/.zshrc
RUN ~/install-fig

ENTRYPOINT ["npm", "run", "test", "--"]
CMD ["tests/zsh"]
