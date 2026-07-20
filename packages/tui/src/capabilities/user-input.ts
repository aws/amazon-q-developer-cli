import type {
  ClientCapability,
  UserInputRequest,
  UserInputResponse,
} from '@kiro/acp-type-covenant';

export const USER_INPUT_METHOD = '_kiro/userInput';

export function createUserInputCapability(
  onRequest: (request: UserInputRequest) => Promise<UserInputResponse>
): ClientCapability<
  typeof USER_INPUT_METHOD,
  UserInputRequest & Record<string, unknown>,
  UserInputResponse & Record<string, unknown>
> {
  return {
    type: 'other',
    key: 'userInput',
    value: true,
    method: USER_INPUT_METHOD,
    handler: async (request) => ({ ...(await onRequest(request)) }),
  };
}
