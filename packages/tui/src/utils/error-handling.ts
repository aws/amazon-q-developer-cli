export const getErrorMessage = (error: unknown): string => {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  return 'An unknown error occurred';
};

export const isNetworkError = (error: unknown): boolean => {
  const message = getErrorMessage(error).toLowerCase();
  return (
    message.includes('network') ||
    message.includes('connection') ||
    message.includes('timeout') ||
    message.includes('econnrefused') ||
    message.includes('enotfound')
  );
};

export const isPermissionError = (error: unknown): boolean => {
  const message = getErrorMessage(error).toLowerCase();
  return (
    message.includes('permission') ||
    message.includes('eacces') ||
    message.includes('eperm')
  );
};

export const formatErrorForUser = (error: unknown): string => {
  const message = getErrorMessage(error);

  if (isNetworkError(error)) {
    return `Connection error: ${message}. Please check your network connection.`;
  }

  if (isPermissionError(error)) {
    return `Permission error: ${message}. Please check file permissions.`;
  }

  return message;
};
