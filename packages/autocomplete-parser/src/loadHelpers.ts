import logger, { Logger } from "loglevel";
import * as semver from "semver";
import { mergeSubcommands } from "@fig/autocomplete-shared";
import {
  ensureTrailingSlash,
  withTimeout,
  exponentialBackoff,
} from "@aws/amazon-q-developer-cli-shared/utils";
import {
  executeCommand,
  fread,
  isInDevMode,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { AuthClient, CDN, Routes } from "@aws/amazon-q-developer-cli-api-client";
import z from "zod";
import { MOST_USED_SPECS } from "./constants.js";
import { mixinCache } from "./caches.js";
import {
  LoadLocalSpecError,
  MissingSpecError,
  SpecCDNError,
} from "./errors.js";

export type SpecFileImport =
  | {
      default: Fig.Spec;
      getVersionCommand?: Fig.GetVersionCommand;
    }
  | {
      default: Fig.Subcommand;
      versions: Fig.VersionDiffMap;
    };

// All private specs a users has access to
let privateSpecs: CDN.PrivateSpecInfo[] = [];

const makeCdnUrlFactory =
  (baseUrl: string) =>
  (specName: string, ext: string = "js") =>
    `${baseUrl}${specName}.${ext}`;

const cdnUrlFactory = makeCdnUrlFactory(
  "https://specs.q.us-east-1.amazonaws.com/",
);

/* eslint-disable-next-line @typescript-eslint/no-explicit-any */
const stringImportCache = new Map<string, any>();

export const importString = async (str: string) => {
  if (stringImportCache.has(str)) {
    return stringImportCache.get(str);
  }
  const result = await import(
    /* @vite-ignore */
    URL.createObjectURL(new Blob([str], { type: "text/javascript" }))
  );

  stringImportCache.set(str, result);
  return result;
};

type FigConfiguration = {
  scriptCompletions?: Record<
    string,
    string | { name: string; namespace?: string }
  >;
} /* @vite-ignore */;

export const importFromPrivateCDN = async (
  info: CDN.PrivateSpecInfo,
  authClient: AuthClient,
): Promise<SpecFileImport> =>
  Routes.cdn
    .getPrivateSpec(info.namespace, info.name, authClient)
    .then(importString);

export async function getSpecInfo(
  name: string,
  path: string,
  localLogger: Logger = logger,
): Promise<CDN.PrivateSpecInfo> {
  localLogger.info(`Loading local spec in ${path}`);
  const result = await fread(`${ensureTrailingSlash(path)}.fig/config.json`);
  const configuration: FigConfiguration = JSON.parse(result);
  const specToLoad = configuration?.scriptCompletions?.[name];

  let specInfo: CDN.PrivateSpecInfo | undefined;
  if (typeof specToLoad === "string") {
    if (specToLoad.startsWith("@") && specToLoad.includes("/")) {
      const idx = specToLoad.indexOf("/");
      specInfo = getPrivateSpec({
        name: specToLoad.slice(idx + 1),
        namespace: specToLoad.slice(1, idx),
        isScript: true,
      });
    } else {
      specInfo = getPrivateSpec({ name: specToLoad, isScript: true });
    }
  } else if (specToLoad) {
    specInfo = getPrivateSpec({
      name: specToLoad.name,
      namespace: specToLoad.namespace,
      isScript: true,
    });
  }

  if (!specInfo) {
    throw new MissingSpecError("No spec found");
  }

  return specInfo;
}

/*
 * Deprecated: eventually will just use importLocalSpec above
 * Load a spec import("{path}/{name}")
 */
export async function importSpecFromFile(
  name: string,
  path: string,
  localLogger: Logger = logger,
): Promise<SpecFileImport> {
  const importFromPath = async (fullPath: string) => {
    localLogger.info(`Loading spec from ${fullPath}`);
    const contents = await fread(fullPath);
    if (!contents) {
      throw new LoadLocalSpecError(`Failed to read file: ${fullPath}`);
    }
    return contents;
  };

  let result: string;
  const joinedPath = `${ensureTrailingSlash(path)}${name}`;
  try {
    result = await importFromPath(`${joinedPath}.js`);
  } catch (_) {
    result = await importFromPath(`${joinedPath}/index.js`);
  }

  return importString(result);
}

/**
 * Specs can only be loaded from non "secure" contexts, so we can't load from https
 */
export const canLoadSpecProtocol = () => window.location.protocol !== "https:";

// TODO: this is a problem for diff-versioned specs
export async function importFromPublicCDN<T = SpecFileImport>(
  name: string,
): Promise<T> {
  if (canLoadSpecProtocol()) {
    return withTimeout(
      20000,
      import(
        /* @vite-ignore */
        `spec://localhost/${name}.js`
      ),
    );
  }

  // Total of retries in the worst case should be close to previous timeout value
  // 500ms * 2^5 + 5 * 1000ms + 5 * 100ms = 21500ms, before the timeout was 20000ms
  try {
    return await exponentialBackoff(
      {
        attemptTimeout: 1000,
        baseDelay: 500,
        maxRetries: 5,
        jitter: 100,
      },

      () => import(/* @vite-ignore */ cdnUrlFactory(name)),
    );
  } catch {
    /**/
  }

  throw new SpecCDNError("Unable to load from a CDN");
}

async function jsonFromPublicCDN(path: string): Promise<unknown> {
  if (canLoadSpecProtocol()) {
    return fetch(`spec://localhost/${path}.json`).then((res) => res.json());
  }

  return exponentialBackoff(
    {
      attemptTimeout: 1000,
      baseDelay: 500,
      maxRetries: 5,
      jitter: 100,
    },
    () => fetch(cdnUrlFactory(path, "json")).then((res) => res.json()),
  );
}

// TODO: this is a problem for diff-versioned specs
export async function importFromLocalhost<T = SpecFileImport>(
  name: string,
  port: number | string,
): Promise<T> {
  return withTimeout(
    20000,
    import(
      /* @vite-ignore */
      `http://localhost:${port}/${name}.js`
    ),
  );
}

const cachedCLIVersions: Record<string, string | undefined> = {};

export const getCachedCLIVersion = (key: string) =>
  cachedCLIVersions[key] ?? null;

export async function getVersionFromFullFile(
  specData: SpecFileImport,
  name: string,
) {
  // if the default export is a function it is a versioned spec
  if (typeof specData.default === "function") {
    try {
      const storageKey = `cliVersion-${name}`;
      const version = getCachedCLIVersion(storageKey);
      if (!isInDevMode() && version !== null) {
        return version;
      }

      if ("getVersionCommand" in specData && specData.getVersionCommand) {
        const newVersion = await specData.getVersionCommand(executeCommand);
        cachedCLIVersions[storageKey] = newVersion;
        return newVersion;
      }

      const newVersion = semver.clean(
        (
          await executeCommand({
            command: name,
            args: ["--version"],
          })
        ).stdout,
      );
      if (newVersion) {
        cachedCLIVersions[storageKey] = newVersion;
        return newVersion;
      }
    } catch {
      /**/
    }
  }
  return undefined;
}

// TODO: cache this request using SWR strategy
let publicSpecsRequest:
  | Promise<{
      completions: Set<string>;
      diffVersionedSpecs: Set<string>;
    }>
  | undefined;

export function clearSpecIndex() {
  publicSpecsRequest = undefined;
}

const INDEX_ZOD = z.object({
  completions: z.array(z.string()),
  diffVersionedCompletions: z.array(z.string()),
});

const createPublicSpecsRequest = async () => {
  if (publicSpecsRequest === undefined) {
    publicSpecsRequest = jsonFromPublicCDN("index")
      .then(INDEX_ZOD.parse)
      .then((index) => ({
        completions: new Set(index.completions),
        diffVersionedSpecs: new Set(index.diffVersionedCompletions),
      }))
      .catch(() => {
        publicSpecsRequest = undefined;
        return { completions: new Set(), diffVersionedSpecs: new Set() };
      });
  }
  return publicSpecsRequest;
};

export async function publicSpecExists(name: string): Promise<boolean> {
  const { completions } = await createPublicSpecsRequest();
  return completions.has(name);
}

export async function isDiffVersionedSpec(name: string): Promise<boolean> {
  const { diffVersionedSpecs } = await createPublicSpecsRequest();
  return diffVersionedSpecs.has(name);
}

export async function loadPrivateSpecs(
  authClient: AuthClient,
): Promise<CDN.PrivateSpecInfo[]> {
  try {
    const data = await Routes.cdn.getPrivateSpecList(authClient);

    if (data && data.length) {
      logger.info("Successfully fetched private spec information");
      privateSpecs = data;
      return data;
    }
  } catch (err) {
    logger.info("Could not fetch private spec info");
  }
  return [];
}

export function getPrivateSpec({
  name,
  isScript,
  namespace,
}: {
  name: string;
  isScript?: boolean;
  namespace?: string;
}): CDN.PrivateSpecInfo | undefined {
  return privateSpecs.find(
    (spec) =>
      spec.name === name &&
      (isScript === undefined ||
        Boolean(spec.isScript) === Boolean(isScript)) &&
      (namespace === undefined || spec.namespace === namespace),
  );
}

export async function preloadSpecs(): Promise<SpecFileImport[]> {
  return Promise.all(
    MOST_USED_SPECS.map(async (name) => {
      // TODO: refactor everything to allow the correct diff-versioned specs to be loaded
      // too, now we are only loading the index
      if (await isDiffVersionedSpec(name)) {
        return importFromPublicCDN(`${name}/index`);
      }
      return importFromPublicCDN(name);
    }).map((promise) => promise.catch((e) => e)),
  );
}

let preloadPromise: Promise<void> | undefined;

export const resetPreloadPromise = () => {
  preloadPromise = undefined;
};

interface MixinFile {
  file: Fig.Subcommand;
  key: string;
}

const mergeConflictingMixinFiles = (mixinFiles: MixinFile[]) => {
  const mixinFilesMap = mixinFiles.reduce(
    (acc, { file, key }) => ({
      ...acc,
      [key]: [...(key in acc ? acc[key] : []), file],
    }),
    {} as Record<string, Fig.Subcommand[]>,
  );

  const mergedMixinFilesMap = Object.entries(mixinFilesMap).reduce(
    (mergedAcc, [key, files]) => {
      const mergedFile = files.reduce(
        (acc, file) => mergeSubcommands(acc, file),
        { name: files[0].name } as Fig.Subcommand,
      );
      return { ...mergedAcc, [key]: mergedFile };
    },
    {} as Record<string, Fig.Subcommand>,
  );

  return mergedMixinFilesMap;
};

export const getMixinCacheKey = (specName: string, specNamespace?: string) =>
  specNamespace ? `private:${specNamespace}/${specName}` : `public:${specName}`;

// This is to prevent multiple fetches being made while the first
// fetch hasn't resolved yet
export const preloadMixins = (authClient: AuthClient) => {
  if (!preloadPromise) {
    preloadPromise = (async () => {
      try {
        const mixinMetas = await Routes.mixins.getAll(authClient);

        const mixinFiles = await Promise.all(
          mixinMetas.map(({ specFile, specName, specNamespace }) =>
            importString(specFile)
              .then((res) => res.default)
              .then((file) => ({
                file,
                key: getMixinCacheKey(specName, specNamespace),
              })),
          ),
        );

        const mergedMixinFiles = mergeConflictingMixinFiles(mixinFiles);
        Object.entries(mergedMixinFiles).forEach(([key, file]) => {
          mixinCache.set(key, file);
        });
        logger.info("Mixins preloaded successfully");
      } catch {
        logger.info("Could not preload mixins");
      }
    })();
  }
  return preloadPromise;
};
