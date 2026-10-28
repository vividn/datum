// SigV4 for the S3 destination (#lib/s3.ts, da-pstzw), against AWS's own
// published S3 examples: «Examples of how to calculate an Authorization
// header» in the S3 API reference (sig-v4-header-based-auth), whose key is
// AKIAIOSFODNN7EXAMPLE and whose bucket is `examplebucket`. The signature
// in each is AWS's; a canonicalisation slip anywhere changes it.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { sign, corsRules } = await import('../src/lib/s3.ts');

const KEY = { key: 'AKIAIOSFODNN7EXAMPLE', secret: 'wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY', region: 'us-east-1' };
const HOST = 'https://examplebucket.s3.amazonaws.com';
const DATETIME = '20130524T000000Z';
const EMPTY = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
const scope = 'AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request';

const authorization = async (url, init) => (await sign(KEY, url, { datetime: DATETIME, ...init })).headers.get('authorization');

describe('SigV4, AWS’s S3 examples', () => {
	test('GET Object', async () => {
		// Range is signed in AWS's example; aws4fetch leaves it out unless told to.
		const auth = await authorization(`${HOST}/test.txt`, { headers: { range: 'bytes=0-9', 'x-amz-content-sha256': EMPTY }, allHeaders: true });
		assert.equal(
			auth,
			`AWS4-HMAC-SHA256 Credential=${scope}, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41`
		);
	});

	test('PUT Object', async () => {
		const auth = await authorization(`${HOST}/test$file.text`, {
			method: 'PUT',
			headers: {
				date: 'Fri, 24 May 2013 00:00:00 GMT',
				'x-amz-storage-class': 'REDUCED_REDUNDANCY',
				'x-amz-content-sha256': '44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072'
			},
			body: 'Welcome to Amazon S3.'
		});
		assert.equal(
			auth,
			`AWS4-HMAC-SHA256 Credential=${scope}, SignedHeaders=date;host;x-amz-content-sha256;x-amz-date;x-amz-storage-class, Signature=98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd`
		);
	});

	test('GET Bucket Lifecycle', async () => {
		const auth = await authorization(`${HOST}/?lifecycle`, { headers: { 'x-amz-content-sha256': EMPTY } });
		assert.equal(
			auth,
			`AWS4-HMAC-SHA256 Credential=${scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=fea454ca298b7da1c68078a5d1bdbfbbe0d65c699e0f91ac7a200a0136783543`
		);
	});

	test('Get Bucket (List Objects)', async () => {
		const auth = await authorization(`${HOST}/?max-keys=2&prefix=J`, { headers: { 'x-amz-content-sha256': EMPTY } });
		assert.equal(
			auth,
			`AWS4-HMAC-SHA256 Credential=${scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7`
		);
	});
});

test('the CORS rules allow this origin to GET, HEAD, PUT and DELETE', () => {
	assert.deepEqual(JSON.parse(corsRules('https://datum.day')), [
		{ AllowedOrigins: ['https://datum.day'], AllowedMethods: ['GET', 'HEAD', 'PUT', 'DELETE'], AllowedHeaders: ['*'], MaxAgeSeconds: 3000 }
	]);
});
