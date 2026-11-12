/**
 * EasyOffice overlay build of rhwp-studio (PD-081), following the reference app's studio-host pattern:
 * upstream modules are aliased to third_party/rhwp/rhwp-studio/src, the modules listed in
 * config/rhwp-studio-overrides.json are replaced by the files in ./src, and the vendored
 * WASM package (vendor/rhwp-core) is used instead of a local wasm-pack build.
 *
 * Output: crates/easyoffice-app/ui/hwp-studio (generated, not tracked).
 */
import { defineConfig, normalizePath } from 'vite';
import type { Plugin } from 'vite';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { verifyOverrides, readManifest } from '../../scripts/hwp-studio-overrides.mjs';
import { applyKoOverrides, readKoOverrides } from '../../scripts/hwp-studio-ko-overrides.mjs';

const here = import.meta.dirname;
const repoRoot = resolve(here, '../..');
const upstreamStudio = resolve(repoRoot, 'third_party/rhwp/rhwp-studio');
const upstreamSrc = resolve(upstreamStudio, 'src');
const upstreamPublic = resolve(upstreamStudio, 'public');
const overlaySrc = resolve(here, 'src');
const staticDir = resolve(here, 'static');
const vendorWasm = resolve(repoRoot, 'vendor/rhwp-core/rhwp.js');
const fontsDir = resolve(repoRoot, 'assets/fonts');
const outDir = resolve(repoRoot, 'crates/easyoffice-app/ui/hwp-studio');
const upstreamConfig = JSON.parse(readFileSync(resolve(repoRoot, 'config/rhwp-upstream.json'), 'utf-8'));

const manifestErrors = verifyOverrides(repoRoot, upstreamConfig);
if (manifestErrors.length > 0) {
  throw new Error(`hwp-studio override manifest is invalid:\n${manifestErrors.join('\n')}`);
}
const manifest = readManifest(repoRoot);

function listFiles(dir: string, extension: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((name) => name.endsWith(extension) && statSync(resolve(dir, name)).isFile())
    .sort((a, b) => a.localeCompare(b));
}

const bundledFontFiles = listFiles(fontsDir, '.woff2').map((name) => `fonts/${name}`);

/**
 * Replaces an upstream module with its overlay counterpart no matter how it is imported
 * (`@/core/font-loader`, `./font-loader.ts`, `../core/font-loader`). Resolving first and
 * mapping the resolved file keeps relative imports inside upstream sources covered.
 */
function easyOfficeOverrides(): Plugin {
  const replacements = new Map<string, string>();
  for (const { id } of manifest.overrides as Array<{ id: string }>) {
    replacements.set(normalizePath(resolve(upstreamSrc, `${id}.ts`)), normalizePath(resolve(overlaySrc, `${id}.ts`)));
  }
  return {
    name: 'easyoffice-overrides',
    enforce: 'pre',
    async resolveId(source, importer, options) {
      if (!importer) return null;
      const resolved = await this.resolve(source, importer, { ...options, skipSelf: true });
      if (!resolved) return null;
      const replacement = replacements.get(normalizePath(resolved.id.split('?')[0]));
      if (!replacement || normalizePath(importer) === replacement) return null;
      return replacement;
    },
  };
}

/**
 * The generated font rule catalog lists CDN URLs for fonts that are not bundled. They are never
 * fetched (the fork of font-loader only requests bundled files), but the literals are rewritten
 * to an inert scheme so the bundle contains no external URL and a future bug cannot reach one.
 */
function neutralizeExternalFontUrls(): Plugin {
  return {
    name: 'easyoffice-neutralize-external-font-urls',
    enforce: 'pre',
    transform(code, id) {
      if (!normalizePath(id).includes('/core/generated/font-rule-projections/')) return null;
      const inert = code.replace(/https?:\/\/cdn\.jsdelivr\.net\//g, 'blocked-external-font:').replace(/https?:\/\//g, 'blocked-external-font://');
      return { code: inert, map: null };
    },
  };
}

/** Rewrites single values of the upstream ko catalog (config/rhwp-studio-ko-overrides.json). */
function koLocaleOverrides(): Plugin {
  const overrides = readKoOverrides(repoRoot);
  return {
    name: 'easyoffice-ko-locale-overrides',
    enforce: 'pre',
    transform(code, id) {
      if (!normalizePath(id.split('?')[0]).endsWith('/i18n/locales/ko.ts')) return null;
      return { code: applyKoOverrides(code, overrides), map: null };
    },
  };
}

/** Emits the non-bundled static files: init scripts, print surface and the shipped fonts. */
function easyOfficeStaticAssets(): Plugin {
  const emit = (ctx: { emitFile: (file: { type: 'asset'; fileName: string; source: Buffer }) => string }, fileName: string, file: string) => {
    ctx.emitFile({ type: 'asset', fileName, source: readFileSync(file) });
  };
  return {
    name: 'easyoffice-static-assets',
    generateBundle(_options, bundle) {
      // The upstream stylesheet points the toolbar icon sprite at the site root (`/images/...`); the
      // bundle is hosted below /hwp-studio/, so the stylesheet (assets/*.css) reaches it as ../images/.
      for (const file of Object.values(bundle)) {
        if (file.type === 'asset' && file.fileName.endsWith('.css') && typeof file.source === 'string') {
          file.source = file.source.replaceAll('url(/images/', 'url(../images/');
        }
      }
      for (const rel of manifest.verbatimAssets as string[]) {
        emit(this, rel.replace(/^public\//, ''), resolve(upstreamStudio, rel));
      }
      for (const name of ['.html', '.css', '.js'].flatMap((extension) => listFiles(staticDir, extension))) {
        emit(this, name, resolve(staticDir, name));
      }
      for (const name of listFiles(fontsDir, '.woff2')) emit(this, `fonts/${name}`, resolve(fontsDir, name));
      for (const name of listFiles(resolve(fontsDir, 'licenses'), '.txt')) {
        emit(this, `fonts/licenses/${name}`, resolve(fontsDir, 'licenses', name));
      }
    },
  };
}

export default defineConfig({
  base: './',
  root: here,
  publicDir: false,
  // Keep Vite's cache out of the read-only upstream checkout.
  cacheDir: resolve(here, 'node_modules/.vite'),
  plugins: [easyOfficeOverrides(), neutralizeExternalFontUrls(), koLocaleOverrides(), easyOfficeStaticAssets()],
  define: {
    __APP_VERSION__: JSON.stringify(upstreamConfig.version),
    __RHWP_DISABLE_EXTERNAL_WEBFONTS__: JSON.stringify(true),
    __RHWP_HWPCTRL__: JSON.stringify(false),
    __EO_BUNDLED_FONT_FILES__: JSON.stringify(bundledFontFiles),
  },
  resolve: {
    alias: [
      { find: '@wasm/rhwp.js', replacement: vendorWasm },
      { find: /^@noble\/hashes\/(.*)$/, replacement: `${resolve(here, 'node_modules/@noble/hashes')}/$1` },
      { find: '@upstream', replacement: upstreamSrc },
      { find: '@', replacement: upstreamSrc },
    ],
  },
  build: {
    outDir,
    emptyOutDir: true,
    sourcemap: false,
    chunkSizeWarningLimit: 4000,
    target: 'es2022',
  },
  server: {
    host: '127.0.0.1',
    port: 7711,
    fs: { allow: [here, upstreamStudio, upstreamPublic, resolve(repoRoot, 'vendor/rhwp-core'), fontsDir] },
  },
});
