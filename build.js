const esbuild = require('esbuild');

esbuild.build({
  entryPoints: ['src/index.ts'],
  outfile: 'dist/index.js',
  platform: 'node',
  target: 'node18',
  bundle: true,
  minify: true,
  sourcemap: true,
  external: [],
  loader: {
    '.ts': 'ts'
  }
});
