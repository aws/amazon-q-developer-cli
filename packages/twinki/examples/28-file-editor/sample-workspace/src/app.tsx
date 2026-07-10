import { useState } from 'react';
import { clamp } from './utils/math.js';

export function Counter({ max = 10 }) {
  const [count, setCount] = useState(0);
  const bump = () => setCount((n) => clamp(n + 1, 0, max));
  return <button onClick={bump}>Count: {count}</button>;
}
