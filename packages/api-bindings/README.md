# TS API Bindings

The Typescript definitions for the Amazon Q for command line API.

Note: The protobuf definitions are updated automatically whenever withfig/proto-api is changed.

## Documenting the API

We use [TSDocs](https://tsdoc.org) to comment the exported namespaces of the API.

> See `docs-generators/fig-api` folder in `public-site-nextjs` repo for reference.

### Supported TSDoc tags and custom TSDoc tags

- @param: The params of an exported function, the format MUST be `@param <name of the param> <some description>`. It can be provided multiple times for different params.
  ```ts
  /**
   * @param notification some description for the notification param
   */
  export function subscribe(notification) {}
  ```
- @returns: An explanation of what is returned by an exported function.
- @remarks: Further details about the implementation of the method, use cases...etc. This data will appear in the `Discussion` section.
- @example: Provide examples about the usage of the API object. It is repeatable.
- @prop: Provide a description for one property of the exported object, the format MUST be `@prop <name of the property> - <some description>`. It can be provided multiple times for different properties.
  ```ts
  /**
   * @prop subscribe - a description
   * @prop unsubscribe - a description
   */
  export const didChange = {
    subscribe: (notification) => {},
    unsubscribe: (notification) => {},
  };
  ```
- @excluded: To exclude some symbol from the docs. It should not be used.
- @deprecated: Mark an API as deprecated providing an optional message about the deprecation.
  ```ts
  /**
   * @deprecated This message is optional
   */
  export const didChange = {
    subscribe: (notification) => {},
  };
  ```

### What will appear in the documentation?

Our API bindings export a list of named namespace objects each one exporting a group of Symbols.
In our docs file we document each of the exported symbols grouped according to their namespace.

### Publishing

This package will be published automatically when pushed to any branch of
this repo.
