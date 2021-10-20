require('esbuild').buildSync({
	entryPoints: [  './src/index.js' ],
	bundle: true,
	outfile: 'out.js',
	platform: 'node'
})