import { v4 as uuidv4 } from 'uuid';
import net from 'net';
import fs from 'fs';

type SocketCallback = (bytes: Buffer) => void;

type Socket = {
  path: string;
  callbacks: Record<string, SocketCallback>;
  server: net.Server;
  connections: net.Socket[];
};

const sockets: Record<string, Socket> = {};

export const socketListen = (
  path: string,
  callback: SocketCallback,
  uuid?: string
): string => {
  const callbackUUID = uuid ?? uuidv4();

  try {
    sockets[path].callbacks[callbackUUID] = callback;
    return callbackUUID;
  } catch (e) {
    // continue
  }

  try {
    fs.unlinkSync(path);
  } catch (e) {
    /* console.log(e) */
  }

  const server = net.createServer();
  const connections: net.Socket[] = [];
  sockets[path] = {
    path,
    callbacks: { [callbackUUID]: callback },
    server,
    connections,
  };

  server.on('connection', (s) => {
    connections.push(s);
    s.on('close', () => {
      const index = connections.findIndex((conn) => conn === s);
      if (index !== -1) connections.splice(index, 1);
    });
    s.on('data', (data) => {
      Object.values(sockets[path]?.callbacks ?? {}).forEach((cb) => cb(data));
      s.end();
    });

    s.on('error', console.log);
  });
  server.listen(path);

  return callbackUUID;
};

export const closeSocket = async (path: string) => {
  if (sockets[path]) {
    const { server, connections } = sockets[path];
    delete sockets[path];

    await Promise.all(
      connections.map(
        (connection) =>
          new Promise<void>((resolve) => {
            connection.end(() => {
              connection.destroy();
              resolve();
            });
          })
        // connection.destroy();
      )
    );
    await new Promise<void>((resolve) => {
      server.close((err) => {
        if (err) console.log(err);
        resolve();
      });
    });
  }
};

export const removeListener = async (path: string, uuid: string) => {
  delete sockets[path]?.callbacks[uuid];
  if (Object.keys(sockets[path].callbacks).length === 0) {
    await closeSocket(path);
  }
};
