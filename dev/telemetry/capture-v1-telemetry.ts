import { appendFileSync } from 'node:fs';

const upstream =
  process.env['KIRO_OTLP_CAPTURE_UPSTREAM'] ?? 'http://127.0.0.1:4318';
const otlpCaptureFile = process.env['KIRO_OTLP_CAPTURE_FILE'];
const otlpPort = Number(process.env['KIRO_OTLP_CAPTURE_PORT'] ?? '14318');
const toolkitCaptureFile = process.env['KIRO_TOOLKIT_CAPTURE_FILE'];
const toolkitPort = Number(process.env['KIRO_TOOLKIT_CAPTURE_PORT'] ?? '14319');

if (!otlpCaptureFile) {
  throw new Error('KIRO_OTLP_CAPTURE_FILE is required');
}
if (!toolkitCaptureFile) {
  throw new Error('KIRO_TOOLKIT_CAPTURE_FILE is required');
}

Bun.serve({
  hostname: '127.0.0.1',
  port: otlpPort,
  async fetch(request) {
    const sourceUrl = new URL(request.url);
    if (sourceUrl.pathname === '/health') {
      return new Response('ok');
    }

    appendFileSync(
      otlpCaptureFile,
      `${JSON.stringify({
        method: request.method,
        path: sourceUrl.pathname,
        machine_id: request.headers.get('x-kiro-machineid'),
      })}\n`
    );

    const targetUrl = new URL(
      `${sourceUrl.pathname}${sourceUrl.search}`,
      upstream
    );
    const headers = new Headers(request.headers);
    headers.delete('content-length');
    headers.delete('host');
    const response = await fetch(targetUrl, {
      method: request.method,
      headers,
      body:
        request.method === 'GET' || request.method === 'HEAD'
          ? undefined
          : await request.arrayBuffer(),
    });
    return new Response(response.body, {
      status: response.status,
      headers: response.headers,
    });
  },
});

Bun.serve({
  hostname: '127.0.0.1',
  port: toolkitPort,
  async fetch(request) {
    const sourceUrl = new URL(request.url);
    if (sourceUrl.pathname === '/health') {
      return new Response('ok');
    }

    const payload: unknown = await request.json();
    const body =
      payload !== null && typeof payload === 'object'
        ? (payload as Record<string, unknown>)
        : {};
    const metricData = Array.isArray(body['MetricData'])
      ? body['MetricData']
      : [];
    const metricNames = metricData.flatMap((datum) => {
      if (datum === null || typeof datum !== 'object') {
        return [];
      }
      const metricName = (datum as Record<string, unknown>)['MetricName'];
      return typeof metricName === 'string' ? [metricName] : [];
    });

    appendFileSync(
      toolkitCaptureFile,
      `${JSON.stringify({
        method: request.method,
        path: sourceUrl.pathname,
        client_id:
          typeof body['ClientID'] === 'string' ? body['ClientID'] : null,
        metric_names: metricNames,
      })}\n`
    );

    return Response.json({});
  },
});
