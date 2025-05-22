const allCaches = [];
export const createCache = () => {
    const cache = new Map();
    allCaches.push(cache);
    return cache;
};
export const resetCaches = () => {
    allCaches.forEach((cache) => {
        cache.clear();
    });
};
window.resetCaches = resetCaches;
export const specCache = createCache();
export const generateSpecCache = createCache();
window.listCache = () => {
    console.log(specCache);
    console.log(generateSpecCache);
};
//# sourceMappingURL=caches.js.map