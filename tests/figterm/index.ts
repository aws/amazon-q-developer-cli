import Shell, { PTYOptions } from './src/shell';

const main = async () => {
  const shell = new Shell();
  await shell.initialize({ shell: 'bash' });
  const res = await shell.execute('echo hello');
  console.log({ output: res });
  await shell.type('echo hi');
  console.log({ buffer: shell.buffer });
  await shell.kill();
};

main();
