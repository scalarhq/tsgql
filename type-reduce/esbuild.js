require('esbuild').buildSync({
	entryPoints: [  './src/index.js' ],
	bundle: true,
	outfile: 'dist/index.js',
	platform: 'node'
})