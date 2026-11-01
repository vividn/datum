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
		const auth = await authorization(`${HOST}/test.txt`, { headers: { range: 'bytes=0-9', 'x-amz-content-sha256': EMPTY } });
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
			}
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

// What AWS's examples leave out, each signature aws4fetch 1.0.20's, the signer
// this one replaced, on these requests: the escaping of a key, a `+` and a stray
// `%` in a path, a query out of order, the regions of the other providers.
describe('SigV4, as aws4fetch signed it', () => {
	for (const [method, url, region, signature] of [
		['PUT', "https://abc.r2.cloudflarestorage.com/backups/!'()*/a%2Bb%20c%26d%3De%3Ff%23g", 'auto', 'd59f7462862ec8c5800c7bcad48511d4c96ead7f1f5478ecb479f03847ceb74f'],
		['GET', 'https://s3.us-west-004.backblazeb2.com/backups/%C3%BC/%E6%97%A5.db', 'us-west-004', '9962b7ab3f1e648c2a4bc12c651008398fb3ce495d1d9a5e1d086423ebb6a64c'],
		['DELETE', 'https://proxy.example/a+b/100%zz', 'us-east-1', 'd8c9c767fb41c6cc95debda9f2e82d2479194a80855eb765785ad1436aaaa04f'],
		['HEAD', 'https://proxy.example/a+b/c%20d', 'us-east-1', 'db85b1205e667839ff1639e69b03cb827b35b1fb6de80b7c29ce018ccacf8995'],
		['GET', 'http://127.0.0.1:9000/backups?prefix=a%20b%2F%21client-&list-type=2&delimiter=%2F', 'us-east-1', '6657917e2872997701192deebf28e22914c154903f931b8ea8e6b4042a5fa033'],
		['GET', 'HTTPS://Host.Example:443/backups/x', 'us-east-1', 'd842a4fa5c5631b75e07c0e10cd80245899815617dce26a9b6bcb99564b0dbf2']
	]) {
		test(`${method} ${url}`, async () => {
			const signed = await sign({ ...KEY, region }, url, { method, datetime: DATETIME });
			assert.equal(signed.headers.get('authorization').split('Signature=')[1], signature);
		});
	}
});

test('the CORS rules allow this origin to GET, HEAD, PUT and DELETE', () => {
	assert.deepEqual(JSON.parse(corsRules('https://datum.day')), [
		{ AllowedOrigins: ['https://datum.day'], AllowedMethods: ['GET', 'HEAD', 'PUT', 'DELETE'], AllowedHeaders: ['*'], MaxAgeSeconds: 3000 }
	]);
});
