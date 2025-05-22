export const createErrorInstance = (name) => class extends Error {
    constructor(message) {
        super(message);
        this.name = `AmazonQ.${name}`;
    }
};
//# sourceMappingURL=errors.js.map