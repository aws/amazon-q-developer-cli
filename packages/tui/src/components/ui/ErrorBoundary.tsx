import React, { useState, useEffect } from 'react';
import { Box, Text } from 'ink';

interface ErrorBoundaryProps {
  children: React.ReactNode;
}

export const ErrorBoundary: React.FC<ErrorBoundaryProps> = ({ children }) => {
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    // In Node.js/Bun environment, use process events instead of window
    if (typeof process !== 'undefined') {
      process.on('uncaughtException', (err) => {
        setError(err);
      });

      process.on('unhandledRejection', (reason: any) => {
        setError(
          new Error(
            typeof reason === 'string'
              ? reason
              : reason?.message || 'Unhandled promise rejection'
          )
        );
      });
    }

    return () => {
      // Cleanup if needed
    };
  }, []);

  if (error) {
    return (
      <Box flexDirection="column" padding={1}>
        <Text bold color="red">
          Application Error
        </Text>
        <Text color="red">Something went wrong: {error.message}</Text>
        <Text dimColor>Please restart the application</Text>
        <Text dimColor>Press Ctrl+C to exit</Text>
      </Box>
    );
  }

  return <>{children}</>;
};
