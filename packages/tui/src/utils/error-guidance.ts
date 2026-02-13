import type { AuthErrorType, SessionErrorType } from '../types/agent-events.js';

/**
 * CLI binary name constant
 */
export const CLI_BINARY_NAME = 'kiro-cli';

/**
 * Error categories for guidance mapping
 */
export type ErrorCategory =
  | 'auth'
  | 'session'
  | 'network'
  | 'permission'
  | 'mcp'
  | 'rate_limit'
  | 'tool'
  | 'unknown';

/**
 * Error guidance result containing the guidance message and optional recovery action
 */
export interface ErrorGuidance {
  message: string;
  recoveryAction?: string;
}

/**
 * Simplify technical error messages for user display
 */
export const simplifyErrorMessage = (errorMessage: string): string => {
  const lowerMessage = errorMessage.toLowerCase();

  // Auth errors - show a simple message
  if (lowerMessage.includes('no token')) {
    return 'Not authenticated';
  }
  if (lowerMessage.includes('token expired')) {
    return 'Session expired';
  }

  // Network errors - simplify
  if (lowerMessage.includes('error sending request')) {
    return 'Network error - unable to connect';
  }
  if (lowerMessage.includes('i/o error') || lowerMessage.includes('io error')) {
    // Extract the URL if present for context
    const urlMatch = errorMessage.match(/url \(([^)]+)\)/i);
    if (urlMatch) {
      return 'Network error - unable to reach service';
    }
    return 'Network error';
  }

  // Remove technical prefixes
  let simplified = errorMessage;

  // Remove "Encountered an error in the response stream: " prefix
  const streamPrefix = 'Encountered an error in the response stream: ';
  if (simplified.startsWith(streamPrefix)) {
    simplified = simplified.slice(streamPrefix.length);
  }

  // Remove "An unknown error occurred: " prefix
  const unknownPrefix = 'An unknown error occurred: ';
  if (simplified.startsWith(unknownPrefix)) {
    simplified = simplified.slice(unknownPrefix.length);
  }

  // Simplify dispatch failure messages
  if (simplified.toLowerCase().startsWith('dispatch failure')) {
    // Extract the root cause after the last " - "
    const parts = simplified.split(' - ');
    if (parts.length > 1) {
      const rootCause = parts[parts.length - 1];
      if (rootCause) {
        simplified = rootCause.charAt(0).toUpperCase() + rootCause.slice(1);
      }
    }
  }

  return simplified;
};

/**
 * Get guidance for authentication errors
 */
export const getAuthErrorGuidance = (
  errorType: AuthErrorType
): ErrorGuidance => {
  const loginCmd = `${CLI_BINARY_NAME} login`;

  switch (errorType) {
    case 'no_token':
      return {
        message: `Please run "${loginCmd}" to authenticate.`,
        recoveryAction: loginCmd,
      };
    case 'token_expired':
      return {
        message: `Your session has expired. Please run "${loginCmd}" to re-authenticate.`,
        recoveryAction: loginCmd,
      };
    case 'oauth_timeout':
      return {
        message: `Authentication timed out. Please try "${loginCmd}" again.`,
        recoveryAction: loginCmd,
      };
    case 'oauth_state_mismatch':
      return {
        message: `Authentication failed due to security validation. Please try "${loginCmd}" again.`,
        recoveryAction: loginCmd,
      };
    case 'social_auth_failure':
      return {
        message: `Social login failed. Please try "${loginCmd}" with a different method.`,
        recoveryAction: loginCmd,
      };
    case 'unauthorized_client':
      return {
        message: 'This application is not authorized. Please contact support.',
      };
    case 'unauthorized':
    default:
      return {
        message: `Authentication required. Please run "${loginCmd}" to authenticate.`,
        recoveryAction: loginCmd,
      };
  }
};

/**
 * Get guidance for session errors
 */
export const getSessionErrorGuidance = (
  errorType: SessionErrorType,
  pid?: number
): ErrorGuidance => {
  switch (errorType) {
    case 'session_locked':
      return {
        message: pid
          ? `Session is locked by another process (PID: ${pid}). Please close the other instance and try again.`
          : 'Session is locked by another process. Please close the other instance and try again.',
      };
    case 'session_not_found':
      return {
        message: 'Session not found. A new session will be created.',
      };
    case 'io_error':
      return {
        message:
          'Failed to access session data. Please check file permissions and try again.',
      };
    case 'json_parse_error':
      return {
        message:
          'Session data is corrupted. Please try starting a new session.',
      };
    default:
      return {
        message: 'A session error occurred. Please try again.',
      };
  }
};

/**
 * Get guidance for network-related errors
 */
export const getNetworkErrorGuidance = (
  errorMessage: string
): ErrorGuidance => {
  const lowerMessage = errorMessage.toLowerCase();

  if (lowerMessage.includes('timeout')) {
    return {
      message:
        'Connection timed out. Please check your network connection and try again.',
    };
  }

  if (
    lowerMessage.includes('econnrefused') ||
    lowerMessage.includes('connection refused')
  ) {
    return {
      message:
        'Connection refused. Please ensure the service is running and try again.',
    };
  }

  if (lowerMessage.includes('enotfound') || lowerMessage.includes('dns')) {
    return {
      message: 'Could not resolve host. Please check your network connection.',
    };
  }

  return {
    message:
      'Network error occurred. Please check your connection and try again.',
  };
};

