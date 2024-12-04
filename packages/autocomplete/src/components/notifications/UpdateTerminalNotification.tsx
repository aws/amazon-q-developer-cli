import { ShellContext } from "@aws/amazon-q-developer-cli-proto/fig_common";
import { useAutocompleteStore } from "../../state";
import { Notification } from "./Notification";

export const UpdateTerminalNotification = () => {
  const { figState } = useAutocompleteStore();
  const shellContext = figState.shellContext as ShellContext | undefined;

  const qtermVersion = shellContext?.qtermVersion;
  const desktopVersion = window.fig.constants?.version;

  const mismatchedVersions = qtermVersion !== desktopVersion;

  const isLegacyVersion = parseInt((fig.constants?.version ?? "0")[0], 10) < 2;

  return (
    <Notification
      localStorageKey={`update_terminal_${
        shellContext?.sessionId ?? "unknown"
      }`}
      show={
        Boolean(qtermVersion) &&
        Boolean(desktopVersion) &&
        mismatchedVersions &&
        !isLegacyVersion
      }
      title={
        <>
          <span>This terminal must be restarted</span>
        </>
      }
      description={
        <>
          This terminal is running integrations from <br />
          an older version of Amazon Q, please restart it.
        </>
      }
    />
  );
};
