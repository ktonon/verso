import { spawnSync } from 'node:child_process';

export function parseCoverageSummary(text) {
	const rows = [];
	let inTable = false;

	for (const line of text.split('\n')) {
		if (!inTable) {
			if (line.startsWith('Filename')) {
				inTable = true;
			}
			continue;
		}

		if (!line.trim() || line.startsWith('---')) {
			continue;
		}

		if (line.startsWith('TOTAL')) {
			break;
		}

		const match = line.match(
			/^(?<file>\S+)\s+\d+\s+\d+\s+\S+\s+\d+\s+\d+\s+\S+\s+\d+\s+\d+\s+(?<lineCover>\S+)\s+\d+\s+\d+\s+\S+\s*$/
		);

		if (!match?.groups) {
			continue;
		}

		rows.push({
			file: match.groups.file,
			lineCover: Number.parseFloat(match.groups.lineCover.replace('%', '')),
		});
	}

	return rows;
}

export function selectModuleCoverage(rows, { limit = 12 } = {}) {
	return rows
		.filter((row) => row.file.includes('/src/') || row.file.includes('/bin/'))
		.sort((left, right) => left.lineCover - right.lineCover || left.file.localeCompare(right.file))
		.slice(0, limit);
}

function runCoverageSummary() {
	const result = spawnSync(
		'cargo',
		['llvm-cov', '--workspace', '--summary-only'],
		{
			encoding: 'utf8',
			stdio: 'pipe',
		}
	);

	if (result.status !== 0) {
		process.stderr.write(result.stdout);
		process.stderr.write(result.stderr);
		process.exit(result.status ?? 1);
	}

	return `${result.stdout}${result.stderr}`;
}

function formatModuleCoverage(rows) {
	const width = rows.reduce((max, row) => Math.max(max, row.file.length), 0);
	return rows
		.map((row) => `${row.file.padEnd(width)}  ${row.lineCover.toFixed(2).padStart(6)}%`)
		.join('\n');
}

if (import.meta.url === `file://${process.argv[1]}`) {
	const text = runCoverageSummary();
	const rows = selectModuleCoverage(parseCoverageSummary(text));

	if (rows.length === 0) {
		console.error('No module coverage rows found in cargo llvm-cov output.');
		process.exit(1);
	}

	console.log('Lowest line coverage modules:');
	console.log(formatModuleCoverage(rows));
}
