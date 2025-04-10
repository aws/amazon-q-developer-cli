# Shared utils

## ⚠️ This package SHOULDN'T contain states but only effects

Why? This package is not published to a registry and when we require some of its exports in the other packages the required functions/constants are bundled in the requiring package. Bundling COPIES exports in the requiring packages (instead of `node_modules` that "reference" exports). For this reason if we create a shared state in this package and we consume it in two different packages that should work simultaneously it won't work correctly because two different states would be created.