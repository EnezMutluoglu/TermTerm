import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: [
        "**/src-tauri/**",
        "**/target/**",
        "**/.tools/**",
        "**/.lab/**",
        "**/artifacts/**",
      ],
    },
  },
  build: {
    outDir: mode === "vpn-dev" ? "dist-vpn-dev" : "dist",
    target: "es2022",
    rollupOptions: {
      output: {
        manualChunks: {
          terminal: [
            "@xterm/xterm",
            "@xterm/addon-fit",
            "@xterm/addon-search",
            "@xterm/addon-webgl",
          ],
        },
      },
    },
  },
}));
