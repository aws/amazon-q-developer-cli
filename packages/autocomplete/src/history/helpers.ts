import { Internal, Metadata } from "@fig/autocomplete-shared";
import logger from "loglevel";
import { Command } from "@aws/amazon-q-developer-cli-shell-parser";
import {
  ArgumentParserResult,
  BasicAnnotation,
  TokenType,
} from "@aws/amazon-q-developer-cli-autocomplete-parser";
import { SuggestionNotFoundError } from "./errors";

export type AnnotatedCommand = {
  command: Command;
  parserResult?: ArgumentParserResult;
};

export type HistoryContext = {
  cwd: string;
  time: number;
  exitCode: number;
  terminalSession: string;
  shell: string;
};

export type HistoryEntry = {
  text: string;
  context?: Partial<HistoryContext>;
  commands: AnnotatedCommand[];
};

export type Graph = {
  nodes: Record<string, number>;
  edges: Record<string, Record<string, number>>;
};

export type KeyFunc<T> = (entry: T) => string;

export const addEntry = (
  graph: Graph,
  getKey: KeyFunc<AnnotatedCommand>,
  entry: HistoryEntry,
  previousEntry?: HistoryEntry,
): Graph => {
  const previousKey = previousEntry?.commands[0]
    ? getKey(previousEntry.commands[0])
    : "";
  const entryKeys = [previousKey, ...entry.commands.map(getKey)];

  const nodes = { ...graph.nodes };
  const edges = { ...graph.edges };

  for (let j = 0; j < entryKeys.length; j += 1) {
    const [prevKey, key] = [entryKeys[j - 1], entryKeys[j]];
    if (key) {
      nodes[key] = nodes[key] || 0;
      edges[key] = edges[key] || {};

      if (j > 0) {
        nodes[key] += 1;
      }

      if (prevKey) {
        const newCount = (edges[prevKey][key] || 0) + 1;
        edges[prevKey] = { ...edges[prevKey], [key]: newCount };
      }
    }
  }

  return { nodes, edges };
};

const sumRecords = (
  a: Record<string, number>,
  b: Record<string, number>,
): Record<string, number> =>
  Object.keys(b).reduce(
    (acc, key) => {
      acc[key] = key in acc ? acc[key] + b[key] : b[key];
      return acc;
    },
    { ...a },
  );

export const addGraphs = (a: Graph, b: Graph): Graph => {
  const nodes = sumRecords(a.nodes, b.nodes);
  return Object.keys(nodes).reduce(
    (acc, key) => {
      acc.edges[key] = sumRecords(a.edges[key] || {}, b.edges[key] || {});
      return acc;
    },
    { nodes, edges: {} } as Graph,
  );
};

export const constructHistoryGraph = (
  source: HistoryEntry[],
  getKey: KeyFunc<AnnotatedCommand>,
): Graph => {
  let graph: Graph = { nodes: {}, edges: {} };
  for (let i = 0; i < source.length; i += 1) {
    graph = addEntry(graph, getKey, source[i], source[i - 1]);
  }
  return graph;
};

export function mirrorSubcommand<
  ArgT = Metadata.ArgMeta,
  ArgS = ArgT,
  OptionT = Metadata.OptionMeta,
  OptionS = OptionT,
  SubcommandT = Metadata.SubcommandMeta,
  SubcommandS = SubcommandT,
>(
  subcommand: Internal.Subcommand<ArgT, OptionT, SubcommandT>,
  mapArg: (arg: ArgT) => ArgS,
  mapOptionMeta: (optionMeta: OptionT) => OptionS,
  mapSubcommandMeta: (subcommandMeta: SubcommandT) => SubcommandS,
): Internal.Subcommand<ArgS, OptionS, SubcommandS> {
  const { subcommands, options, persistentOptions, args, name, ...meta } =
    subcommand;
  const mapOptions = (
    optionRecord: Record<string, Internal.Option<ArgT, OptionT>>,
  ): Record<string, Internal.Option<ArgS, OptionS>> =>
    Object.values(optionRecord).reduce(
      (acc, option) => {
        const value: Internal.Option<ArgS, OptionS> = {
          ...mapOptionMeta(option),
          name: option.name,
          args: option.args.map(mapArg),
        };
        option.name.forEach((n) => {
          if (acc[n] === undefined) {
            acc[n] = value;
          }
        });
        return acc;
      },
      {} as Record<string, Internal.Option<ArgS, OptionS>>,
    );

  return {
    ...mapSubcommandMeta(meta as unknown as SubcommandT),
    name,
    subcommands: Object.values(subcommands).reduce(
      (acc, currentSubcommand) => {
        const value: Internal.Subcommand<ArgS, OptionS, SubcommandS> =
          mirrorSubcommand(
            currentSubcommand,
            mapArg,
            mapOptionMeta,
            mapSubcommandMeta,
          );
        currentSubcommand.name.forEach((n) => {
          if (acc[n] === undefined) {
            acc[n] = value;
          }
        });
        return acc;
      },
      {} as Record<string, Internal.Subcommand<ArgS, OptionS, SubcommandS>>,
    ),
    options: mapOptions(options),
    persistentOptions: mapOptions(persistentOptions),
    args: args.map(mapArg),
  };
}

