import { useEffect, useState } from 'react';

export function useTerminalSize(): { width: number; height: number } {
  const [size, setSize] = useState({
    width: process.stdout.columns || 60,
    height: process.stdout.rows || 20,
  });

  useEffect(() => {
    function updateSize() {
      setSize({
        width: process.stdout.columns || 60,
        height: process.stdout.rows || 20,
      });
    }

    process.stdout.on('resize', updateSize);
    return () => {
      process.stdout.off('resize', updateSize);
    };
  }, []);

  return size;
}
