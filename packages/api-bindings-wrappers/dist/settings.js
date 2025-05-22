export var SETTINGS;
(function (SETTINGS) {
    // Dev settings.
    SETTINGS["DEV_MODE"] = "autocomplete.developerMode";
    SETTINGS["DEV_MODE_NPM"] = "autocomplete.developerModeNPM";
    SETTINGS["DEV_MODE_NPM_INVALIDATE_CACHE"] = "autocomplete.developerModeNPMInvalidateCache";
    SETTINGS["DEV_COMPLETIONS_FOLDER"] = "autocomplete.devCompletionsFolder";
    SETTINGS["DEV_COMPLETIONS_SERVER_PORT"] = "autocomplete.devCompletionsServerPort";
    // Style settings
    SETTINGS["WIDTH"] = "autocomplete.width";
    SETTINGS["HEIGHT"] = "autocomplete.height";
    SETTINGS["THEME"] = "autocomplete.theme";
    SETTINGS["USER_STYLES"] = "autocomplete.userStyles";
    SETTINGS["FONT_FAMILY"] = "autocomplete.fontFamily";
    SETTINGS["FONT_SIZE"] = "autocomplete.fontSize";
    SETTINGS["CACHE_ALL_GENERATORS"] = "beta.autocomplete.auto-cache";
    // Behavior settings
    SETTINGS["DISABLE_FOR_COMMANDS"] = "autocomplete.disableForCommands";
    SETTINGS["IMMEDIATELY_EXEC_AFTER_SPACE"] = "autocomplete.immediatelyExecuteAfterSpace";
    SETTINGS["IMMEDIATELY_RUN_DANGEROUS_COMMANDS"] = "autocomplete.immediatelyRunDangerousCommands";
    SETTINGS["IMMEDIATELY_RUN_GIT_ALIAS"] = "autocomplete.immediatelyRunGitAliases";
    SETTINGS["INSERT_SPACE_AUTOMATICALLY"] = "autocomplete.insertSpaceAutomatically";
    SETTINGS["SCROLL_WRAP_AROUND"] = "autocomplete.scrollWrapAround";
    SETTINGS["SORT_METHOD"] = "autocomplete.sortMethod";
    SETTINGS["ALWAYS_SUGGEST_CURRENT_TOKEN"] = "autocomplete.alwaysSuggestCurrentToken";
    SETTINGS["NAVIGATE_TO_HISTORY"] = "autocomplete.navigateToHistory";
    SETTINGS["ONLY_SHOW_ON_TAB"] = "autocomplete.onlyShowOnTab";
    SETTINGS["ALWAYS_SHOW_DESCRIPTION"] = "autocomplete.alwaysShowDescription";
    SETTINGS["HIDE_PREVIEW"] = "autocomplete.hidePreviewWindow";
    SETTINGS["SCRIPT_TIMEOUT"] = "autocomplete.scriptTimeout";
    SETTINGS["PREFER_VERBOSE_SUGGESTIONS"] = "autocomplete.preferVerboseSuggestions";
    SETTINGS["HIDE_AUTO_EXECUTE_SUGGESTION"] = "autocomplete.hideAutoExecuteSuggestion";
    SETTINGS["FUZZY_SEARCH"] = "autocomplete.fuzzySearch";
    SETTINGS["PERSONAL_SHORTCUTS_TOKEN"] = "autocomplete.personalShortcutsToken";
    SETTINGS["DISABLE_HISTORY_LOADING"] = "autocomplete.history.disableLoading";
    // History settings
    // one of "off", "history_only", "show"
    SETTINGS["HISTORY_MODE"] = "beta.history.mode";
    SETTINGS["HISTORY_COMMAND"] = "beta.history.customCommand";
    SETTINGS["HISTORY_MERGE_SHELLS"] = "beta.history.allShells";
    SETTINGS["HISTORY_CTRL_R_TOGGLE"] = "beta.history.ctrl-r";
    SETTINGS["FIRST_COMMAND_COMPLETION"] = "autocomplete.firstTokenCompletion";
    SETTINGS["TELEMETRY_ENABLED"] = "telemetry.enabled";
})(SETTINGS || (SETTINGS = {}));
let settings = {};
export const updateSettings = (newSettings) => {
    settings = newSettings;
};
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const getSetting = (key, defaultValue) => settings[key] ?? defaultValue;
export const getSettings = () => settings;
export function isInDevMode() {
    return (Boolean(getSetting(SETTINGS.DEV_MODE)) ||
        Boolean(getSetting(SETTINGS.DEV_MODE_NPM)));
}
//# sourceMappingURL=settings.js.map