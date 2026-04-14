import type {
  IframeMessage,
  IframeRunRequest,
  IframeRunResponse,
  JsonValue,
  PlaygroundRunResult,
} from './iframe-protocol'
import type { PlaygroundScenario } from './scenarios'

const IFRAME_ORIGIN = '*'
const READY_TIMEOUT_MS = 8_000
const RUN_TIMEOUT_MS = 4_000

function isAllowedIframeOrigin(origin: string) {
  return origin === 'null' || origin === window.location.origin
}

function createRequestId() {
  return `playground-${crypto.randomUUID()}`
}

function requestIframeReady(iframe: HTMLIFrameElement) {
  iframe.contentWindow?.postMessage({ type: 'playground-ready-check' }, IFRAME_ORIGIN)
}

function isIframeResponse(message: unknown): message is IframeRunResponse {
  return (
    typeof message === 'object' &&
    message !== null &&
    'type' in message &&
    message.type === 'playground-result'
  )
}

function isIframeReady(message: unknown): message is { type: 'playground-ready' } {
  return (
    typeof message === 'object' &&
    message !== null &&
    'type' in message &&
    message.type === 'playground-ready'
  )
}

export async function ensureIframeReady(iframe: HTMLIFrameElement): Promise<void> {
  if (iframe.dataset.ready === 'true') {
    return
  }

  await new Promise<void>((resolve, reject) => {
    const cleanup = () => {
      window.clearTimeout(timeoutId)
      window.removeEventListener('message', onMessage)
      iframe.removeEventListener('load', onLoad)
    }

    const timeoutId = window.setTimeout(() => {
      cleanup()
      reject(new Error('sandboxed iframe did not become ready in time'))
    }, READY_TIMEOUT_MS)

    const onMessage = (event: MessageEvent<IframeMessage>) => {
      if (event.source !== iframe.contentWindow) {
        return
      }
      if (!isAllowedIframeOrigin(event.origin)) {
        return
      }
      if (!isIframeReady(event.data)) {
        return
      }
      iframe.dataset.ready = 'true'
      cleanup()
      resolve()
    }

    const onLoad = () => {
      requestIframeReady(iframe)
    }

    window.addEventListener('message', onMessage)
    iframe.addEventListener('load', onLoad)
    requestIframeReady(iframe)
  })
}

function resetIframe(iframe: HTMLIFrameElement) {
  iframe.dataset.ready = 'false'
  const currentSrc = iframe.src
  iframe.src = currentSrc
}

export async function runVanillaScenario(
  iframe: HTMLIFrameElement,
  scenario: PlaygroundScenario,
  code: string,
): Promise<PlaygroundRunResult> {
  await ensureIframeReady(iframe)

  const requestId = createRequestId()
  const request: IframeRunRequest = {
    type: 'playground-run',
    requestId,
    code,
    helperSources: scenario.helperSources,
    context: scenario.context,
    inputs: scenario.inputs as Record<string, JsonValue>,
  }

  return await new Promise<PlaygroundRunResult>((resolve) => {
    const timeoutId = window.setTimeout(() => {
      window.removeEventListener('message', onMessage)
      resetIframe(iframe)
      resolve({
        ok: false,
        elapsedMs: RUN_TIMEOUT_MS,
        error: {
          name: 'VanillaIframeTimeout',
          message: 'sandboxed iframe execution timed out',
        },
        trace: [],
      })
    }, RUN_TIMEOUT_MS)

    const onMessage = (event: MessageEvent<IframeMessage>) => {
      if (event.source !== iframe.contentWindow) {
        return
      }
      if (!isAllowedIframeOrigin(event.origin)) {
        return
      }
      if (!isIframeResponse(event.data) || event.data.requestId !== requestId) {
        return
      }
      window.clearTimeout(timeoutId)
      window.removeEventListener('message', onMessage)
      resolve({
        ok: event.data.ok,
        elapsedMs: event.data.elapsedMs,
        result: event.data.result,
        error: event.data.error,
        trace: [],
      })
    }

    window.addEventListener('message', onMessage)
    iframe.contentWindow?.postMessage(request, IFRAME_ORIGIN)
  })
}
