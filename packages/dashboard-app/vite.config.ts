import path from "node:path";
import { defineConfig, type HtmlTagDescriptor, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const csp: Record<string, string> = {
  "default-src": "'self'",
  "script-src": "'self'",
  "img-src": "'self'",
  "style-src": "'self'",
  "connect-src": "'self' api:",
  "object-src": "'none'",
  "frame-src": "'none'",
};

const cspContent = Object.entries(csp)
  .map(([k, v]) => `${k} ${v}`)
  .join("; ");

const htmlCspPlugin: Plugin = {
  name: "html-csp",
  transformIndexHtml: {
    order: "post",
    handler: (_html, ctx): HtmlTagDescriptor[] => {
      if (ctx.server?.config?.mode === "development") {
        return [];
      }

      return [
        {
          injectTo: "head",
          tag: "meta",
          attrs: {
            "http-equiv": "Content-Security-Policy",
            content: cspContent,
          },
        },
      ];
    },
  },
};

// https://vitejs.dev/config/
export default defineConfig(({ command }) => ({
  plugins: [react(), htmlCspPlugin],
  server: {
    port: 3433,
  },
  build: {
    target: command === "build" ? "es2017" : "esnext",
    sourcemap: command !== "build",
  },
  esbuild: {
    target: command === "build" ? ["es2017", "safari11"] : undefined,
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@assets": path.resolve(__dirname, "./assets"),
    },
  },
}));
