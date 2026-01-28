import React, { useEffect, createContext, useContext } from 'react';
import { Box } from 'ink';
import { enableAutoSync, detectSynchronizedOutput } from '../../../utils/synchronized-output.js';
import { Divider } from '../divider/Divider.js';

// Card Context - provides active state to children
interface CardContextType {
  active: boolean;
}

const CardContext = createContext<CardContextType>({ active: false });

export const useCardContext = () => useContext(CardContext);

export interface CardProps {
  children: React.ReactNode[] | React.ReactNode;
  active?: boolean;
}

// Detect terminals that fill edge margins (e.g., background color bleeding into terminal margins)
const fillsEdgeMargin = process.env.TERM_PROGRAM === 'iTerm.app';

export function Card({ children, active = false }: CardProps) {
  // Enable automatic synchronized output for supported terminals (once per app)
  useEffect(() => {
    if (detectSynchronizedOutput()) {
      enableAutoSync();
    }
  }, []);

  return (
    <CardContext.Provider value={{ active }}>
      <Box flexDirection="column" width="100%">
        <Divider />
        <Box flexDirection="column" width="100%" marginLeft={fillsEdgeMargin ? 1 : 0}>
          {children}
        </Box>
      </Box>
    </CardContext.Provider>
  );
}
