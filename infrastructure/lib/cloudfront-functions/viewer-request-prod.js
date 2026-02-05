// eslint-disable-next-line @typescript-eslint/no-unused-vars
function handler(event) {
	const request = event.request;

	if (!request.uri.match(/\.[^/]+$/)) {
		request.uri = '/index.html';
	}

	// Handle compression
	if (request.uri.endsWith('.js')) {
		const headers = request.headers;
		const acceptEncoding = headers['accept-encoding'] ? headers['accept-encoding'].value : '';
		if (acceptEncoding.includes('br')) {
			request.uri += '.br';
			request.headers['accept-encoding'] = { value: 'identity' };
		} else if (acceptEncoding.includes('gzip')) {
			request.uri += '.gz';
			request.headers['accept-encoding'] = { value: 'identity' };
		}
	}

	return request;
}
