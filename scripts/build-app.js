#!/usr/bin/env node

import chokidar from 'chokidar';
import { build } from 'esbuild';
import compress from 'esbuild-plugin-compress';
import { copy } from 'esbuild-plugin-copy';
import { createServer } from 'esbuild-server';
import { glob } from 'glob';
import { spawn } from 'node:child_process';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { getConfig } from './config.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const modelPath = path.join(__dirname, '..', 'erd_model');
const viewerLibPath = path.join(__dirname, '..', 'erd_viewer');
const publicPath = path.join(__dirname, '..', 'public');

/** @type {Record<string, boolean>} */
const rebuildingRust = {};

const rebuildTex = rustRebuilder(
	modelPath,
	'cargo run --release --bin build_tex',
	touchMain);

const rebuildViewerLib = rustRebuilder(
	viewerLibPath,
	'wasm-pack build --target=web --no-default-features --release',
	copyWasm);

const shouldServe = process.argv[2] === '--server';

const config = {
	entryPoints: [
		'erd_app/main.ts',
	],
	entryNames: '[name]',
	bundle: true,
	format: 'esm',
	loader: { '.wasm': 'file' },
	target: 'esnext',
};

if (shouldServe) {
	chokidar.watch(modelPath, {
		ignored: [path.join(modelPath, 'target')],
	}).on('change', rebuildTex);

	chokidar.watch(viewerLibPath, {
		ignored: [path.join(viewerLibPath, 'target')],
	}).on('change', rebuildViewerLib);

	await rebuildTex();
	await rebuildViewerLib();
	startServer();

} else {
	const appConfig = getConfig();
	console.log(`Building Frontend for ${appConfig.stage}`);

	await rebuildTex();
	await rebuildViewerLib();
	await build({
		...config,
		write: false,
		outdir: 'dist',
		minify: true,
		sourcemap: false,
		platform: 'browser',
		plugins: [
			getCopyPlugin(),
			compress({ gzip: true }),
		],
	});
}

function startServer() {
	const server = createServer({
		...config,
		splitting: true,
		sourcemap: true,
		plugins: [getCopyPlugin()],
	}, {
		injectLiveReload: true,
		open: false,
		port: 8080,
		historyApiFallback: true,
		static: 'public',
	});

	server.start();
	console.log('🚀 Dev server running at http://localhost:8080');
}

function getCopyPlugin() {
	return copy({
		resolveFrom: 'cwd',
		assets: {
			from: ['./public/*'],
			to: ['./dist'],
		},
	});
}

/**
 * @param {string} projectPath
 * @param {string} cmdString
 * @param {async () => Promise<void>} andThen
 * @returns {() => Promise<void>}
 */
function rustRebuilder(projectPath, cmdString, andThen) {
	const name = path.basename(projectPath);
	const [cmd, ...args] = cmdString.split(' ');
	return () => {
		if (rebuildingRust[projectPath]) { return; }

		rebuildingRust[projectPath] = true;
		console.log(`🔧 Rebuilding Rust: ${name}…`);

		const build = spawn(cmd, args, {
			env: { ...process.env, RUST_LOG: 'warn' },
			stdio: 'inherit',
			cwd: projectPath,
		});

		let resolve = null;
		const promise = new Promise(r => { resolve = r; });
		build.on('exit', async function onExit() {
			rebuildingRust[projectPath] = false;
			console.log(`✅ Rust build done: ${name}`);
			await andThen();
			resolve();
		});

		return promise;
	};
}

async function copyWasm() {
	const files = await glob(path.join(viewerLibPath, 'pkg/erd_viewer_bg.*'));
	await Promise.all(files.map(file =>
		fs.copyFile(
			file,
			path.join(publicPath, path.basename(file))
		)));
}

async function touchMain() {
	await fs.writeFile(path.join(__dirname, '..', 'erd_app', 'build-date.ts'), `export const buildDate = '${new Date()}';\n`, { encoding: 'utf-8' });
}
