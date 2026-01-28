import { render as inkRender } from 'ink-testing-library';
import { act } from 'react';
import type React from 'react';

/**
 * Test utility wrapper around ink-testing-library's render function.
 *
 * Wraps the render call in React's act() to ensure state updates are
 * properly batched during testing.
 *
 * @param tree - React element to render in the test terminal
 * @returns ink-testing-library render result with lastFrame(), stdin, etc.
 */
export const render = (
  tree: React.ReactElement
): ReturnType<typeof inkRender> => {
  let renderResult: ReturnType<typeof inkRender>;

  act(() => {
    renderResult = inkRender(tree);
  });

  return renderResult!;
};
