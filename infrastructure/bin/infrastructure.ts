#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';
import { InfrastructureStack } from '../lib/infrastructure-stack';

const projectName = 'erd';

const {
	erd_account: account,
	erd_region: region,
	erd_certificate_id: certificateId,
	erd_domain_name: domainName,
} = process.env;

const stage = process.env.erd_stage ?? 'prod';

if (stage !== 'prod') {
	throw new Error(`Invalid stage: ${stage}`);
}
if (!account || !certificateId || !domainName || !region) {
	throw new Error(`Please provide all of the following configuration (in the environment):

erd_account          The AWS account identifier.
erd_region           The AWS region.
erd_stage            The deployment stage (ex, prod).
erd_domain_name      A domain name for which there exists a hosted zone in AWS.
erd_certificate_id   An AWS ACM certificate id that is valid for the provided
                     domain name, and the sub-domain with name "erd".
					 This certificate must be created in the us-east-1 region,
					 even if the deployment region is something else.

`);
}

const app = new cdk.App();

const stack = new InfrastructureStack(app, `${projectName}-${stage}-stack`, {
	env: { account, region },
	projectName,
	stage,
	certificateId,
	domainName,
});

stack.setup();
