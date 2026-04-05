import React, { useState, useEffect } from 'react';
import { Box, Text } from './../../renderer.js';

interface ErrorBoundaryProps {
  children: React.ReactNode;
}

export const ErrorBoundary: React.FC<ErrorBoundaryProps> = ({ children }) => {
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    if (typeof process === 'undefined') return;

    const handleException = (err: Error) => {
      setError(err);
    };

    const handleRejection = (reason: unknown) => {
      setError(
        new Error(
          typeof reason === 'string'
            ? reason
            : (reason as any)?.message || 'Unhandled promise rejection'
        )
      );
    };

    process.on('uncaughtException', handleException);
    process.on('unhandledRejection', handleRejection);

    return () => {
      process.removeListener('uncaughtException', handleException);
      process.removeListener('unhandledRejection', handleRejection);
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
