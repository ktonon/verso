import { CfnOutput, Stack, StackProps } from 'aws-cdk-lib';
import * as acm from 'aws-cdk-lib/aws-certificatemanager';
import * as cloudfront from 'aws-cdk-lib/aws-cloudfront';
import * as origins from 'aws-cdk-lib/aws-cloudfront-origins';
import * as route53 from 'aws-cdk-lib/aws-route53';
import * as targets from 'aws-cdk-lib/aws-route53-targets';
import * as s3 from 'aws-cdk-lib/aws-s3';
import { Construct } from 'constructs';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { Stage } from './types';
import { toCamelCase } from './util';

export interface ProjectStackProps extends StackProps {
	projectName: string
	stage: Stage
	domainName: string
	certificateId: string
}

export class InfrastructureStack extends Stack {
	private certificateId: string;
	private rootDomainName: string;
	private projectName: string;
	private stage: Stage;

	constructor(scope: Construct, id: string, props: ProjectStackProps) {
		super(scope, id, props);
		this.projectName = props.projectName;
		this.stage = props.stage;
		this.certificateId = props.certificateId;
		this.rootDomainName = props.domainName;
	}

	scoped(connector: '_' | '-', ...parts: string[]): string {
		return [
			this.projectName,
			this.stage,
			...parts,
		].join(connector);
	}

	get domainName(): string {
		switch (this.stage) {
			case 'prod': return `${this.projectName}.${this.rootDomainName}`;
			default: return unhandled(this.stage);
		}
	}

	async setup() {
		const siteBucket = this.provisionBucket();
		const cert = this.findCert();

		const viewerReqFunc = await this.provisionCloudfrontFunction(`viewer-request-${this.stage}`);
		const distro = this.provisionDistribution(siteBucket, cert, viewerReqFunc);

		const zone = route53.HostedZone.fromLookup(this, 'zone', {
			domainName: this.rootDomainName,
		});

		new route53.ARecord(this, 'alias_record', {
			zone,
			target: route53.RecordTarget.fromAlias(new targets.CloudFrontTarget(distro)),
			recordName: this.stage === 'prod'
				? this.projectName
				: `${this.stage}.${this.projectName}`,
		});
	}

	findCert(): acm.ICertificate {
		const certRegion = 'us-east-1'; // CDK only works with certs in this region
		const certificateArn = `arn:aws:acm:${certRegion}:${this.account}:certificate/${this.certificateId}`;
		return acm.Certificate.fromCertificateArn(this, 'site_cert', certificateArn);
	}

	provisionBucket(): s3.Bucket {
		const bucket = new s3.Bucket(this, 'site_bucket', {
			bucketName: this.scoped('-', 'sitefiles'),
			blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
			autoDeleteObjects: false,
			versioned: false,
			websiteIndexDocument: 'index.html',
			websiteErrorDocument: 'error.html',
		});

		new CfnOutput(this, 'BucketName', {
			value: bucket.bucketName,
			exportName: `${this.projectName}${this.stage}BucketName`,
		});

		return bucket;
	}

	async provisionCloudfrontFunction(name: string): Promise<cloudfront.Function> {
		const filename = path.join(__dirname, 'cloudfront-functions', `${name}.js`);
		const inlineCode = await fs.readFile(filename, 'utf-8');

		const functionName = this.scoped('_', toCamelCase(name));
		return new cloudfront.Function(this, functionName, {
			functionName,
			code: cloudfront.FunctionCode.fromInline(inlineCode),
			runtime: cloudfront.FunctionRuntime.JS_2_0,
		});
	}

	provisionDistribution(bucket: s3.Bucket, certificate: acm.ICertificate, viewerRequestFunction: cloudfront.Function): cloudfront.Distribution {
		const distro = new cloudfront.Distribution(this, 'site_distribution', {
			defaultBehavior: {
				origin: origins.S3BucketOrigin.withOriginAccessControl(bucket),
				viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
				functionAssociations: [{
					eventType: cloudfront.FunctionEventType.VIEWER_REQUEST,
					function: viewerRequestFunction,
				}],
			},
			defaultRootObject: 'index.html',
			domainNames: [this.domainName],
			certificate,
		});

		new CfnOutput(this, 'DistributionId', {
			value: distro.distributionId,
			exportName: `${this.projectName}${this.stage}DistributionId`,
		});

		return distro;
	}
}

export function unhandled<T>(value: never): T {
	const unhandled: never = value;
	throw new Error(`Unhandled case: ${unhandled}`);
}
