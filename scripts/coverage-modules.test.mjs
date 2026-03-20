import test from 'node:test';
import assert from 'node:assert/strict';

import {
	parseCoverageSummary,
	selectModuleCoverage,
} from './coverage-modules.mjs';

test('parseCoverageSummary extracts file rows and line coverage', () => {
	const summary = `Filename                                      Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches   Missed Branches     Cover
---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
verso_doc/src/bin/verso.rs                       1221              1093    10.48%          90                76    15.56%         820               735    10.37%           0                 0         -
verso_training/src/policy_evaluate.rs             403                88    78.16%          14                 2    85.71%         265                55    79.25%           0                 0         -
TOTAL                                           39458              5802    85.30%        2186               258    88.20%       20045              3423    82.92%           0                 0         -
`;

	assert.deepEqual(parseCoverageSummary(summary), [
		{ file: 'verso_doc/src/bin/verso.rs', lineCover: 10.37 },
		{ file: 'verso_training/src/policy_evaluate.rs', lineCover: 79.25 },
	]);
});

test('selectModuleCoverage sorts lowest line coverage first', () => {
	const rows = [
		{ file: 'verso_symbolic/src/search.rs', lineCover: 90.07 },
		{ file: 'verso_training/src/ml_simplify.rs', lineCover: 0.0 },
		{ file: 'README.md', lineCover: 100.0 },
		{ file: 'verso_doc/src/bin/verso.rs', lineCover: 10.37 },
	];

	assert.deepEqual(selectModuleCoverage(rows, { limit: 2 }), [
		{ file: 'verso_training/src/ml_simplify.rs', lineCover: 0.0 },
		{ file: 'verso_doc/src/bin/verso.rs', lineCover: 10.37 },
	]);
});
