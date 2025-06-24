import yaml from 'js-yaml';
import fs from 'fs';

type ServiceConfig = {
  command: string;
  // eslint-disable-next-line camelcase
  container_name?: string;
  build?: string;
  volumes?: string[];
  tty?: boolean;
};

const skeleton = yaml.load(fs.readFileSync('environments.yaml').toString()) as {
  services: Record<string, ServiceConfig>;
};

const services = Object.entries(skeleton.services).reduce(
  (acc, [key, val]) => {
    acc[key] = {
      container_name: key,
      build: `./configs/${key}`,
      // Mount current directory by default in all containers to avoid having to
      // rebuild when writing tests.
      volumes: ['./:/usr/home/app/', '/usr/home/app/node_modules'],
      tty: true,
      ...val,
    };
    return acc;
  },
  {} as Record<string, ServiceConfig>
);

fs.writeFileSync('docker-compose.yaml', yaml.dump({ services }));
