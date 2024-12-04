import { createErrorInstance } from "@aws/amazon-q-developer-cli-shared/errors";

export const GenericRequestError = createErrorInstance("GenericRequestError");
export const CredentialsError = createErrorInstance("CredentialsError");
