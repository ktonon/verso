import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export function getConfig() {
	const stage = process.env.swm_stage ?? 'prod';
	if (stage !== 'prod') {
		throw new Error(`Invalid stage: ${stage}`);
	}

	const __dirname = dirname(fileURLToPath(import.meta.url));
	const filepath = join(__dirname, '.cdk-outputs.json');
	const content = readFileSync(filepath, { encoding: 'utf-8' });
	const outputs = JSON.parse(content)[`erd-${stage}-stack`];

	return {
		stage,
		region: 'ca-central-1',
		bucket: outputs.BucketName,
		distro: outputs.DistributionId,
	};
}
