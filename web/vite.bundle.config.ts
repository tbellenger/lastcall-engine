import { defineConfig, type Plugin } from "vite";

// Library mode keeps the mountPlayer export. It also inlines every asset, so the
// wasm URL opts out with Vite's ?no-inline and is emitted as a real file.
function wasmNoInline(): Plugin {
  return {
    name: "wasm-no-inline",
    enforce: "pre",
    transform(code, id) {
      if (!id.includes("lastcall_wasm.js")) {
        return null;
      }
      const next = code.replace(
        "new URL('lastcall_wasm_bg.wasm', import.meta.url)",
        "new URL('lastcall_wasm_bg.wasm?no-inline', import.meta.url)",
      );
      return next === code ? null : next;
    },
  };
}

export default defineConfig({
  base: "./",
  publicDir: false,
  plugins: [wasmNoInline()],
  build: {
    outDir: "dist-bundle",
    emptyOutDir: true,
    cssCodeSplit: false,
    assetsInlineLimit: 0,
    minify: false,
    sourcemap: false,
    lib: {
      entry: "src/mount.ts",
      formats: ["es"],
      fileName: () => "player.js",
      cssFileName: "player",
    },
    rollupOptions: {
      output: {
        assetFileNames: (info) => {
          const name = info.names?.[0] ?? info.name ?? "";
          if (name.endsWith(".css")) {
            return "player.css";
          }
          if (name.endsWith(".wasm")) {
            return "assets/[name][extname]";
          }
          return "assets/[name]-[hash][extname]";
        },
      },
    },
  },
});