type WalkCallback<T> = (item: T, annotation: BasicAnnotation) => void;

type WalkSubcommandCallbacks<ArgT, OptionT, SubcommandT> = {
  subcommand?: WalkCallback<Internal.Subcommand<ArgT, OptionT, SubcommandT>>;
  option?: WalkCallback<Internal.Option<ArgT, OptionT>>;
  arg?: WalkCallback<ArgT>;
};

// TODO: handle loadSpec on args.
export const walkSubcommand = <
  ArgT = Metadata.ArgMeta,
  OptionT = Metadata.OptionMeta,
  SubcommandT = Metadata.SubcommandMeta,
>(
  subcommand: Internal.Subcommand<ArgT, OptionT, SubcommandT>,
  annotations: BasicAnnotation[],
  callbacks?: WalkSubcommandCallbacks<ArgT, OptionT, SubcommandT>,
) => {
  const initialState = { optionArgIndex: 0, subcommandArgIndex: 0 };
  try {
    const finalState = annotations.reduce(
      (state, annotation) => {
        const { text, type } = annotation;
        const name =
          "tokenName" in annotation ? (annotation.tokenName ?? text) : text;

        if (type === TokenType.Subcommand) {
          if (!state.subcommand.subcommands[name]) {
            throw new SuggestionNotFoundError(`Subcommand not found: ${name}`);
          }
          if (callbacks?.subcommand && subcommand) {
            callbacks.subcommand(
              state.subcommand.subcommands[name],
              annotation,
            );
          }
          return {
            ...initialState,
            subcommand: state.subcommand.subcommands[name],
          };
        }
        if (type === TokenType.Option) {
          const option =
            state.subcommand.options[name] ??
            state.subcommand.persistentOptions[name];
          if (!option && name !== "--") {
            throw new SuggestionNotFoundError(`Option not found: ${name}`);
          }
          if (callbacks?.option && option) {
            callbacks.option(option, annotation);
          }
          return { ...state, option, optionArgIndex: 0 };
        }
        if (type === TokenType.OptionArg) {
          const arg = state.option?.args[state.optionArgIndex];
          if (callbacks?.arg && arg) {
            callbacks.arg(arg, annotation);
          }
          return { ...state, optionArgIndex: state.optionArgIndex + 1 };
        }
        if (type === TokenType.SubcommandArg) {
          const arg = state.subcommand.args[state.subcommandArgIndex];
          if (callbacks?.arg && arg) {
            callbacks.arg(arg, annotation);
          }
          return {
            ...state,
            option: undefined,
            subcommandArgIndex: state.subcommandArgIndex + 1,
          };
        }
        return state;
      },
      { ...initialState, subcommand } as {
        subcommand: Internal.Subcommand<ArgT, OptionT, SubcommandT>;
        option?: Internal.Option<ArgT, OptionT>;
        optionArgIndex: number;
        subcommandArgIndex: number;
      },
    );

    const type = annotations[annotations.length - 1]?.type;
    switch (type) {
      case TokenType.Subcommand:
        return { type, subcommand: finalState.subcommand };
      case TokenType.Option:
        return { type, option: finalState.option };
      case TokenType.OptionArg:
        return {
          type,
          arg: finalState.option?.args[finalState.optionArgIndex - 1],
        };
      case TokenType.SubcommandArg:
        return {
          type,
          arg: finalState.subcommand.args[finalState.subcommandArgIndex - 1],
        };
      case TokenType.None:
      default:
        return { type };
    }
  } catch (err) {
    logger.warn(err);
    return { type: TokenType.None };
  }
};
