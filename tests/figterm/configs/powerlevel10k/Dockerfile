FROM dotenv-base

RUN git clone --depth=1 https://github.com/romkatv/powerlevel10k.git ~/powerlevel10k
COPY zshrc ~/.zshrc
COPY p10k.zsh ~/.p10k.zsh
RUN ~/install-fig

ENTRYPOINT ["npm", "run", "test", "--"]
CMD ["tests/zsh"]
