# web/raster

The renderer behind `onto raster`: Apache ECharts (Canvas), TypeScript,
bundled by esbuild into `crates/onto-cli/assets/raster.js`, which the
`onto` binary embeds into a self-contained HTML page. The built file is
committed, so building onto needs no Node.

```sh
cd web/raster
npm install
npm run build     # type-check, then bundle into crates/onto-cli/assets/raster.js
```

The renderer reads only the typed projection of
`crates/onto-runtime/src/trace.rs` (mirrored in `src/types.ts`); it never
reads telemetry. Another renderer (deck.gl for very large runs, a static
figure exporter) can consume the same projection.

Apache ECharts is Apache-2.0; its license and notice ship next to the
bundle (`crates/onto-cli/assets/ECHARTS-LICENSE.txt`,
`ECHARTS-NOTICE.txt`) and its legal comments are kept at the end of the
bundle.
