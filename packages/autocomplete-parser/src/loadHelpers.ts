import logger, { Logger } from "loglevel";
import * as semver from "semver";
import { mergeSubcommands } from "@fig/autocomplete-shared";
import { ensureTrailingSlash, withTimeout } from "@internal/shared/utils";
import {
  executeCommand,
  executeShellCommand,
  fread,
  isInDevMode,
} from "@amzn/fig-io-api-bindings-wrappers";
import { AuthClient, CDN, makeRequest, Routes } from "@amzn/fig-io-api-client";
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
  (specName: string, forceReload = false) =>
    `${baseUrl}${specName}.js${forceReload ? `?date=${Date.now()}` : ""}`;

const cdnUrlFactories = [
  "https://cdn.jsdelivr.net/npm/@withfig/autocomplete@2/build/",
  "https://unpkg.com/@withfig/autocomplete@^2.0.0/build/",
].map(makeCdnUrlFactory);

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
export const canLoadFigspec = () =>
  window.location.protocol === "figapp:" ||
  window.location.protocol === "http:";

// TODO: fedeci this is a problem for diff-versioned specs
export async function importFromPublicCDN<T = SpecFileImport>(
  name: string,
  forceReload = false,
): Promise<T> {
  if (canLoadFigspec()) {
    return withTimeout(
      20000,
      import(
        /* @vite-ignore */
        `figspec://localhost/${name}.js`
      ),
    );
  }

  for (const [index, urlFactory] of cdnUrlFactories.entries()) {
    try {
      // eslint-disable-next-line no-await-in-loop
      const result: T = await withTimeout(
        20000,
        import(
          /* @vite-ignore */
          urlFactory(name, forceReload)
        ),
      );

      return result;
    } catch (e) {
      // Remove the factory from its current position in the array,
      // then push it to the end so it gets tried last.
      cdnUrlFactories.splice(index, 1);
      cdnUrlFactories.push(urlFactory);
    }
  }
  throw new SpecCDNError("Unable to load from a CDN");
}

// TODO: fedeci this is a problem for diff-versioned specs
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
        const newVersion =
          await specData.getVersionCommand(executeShellCommand);
        cachedCLIVersions[storageKey] = newVersion;
        return newVersion;
      }

      const newVersion = semver.clean(
        await executeCommand(`${name} --version`),
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

interface PublicSpecsModule {
  default: string[];
  diffVersionedCompletions: string[];
}

// TODO(fedeci): cache this request using SWR strategy
let publicSpecsRequest:
  | Promise<{
      specs: Set<string>;
      diffVersionedSpecs: Set<string>;
    }>
  | undefined;

const createPublicSpecsRequest = async () => {
  if (publicSpecsRequest === undefined) {
    publicSpecsRequest = importFromPublicCDN<PublicSpecsModule>("index", true)
      .then((module) => ({
        specs: new Set(module.default),
        diffVersionedSpecs: new Set(module.diffVersionedCompletions),
      }))
      .catch(() => {
        publicSpecsRequest = undefined;
        return { specs: new Set(), diffVersionedSpecs: new Set() };
      });
  }
  return publicSpecsRequest;
};

export async function publicSpecExists(name: string): Promise<boolean> {
  const { specs } = await createPublicSpecsRequest();
  return specs.has(name);
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

export async function preloadSpecs(
  authClient: AuthClient,
): Promise<SpecFileImport[]> {
  let privateSpecInfo: CDN.PrivateSpecInfo[] = [];
  logger.info("Preloading specs...");
  try {
    privateSpecInfo = await loadPrivateSpecs(authClient);
  } catch (e) {
    logger.info("Failed to load private specs", e);
  }
  let promises = privateSpecInfo.map((v) =>
    importFromPrivateCDN(v, authClient),
  );

  if (!canLoadFigspec()) {
    promises = [
      ...promises,
      ...MOST_USED_SPECS.map(async (name) => {
        // TODO(fedeci): refactor everything to allow the correct diff-versioned specs to be loaded
        // too, now we are only loading the index
        if (await isDiffVersionedSpec(name)) {
          return importFromPublicCDN(`${name}/index`);
        }
        return importFromPublicCDN(name);
      }),
    ];
  }

  return Promise.all(promises.map((promise) => promise.catch((e) => e)));
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

export interface PrivateCliInfo {
  name: string;
  namespace: string;
}

// All private clis a users has access to
let privateClis: PrivateCliInfo[] | undefined;

export const reloadClis = async (authClient: AuthClient) => {
  try {
    type CommandLineTool = {
      currentUser: {
        namespace: {
          username: string;
          commandlineTools: {
            root: {
              name: string;
            };
          }[];
        };
        teamMemberships: {
          team: {
            namespace: {
              username: string;
              commandlineTools: {
                root: {
                  name: string;
                };
              }[];
            };
          };
        }[];
      };
    };

    const query = `query CommandLineTool {
    currentUser {
      namespace {
        username
        commandlineTools {
          root {
            name
          }
        }
      }
      teamMemberships {
        team {
          namespace {
            username
            commandlineTools {
              root {
                name
              }            
            }
          }
        }
      }
    }
  }`;

    const gqlResponse = await makeRequest(
      "graphql",
      authClient,
      JSON.stringify({ query }),
      {
        addionalHeaders: {
          "Content-Type": "application/json",
        },
        requestType: "POST",
      },
    ).then((res) => res.json());

    const commandLineTools: CommandLineTool = gqlResponse.data;

    const clis: PrivateCliInfo[] = [];

    const userNamespace = commandLineTools.currentUser.namespace;
    for (const comandlineTool of userNamespace.commandlineTools) {
      clis.push({
        name: comandlineTool.root.name,
        namespace: userNamespace.username,
      });
    }

    for (const { team } of commandLineTools.currentUser.teamMemberships) {
      const teamNamespace = team.namespace;
      for (const comandlineTool of teamNamespace.commandlineTools) {
        clis.push({
          name: comandlineTool.root.name,
          namespace: teamNamespace.username,
        });
      }
    }

    privateClis = clis;
  } catch (err) {
    logger.error("Could not preload clis", err);
    privateClis = [];
  }

  return privateClis;
};

export const preloadClis = async (authClient: AuthClient) => {
  if (!privateClis) {
    await reloadClis(authClient);
  }
  return privateClis;
};