/**
 * Get guidance for permission errors
 */
export const getPermissionErrorGuidance = (
  errorMessage: string
): ErrorGuidance => {
  const lowerMessage = errorMessage.toLowerCase();

  if (
    lowerMessage.includes('eacces') ||
    lowerMessage.includes('permission denied')
  ) {
    return {
      message:
        'Permission denied. Please check file permissions or run with appropriate privileges.',
    };
  }

  if (lowerMessage.includes('eperm')) {
    return {
      message: 'Operation not permitted. You may need elevated privileges.',
    };
  }

  return {
    message: 'Permission error. Please check your access rights.',
  };
};

/**
 * Get guidance for MCP server errors
 */
export const getMcpErrorGuidance = (
  serverName: string,
  error: string
): ErrorGuidance => {
  const lowerError = error.toLowerCase();

  if (lowerError.includes('not found') || lowerError.includes('enoent')) {
    return {
      message: `MCP server "${serverName}" executable not found. Please check your MCP configuration.`,
    };
  }

  if (lowerError.includes('timeout')) {
    return {
      message: `MCP server "${serverName}" timed out during initialization. The server may be slow to start.`,
    };
  }

  return {
    message: `MCP server "${serverName}" failed to initialize. Check the server configuration and logs.`,
  };
};

/**
 * Get guidance for rate limit errors
 */
export const getRateLimitGuidance = (): ErrorGuidance => {
  return {
    message: 'Rate limit exceeded. Please wait a moment before trying again.',
  };
};

/**
 * Get guidance for tool execution errors
 */
export const getToolErrorGuidance = (
  toolName: string,
  error: string
): ErrorGuidance => {
  const lowerError = error.toLowerCase();

  if (lowerError.includes('timeout')) {
    return {
      message: `Tool "${toolName}" timed out. The operation may have taken too long.`,
    };
  }

  if (lowerError.includes('permission') || lowerError.includes('denied')) {
    return {
      message: `Tool "${toolName}" was denied permission. You may need to approve the action.`,
    };
  }

  return {
    message: `Tool "${toolName}" encountered an error. The conversation can continue.`,
  };
};

/**
 * Detect error category from error message
 */
export const detectErrorCategory = (errorMessage: string): ErrorCategory => {
  const lowerMessage = errorMessage.toLowerCase();

  // Check for auth-related errors first (most specific)
  if (
    lowerMessage.includes('no token') ||
    lowerMessage.includes('token expired') ||
    lowerMessage.includes('unauthorized') ||
    lowerMessage.includes('not authenticated') ||
    lowerMessage.includes('authentication required') ||
    // Dispatch failures with auth-related causes
    (lowerMessage.includes('dispatch failure') &&
      lowerMessage.includes('token'))
  ) {
    return 'auth';
  }

  // Generic auth keywords (less specific)
  if (lowerMessage.includes('auth') || lowerMessage.includes('login')) {
    return 'auth';
  }

  if (lowerMessage.includes('session')) {
    return 'session';
  }

  if (
    lowerMessage.includes('network') ||
    lowerMessage.includes('connection') ||
    lowerMessage.includes('timeout') ||
    lowerMessage.includes('econnrefused') ||
    lowerMessage.includes('enotfound') ||
    lowerMessage.includes('i/o error') ||
    lowerMessage.includes('io error') ||
    lowerMessage.includes('error sending request')
  ) {
    return 'network';
  }

  if (
    lowerMessage.includes('permission') ||
    lowerMessage.includes('eacces') ||
    lowerMessage.includes('eperm')
  ) {
    return 'permission';
  }

  if (lowerMessage.includes('mcp')) {
    return 'mcp';
  }

  if (
    lowerMessage.includes('rate limit') ||
    lowerMessage.includes('throttle') ||
    lowerMessage.includes('throttling')
  ) {
    return 'rate_limit';
  }

  if (lowerMessage.includes('tool')) {
    return 'tool';
  }

  return 'unknown';
};

/**
 * Get guidance for any error message by detecting its category
 */
export const getErrorGuidance = (errorMessage: string): ErrorGuidance => {
  const category = detectErrorCategory(errorMessage);
  const loginCmd = `${CLI_BINARY_NAME} login`;

  switch (category) {
    case 'auth':
      return {
        message: `Please run "${loginCmd}" to authenticate.`,
        recoveryAction: loginCmd,
      };
    case 'network':
      return getNetworkErrorGuidance(errorMessage);
    case 'permission':
      return getPermissionErrorGuidance(errorMessage);
    case 'rate_limit':
      return getRateLimitGuidance();
    case 'session':
      return {
        message: 'A session error occurred. Please try again.',
      };
    case 'mcp':
      return {
        message: 'An MCP server error occurred. Check your MCP configuration.',
      };
    case 'tool':
      return {
        message:
          'A tool execution error occurred. The conversation can continue.',
      };
    default:
      return {
        message: 'An error occurred. Please try again.',
      };
  }
};
