import { sendCodewhispererListCustomizationRequest } from "./requests.js";
const listCustomizations = async () => (await sendCodewhispererListCustomizationRequest({})).customizations;
export { listCustomizations };
//# sourceMappingURL=codewhisperer.js.map