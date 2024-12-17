import React, { ErrorInfo, lazy, Suspense, useMemo } from "react";
import { Suggestion } from "@aws/amazon-q-developer-cli-shared/internal";
import root from "react-shadow";
import { SETTINGS } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { importFromPublicCDN } from "@aws/amazon-q-developer-cli-autocomplete-parser";
import { getMaxHeight, POPOUT_WIDTH } from "../window";
import { useAutocompleteStore } from "../state";
import { shellContextSelector } from "../state/generators";
import LoadingIcon from "./LoadingIcon";
import { DescriptionPosition } from "./Description";

const COMPONENT_PATH = "components/src";

const useLoadComponent = (componentPath: string) => {
  const aeState = useAutocompleteStore();
  const isDevMode =
    Boolean(aeState.settings[SETTINGS.DEV_MODE]) ||
    Boolean(aeState.settings[SETTINGS.DEV_MODE_NPM]);
  const devPort = isDevMode
    ? aeState.settings[SETTINGS.DEV_COMPLETIONS_SERVER_PORT]
    : undefined;

  return useMemo(() => {
    const component = aeState.components[componentPath];
    if (component) {
      return component;
    }

    const newComponent = lazy(() => {
      if (typeof devPort === "number" || typeof devPort === "string") {
        return import(
          /* @vite-ignore */
          `http://localhost:${devPort}/${COMPONENT_PATH}/${componentPath}.js`
        );
      }

      return importFromPublicCDN(`${COMPONENT_PATH}/${componentPath}`);
    });

    aeState.setComponents({
      ...aeState.components,
      [componentPath]: newComponent,
    });

    return newComponent;
  }, [componentPath, devPort, aeState]);
};

const useCssUrl = () => {
  const aeState = useAutocompleteStore();
  const isDevMode =
    Boolean(aeState.settings[SETTINGS.DEV_MODE]) ||
    Boolean(aeState.settings[SETTINGS.DEV_MODE_NPM]);
  const devServerPort = aeState.settings[SETTINGS.DEV_COMPLETIONS_SERVER_PORT];

  return useMemo(() => {
    const devPort = isDevMode ? devServerPort : undefined;

    if (typeof devPort === "number" || typeof devPort === "string") {
      return `http://localhost:${devPort}/components/style.css`;
    }

    return "spec://localhost/components/style.css";
  }, [isDevMode, devServerPort]);
};

class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: false } | { hasError: true; error: Error; errorInfo: ErrorInfo }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { hasError: false };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    this.setState({
      hasError: true,
      error,
      errorInfo,
    });
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="text-main-text bg-main-bg override-scrollbar overflow-y-auto overflow-x-hidden rounded px-1.5 py-1 shadow-[0px_0px_3px_0px_rgb(85,_85,_85)]">
          <div>{this.state.error.toString()}</div>
          <div className="mt-2">Component stack:</div>
          <div className="mt-0.5 font-mono text-xs">
            {this.state.errorInfo.componentStack}
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

type PreviewProps = {
  selectedItem: Suggestion;
  position: DescriptionPosition;
};

const Preview = ({ selectedItem, position }: PreviewProps) => {
  const aeState = useAutocompleteStore();
  const cssUrl = useCssUrl();

  const previewComponentPath = selectedItem.previewComponent;
  const PreviewComponent = previewComponentPath
    ? // eslint-disable-next-line react-hooks/rules-of-hooks
      useLoadComponent(previewComponentPath)
    : undefined;

  const maxHeight = getMaxHeight();
  const shellContext: Fig.ShellContext = shellContextSelector(aeState);

  if (!PreviewComponent || position === DescriptionPosition.UNKNOWN) {
    return <></>;
  }

  return (
    <div
      style={{
        maxHeight,
        width: POPOUT_WIDTH,
        maxWidth: POPOUT_WIDTH,

        ...({
          "--main-bg-color": "rgb(var(--main-bg-color))",
          "--main-text-color": "rgb(var(--main-text-color))",
          "--description-text-color": "rgb(var(--description-text-color))",
          "--description-border-color": "rgb(var(--description-border-color))",
          "--selected-bg-color": "rgb(var(--selected-bg-color))",
          "--selected-text-color": "rgb(var(--selected-text-color))",
          "--matching-bg-color": "rgb(var(--matching-bg-color))",
          "--selected-matching-bg-color":
            "rgb(var(--selected-matching-bg-color))",
        } as React.CSSProperties),
      }}
    >
      <ErrorBoundary>
        <root.div>
          <link href={cssUrl} rel="stylesheet" />
          <Suspense fallback={<LoadingIcon />}>
            <PreviewComponent
              suggestion={selectedItem}
              shellContext={shellContext}
              maxHeight={maxHeight}
              tokens={aeState.command?.tokens}
            />
          </Suspense>
        </root.div>
      </ErrorBoundary>
    </div>
  );
};

export default Preview;
