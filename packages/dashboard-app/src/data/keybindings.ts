const keybindings = [
  {
    title: "General",
    actions: [
      {
        id: "autocomplete.toggleHistoryMode",
        title: "Toggle history mode",
        description: `Toggle between history suggestions and autocomplete spec suggestions`,
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: ["control+r"],
      },
      {
        id: "autocomplete.toggleFuzzySearch",
        title: "Toggle fuzzy search",
        description: "Toggle between normal prefix search and fuzzy search",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: [],
      },
    ],
  },
  {
    title: "Appearance",
    actions: [
      {
        id: "autocomplete.increaseSize",
        title: "Increase window size",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: [],
      },
      {
        id: "autocomplete.decreaseSize",
        title: "Decrease window size",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: [],
      },
      {
        id: "autocomplete.toggleAutocomplete",
        title: "Toggle autocomplete",
        description: "Toggle the visibility of the autocomplete window",
        availability: "ALWAYS",
        type: "keystrokes",
        default: [],
      },
      // {
      //   id: "autocomplete.hideAutocomplete",
      //   title: "Hide autocomplete",
      //   "category": "General",
      //   description: "Hide the autocomplete window",
      //   availability: "ALWAYS",
      // type: 'keystrokes',
      //   default: ["esc"]
      // },
      // {
      //   id: "autocomplete.showAutocomplete",
      //   title: "Show autocomplete",
      //   "category": "General",
      //   description: "Show the autocomplete window",
      //   availability: "ALWAYS",
      // type: 'keystrokes',
      //   default: []
      // },
      {
        id: "autocomplete.toggleDescription",
        title: "Toggle description popout",
        description: "Toggle visibility of autocomplete description popout",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: ["control+k"],
      },
      // {
      //   id: "autocomplete.hideDescription",
      //   title: "Hide description popout",
      //   category: "Appearance",
      //   description: "Hide autocomplete description popout",
      //   availability: "WHEN_FOCUSED",
      //   type: 'keystrokes',
      //   default: []
      // },
      // {
      //   id: "autocomplete.showDescription",
      //   title: "Show description popout",
      //   category: "Appearance",
      //   description: "Show autocomplete description popout",
      //   availability: "WHEN_FOCUSED",
      //   type: 'keystrokes',
      //   default: []
      // },
    ],
  },
  {
    title: "Insertion",
    actions: [
      {
        id: "autocomplete.insertSelected",
        title: "Insert selected",
        description: "Insert selected suggestion",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: ["enter"],
      },
      {
        id: "autocomplete.insertCommonPrefix",
        title: "Insert common prefix or shake",
        description:
          "Insert shared prefix of available suggestions. Shake if there's no common prefix.",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: ["tab"],
      },
      {
        id: "autocomplete.insertCommonPrefixOrNavigateDown",
        title: "Insert common prefix or navigate",
        description:
          "Insert shared prefix of available suggestions. Navigate if there's no common prefix.",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: [],
      },
      {
        id: "autocomplete.insertCommonPrefixOrInsertSelected",
        title: "Insert common prefix or insert selected",
        description:
          "Insert shared prefix of available suggestions. Insert currently selected suggestion if there's not common prefix.",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: [],
      },
      {
        id: "autocomplete.insertSelectedAndExecute",
        title: "Insert selected and execute",
        description:
          "Insert selected suggestion and then execute the current command.",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: [],
      },
      {
        id: "autocomplete.execute",
        title: "Execute",
        description: "Execute the current command.",
        availability: "WHEN_FOCUSED",
        type: "keystrokes",
        default: [],
      },
    ],
  },
  {
    title: "Navigation",
    actions: [
      {
        id: "autocomplete.navigateUp",
        title: "Navigate up",
        type: "keystrokes",
        description: "Scroll up one entry in the list of suggestions",
        availability: "WHEN_FOCUSED",
        default: ["shift+tab", "up", "control+p"],
      },
      {
        id: "autocomplete.navigateDown",
        title: "Navigate down",
        type: "keystrokes",
        description: "Scroll down one entry in the list of suggestions",
        availability: "WHEN_FOCUSED",
        default: ["down", "control+n"],
      },
    ],
  },
];

export default keybindings;
