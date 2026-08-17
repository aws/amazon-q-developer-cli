export * from './rc-binary-cache/core';
export * from './rc-binary-cache/bundle';
export * from './rc-binary-cache/identity';

import { main } from './rc-binary-cache/cli';

if (import.meta.main) {
  main().catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`::error::RC binary cache: ${message}`);
    process.exit(1);
  });
}
