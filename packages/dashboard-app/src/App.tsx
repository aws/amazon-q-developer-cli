import { Routes, Route, Outlet, useNavigate } from "react-router-dom";
import Account from "./pages/settings/account";
import Help from "./pages/help";
import SidebarLink from "./components/sidebar/link";
import Autocomplete from "./pages/terminal/autocomplete";
import Translate from "./pages/terminal/translate";
import Chat from "./pages/terminal/chat";
import Inline from "./pages/terminal/inline";
import Onboarding from "./pages/onboarding";
import Preferences from "./pages/settings/preferences";
import Integrations from "./pages/settings/integrations";
import Keybindings from "./pages/settings/keybindings";
import Licenses from "./pages/licenses";
import { Suspense, useContext, useEffect, useRef } from "react";
import Modal from "./components/modal";
import { Telemetry, Event } from "@aws/amazon-q-developer-cli-api-bindings";
import InstallModal from "./components/installs/modal";
import LoginModal from "./components/installs/modal/login";
import { getIconFromName } from "./lib/icons";
import { StoreContext } from "./context/zustand";
import { createStore } from "./lib/store";
import ListenerContext from "./context/input";
import { useLocation } from "react-router-dom";
import {
  useAccessibilityCheck,
  useDotfilesCheck,
  useGnomeExtensionCheck,
} from "./hooks/store";
import { useLocalStateZodDefault } from "./hooks/store/useState";
import { z } from "zod";
import { useAuth } from "./hooks/store/useAuth";
import { NOTIFICATIONS_SEEN_STATE_KEY } from "./lib/constants";
import WhatsNew from "./pages/whats-new";
import notificationFeedItems from "../../../feed.json";
import { useState } from "react";
import { usePlatformInfo } from "./hooks/store/usePlatformInfo";
import { Platform } from "@aws/amazon-q-developer-cli-api-bindings";
import { matchesPlatformRestrictions } from "./lib/platform";
import { gnomeExtensionInstallCheck } from "./data/install";

function App() {
  const store = useRef(createStore()).current;
  const [listening, setListening] = useState<string | null>(null);

  return (
    <StoreContext.Provider value={store}>
      <ListenerContext.Provider value={{ listening, setListening }}>
        <AppLoading />
      </ListenerContext.Provider>
    </StoreContext.Provider>
  );
}

function AppLoading() {
  const store = useContext(StoreContext);
  if (store === null || store().isLoading()) {
    return <div className="w-screen h-screen bg-white dark:bg-zinc-800"></div>;
  } else {
    return <Router />;
  }
}

function ActiveModal() {
  const auth = useAuth();
  const [onboardingComplete] = useLocalStateZodDefault(
    "desktop.completedOnboarding",
    z.boolean(),
    false,
  );
  const [closed, setClosed] = useState(false);

  if (closed) return null;

  if (onboardingComplete === false) {
    return (
      <Modal>
        <InstallModal />
      </Modal>
    );
  }

  if (onboardingComplete && auth.authed === false) {
    return (
      <Modal>
        <LoginModal next={() => setClosed(true)} />
      </Modal>
    );
  }

  return null;
}

function Router() {
  const navigate = useNavigate();
  const location = useLocation();
  const platformInfo = usePlatformInfo();

  useEffect(() => {
    try {
      Telemetry.page("", location.pathname, { ...location });
    } catch (e) {
      console.error(e);
    }
  }, [location]);

  useEffect(() => {
    let unsubscribe: () => void;
    let isStale = false;
    Event.subscribe("dashboard.navigate", (request) => {
      if (
        typeof request === "object" &&
        request !== null &&
        "path" in request &&
        typeof request.path === "string"
      ) {
        navigate(request.path);
      } else {
        console.error("Invalid dashboard.navigate request", request);
      }

      return { unsubscribe: false };
    })?.then((result) => {
      unsubscribe = result.unsubscribe;
      if (isStale) unsubscribe();
    });
    return () => {
      if (unsubscribe) unsubscribe();
      isStale = true;
    };
  }, [navigate]);

  return (
    <>
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<Onboarding />} />
          <Route path="help" element={<Help />} />
          <Route path="whats-new" element={<WhatsNew />} />
          <Route path="autocomplete" element={<Autocomplete />} />
          <Route path="translate" element={<Translate />} />
          <Route path="chat" element={<Chat />} />
          <Route path="inline" element={<Inline />} />
          <Route path="account" element={<Account />} />
          <Route path="keybindings" element={<Keybindings />} />
          {platformInfo && platformInfo.os === Platform.Os.MACOS && (
            <Route path="integrations" element={<Integrations />} />
          )}
          <Route path="preferences" element={<Preferences />} />
          <Route path="licenses" element={<Licenses />} />
        </Route>
      </Routes>
      <Suspense fallback={<></>}>
        <ActiveModal />
      </Suspense>
    </>
  );
}

