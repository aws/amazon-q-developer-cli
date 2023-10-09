const settings = [{
  title: 'Translate',
  properties: [
    {
      id: "ai.terminal-hash-sub",
      title: "Hashtag Substitution",
      description: "Comments in the shell will be substituted with the `fig ai` command.",
      type: "boolean",
      default: true,
      inverted: true,
    },
    {
      id: "ai.menu-actions",
      title: "Menu Actions",
      description: "The actions that will be available in the AI menu.",
      type: "multiselect",
      options: ["execute", "edit", "copy", "regenerate", "ask", "cancel"],
      default: ["execute", "edit", "regenerate", "ask", "cancel"],
      inverted: true,
    },
  ]
}]

export default settings