FROM dotenv-base

RUN ~/install-fig

RUN rm -rf ~/.profile ~/.zprofile
RUN mkdir -p ~/withfig ~/withoutfig
RUN mv ~/.bashrc ~/.zshrc ~/withfig

# Copy configs without fig from this dir
COPY ./bashrc /usr/home/withoutfig/.bashrc
COPY ./zshrc /usr/home/withoutfig/.zshrc

ENTRYPOINT ["npm", "run", "test", "--"]
CMD ["tests/startup"]