const useNavData = () => {
  const platformInfo = usePlatformInfo();
  if (!platformInfo) {
    return [];
  }

  return [
    {
      type: "link",
      name: "Getting started",
      link: "/",
    },
    // {
    //   type: "link",
    //   name: "Getting started",
    //   link: "/onboarding",
    // },
    {
      type: "link",
      name: "What's new?",
      link: "/whats-new",
    },
    {
      type: "link",
      name: "Help & support",
      link: "/help",
    },
    {
      type: "header",
      name: "Features",
    },
    {
      type: "link",
      name: "CLI Completions",
      link: "/autocomplete",
    },
    {
      type: "link",
      name: "Chat",
      link: "/chat",
    },
    {
      type: "link",
      name: "Inline",
      link: "/inline",
    },
    {
      type: "link",
      name: "Translate",
      link: "/translate",
    },
    {
      type: "header",
      name: "Settings",
    },
    // {
    //   type: "link",
    //   name: "Account",
    //   link: "/account",
    // },
    {
      type: "link",
      name: "Keybindings",
      link: "/keybindings",
    },
    {
      type: "link",
      name: "Integrations",
      link: "/integrations",
      platformRestrictions: {
        os: Platform.Os.MACOS,
      },
    },
    {
      type: "link",
      name: "Preferences",
      link: "/preferences",
    },
  ].filter((data) =>
    matchesPlatformRestrictions(platformInfo, data.platformRestrictions),
  );
};

function Layout() {
  const [onboardingComplete] = useLocalStateZodDefault(
    "desktop.completedOnboarding",
    z.boolean(),
    false,
  );
  const platformInfo = usePlatformInfo();
  const [accessibilityCheck] = useAccessibilityCheck();
  const [dotfilesCheck] = useDotfilesCheck();
  const [gnomeExtensionCheck] = useGnomeExtensionCheck();
  const navData = useNavData();
  const error =
    onboardingComplete &&
    (accessibilityCheck === false ||
      dotfilesCheck === false ||
      (platformInfo &&
        matchesPlatformRestrictions(
          platformInfo,
          gnomeExtensionInstallCheck.platformRestrictions,
        ) &&
        gnomeExtensionCheck === false));

  const [notifCount, _] = useLocalStateZodDefault(
    NOTIFICATIONS_SEEN_STATE_KEY,
    z.number(),
    0,
  );

  return (
    <div className="flex flex-row h-screen w-full overflow-hidden bg-white dark:bg-zinc-800 text-black dark:text-zinc-200">
      <nav className="w-[240px] flex-none h-full flex flex-col items-center gap-1 p-4">
        {navData.map((item) =>
          item.type === "link" ? (
            <SidebarLink
              key={item.name}
              path={item.link}
              name={item.name}
              icon={getIconFromName(item.name)}
              count={
                item.link == "/whats-new"
                  ? notificationFeedItems.entries.filter((i) => !i.hidden)
                      .length - notifCount
                  : undefined
              }
              error={item.link == "/help" ? error : undefined}
            />
          ) : (
            <div
              key={item.name}
              className="pt-4 pl-3 text-sm text-zinc-600 dark:text-zinc-400 w-full rounded-lg flex flex-row items-center font-medium select-none"
            >
              {item.name}
            </div>
          ),
        )}
      </nav>
      <main className="flex flex-col overflow-y-auto p-4 w-full">
        <Outlet />
      </main>
    </div>
  );
}

export default App;
