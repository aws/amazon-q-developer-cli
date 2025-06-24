<p align="center">
    <img width="300" src="https://github.com/withfig/fig/blob/main/static/FigBanner.png?raw=true"/>
</p>

---

# Autocomplete v9

This is where all the changes to autocomplete are made. This readme will
be updated for each version of autocomplete

## Folders and Files

- `fig/`: Wrappers around the fig.js API to make it nicer to work with
- `parser/`: Parses command line output using a bash parser and then
  a contextual parser that incorporates completion specs
- `generators/`: Launches asynchronous requests to generator suggestions
  dynamically
- `suggestions/`: Utilities to compute, sort, and filter a list of
  suggestions from parser results and generators that have completed
- `state/`: Manages react state including figState
  from fig.js hooks, parser results, generator results, suggestions to
  display, and visibility of the window
- `hooks/keypress.ts`: Handles keypresses from the user, updating state
  accordingly

## How it works

The primary logic for the app is contained within
`state/` and `App.tsx` which synthesize the parts listed
above into the autocomplete app. In particular,
`state/` lays out a reducer that dictates all of the
ways the state of the fig app can change. We want to update state at the
following times:

- We receive an autocomplete event from the fig.js API. We call
  `setFigState` to update the current buffer, cwd, etc. in our `autocompleteState`
- The buffer has changed. We must re-parse the new buffer. This is
  done asynchronous because of the need to load spec files from disk.
  When the parser is complete we call `updateParserResult`.
- A generator has completed. If the results aren't stale, we must update
  our generator state with the new results, calling
  `updateGeneratorResult`
- We receive a keypress even from the fig.js API. Depending on the
  keypress, we might need to update the selected item index, or possibly
  hide the autocomplete window. We call `hide` or `scroll` to update state.
- An unrecoverable error occurs. We want to hide the fig window until we
  can show something again.

The functions/actions mentioned above are all defined in
`state/`.

Every time we update the state in one of the ways mentioned above, we
might need to get a new list of suggestions. We also might need to trigger
generators if the search term that the user typed for the current argument
has changed. Every time we update state, we do a few checks to see if we
need to regenerate suggestions or trigger generators.

## Versioning

New versions of autocomplete are created when changes are made to the
autocomplete spec format. For example, if we add “generators” as
a property of args, this is a change. AND changes to the fig hooks that
are sent

## Testing locally

You can launch the autocomplete engine locally by running:

```bash
npm run dev
```

which will start serving the repo with hot reloading on `localhost:3000`

**Tip**: to view logs in the console when their source is displaying instrument.ts, right click anywhere it says "instrument.ts" and click "Blackbox Script" in the option menu
